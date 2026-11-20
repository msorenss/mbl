//! Running an agent program (Claude Code, Hermes…) as the assistant.
//!
//! The admin configures a command line. For each answer MBL creates a short-lived API token
//! with the assistant's scopes for the person asking (read only unless the admin's role
//! settings allow more), writes an MCP client configuration with it, runs
//! the command with the prompt on stdin and takes stdout as the answer. The token is revoked
//! and the configuration removed afterwards. The token never appears in the command line:
//! it is in the configuration file and in `MBL_MCP_TOKEN`.
//!
//! Placeholders in the command: `{mcp_config}` (path of the configuration file) and
//! `{mcp_url}`. The program also gets `MBL_MCP_URL`, `MBL_MCP_TOKEN` and `MBL_MCP_CONFIG`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, Result};
use mbl_service::Services;
use mbl_service::auth::CreateApiTokenRequest;
use serde_json::json;
use tokio::io::AsyncWriteExt;

/// Longest an agent program may run for one answer.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
/// The temporary token outlives the run a little in case cleanup is interrupted.
const TOKEN_LIFETIME_MS: i64 = 15 * 60_000;
const MAX_OUTPUT_CHARS: usize = 8000;

/// What one run produced.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub text: String,
    /// The temporary token's ID, to look up which tools the program used.
    pub token_id: i64,
}

/// Splits a command line into words. Single and double quotes group words; there is no
/// shell, so `|`, `;`, `$(…)` and the like have no special meaning.
pub fn split_command(line: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, c) => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err(Error::Validation("The command has an unclosed quote".into()));
    }
    if in_word {
        words.push(current);
    }
    if words.is_empty() {
        return Err(Error::Validation("Enter the command".into()));
    }
    Ok(words)
}

/// Runs `command` once with `prompt` on stdin, as `actor`.
pub async fn run_command(
    services: &Services,
    actor: &Actor,
    scopes: &mbl_auth::tokens::ScopeSet,
    command: &str,
    mcp_url: &str,
    prompt: &str,
) -> Result<CommandOutput> {
    let words = split_command(command)?;
    let created = services
        .auth
        .create_api_token(
            actor,
            CreateApiTokenRequest {
                name: "Assistant command (temporary)".into(),
                scopes: scopes.to_strings(),
                expires_at: Some(now_ms() + TOKEN_LIFETIME_MS),
                allowed_networks: None,
            },
        )
        .await?;
    let config_path = config_path()?;
    let outcome = async {
        write_config(&config_path, mcp_url, &created.token)?;
        let config = config_path.to_string_lossy().into_owned();
        let args: Vec<String> = words
            .iter()
            .skip(1)
            .map(|w| w.replace("{mcp_config}", &config).replace("{mcp_url}", mcp_url))
            .collect();
        let mut child = tokio::process::Command::new(&words[0])
            .args(&args)
            .env("MBL_MCP_URL", mcp_url)
            .env("MBL_MCP_TOKEN", &created.token)
            .env("MBL_MCP_CONFIG", &config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::External(format!("Could not start '{}': {e}", words[0])))?;
        if let Some(mut stdin) = child.stdin.take() {
            // A program that does not read stdin closes the pipe; that is fine.
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
        let output = tokio::time::timeout(COMMAND_TIMEOUT, child.wait_with_output())
            .await
            .map_err(|_| Error::External("The command did not finish in time".into()))?
            .map_err(|e| Error::External(format!("The command failed: {e}")))?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default();
            return Err(Error::External(format!(
                "The command failed ({}){}",
                output.status,
                if reason.is_empty() { String::new() } else { format!(": {}", reason.trim()) }
            )));
        }
        if stdout.is_empty() {
            return Err(Error::External("The command printed no answer".into()));
        }
        Ok(stdout.chars().take(MAX_OUTPUT_CHARS).collect::<String>())
    }
    .await;
    // Clean up whatever happened.
    let _ = std::fs::remove_file(&config_path);
    if let Err(error) = services.auth.revoke_api_token(actor, created.info.id).await {
        tracing::warn!(%error, "could not revoke the assistant command's token");
    }
    outcome.map(|text| CommandOutput { text, token_id: created.info.id })
}

fn config_path() -> Result<PathBuf> {
    let mut bytes = [0u8; 12];
    getrandom::fill(&mut bytes).map_err(|e| Error::Internal(e.to_string().into()))?;
    let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(std::env::temp_dir().join(format!("mbl-assistant-{name}.json")))
}

/// An MCP client configuration (the `mcpServers` format Claude Code and others read), only
/// readable by the server's user.
fn write_config(path: &std::path::Path, url: &str, token: &str) -> Result<()> {
    let config = json!({
        "mcpServers": {
            "mbl": {
                "type": "http",
                "url": url,
                "headers": { "Authorization": format!("Bearer {token}") }
            }
        }
    });
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(Error::internal)?;
    std::io::Write::write_all(&mut file, config.to_string().as_bytes()).map_err(Error::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_split_without_a_shell() {
        assert_eq!(
            split_command(r#"claude -p --mcp-config {mcp_config} --allowedTools "mcp__mbl" 'a b'"#)
                .unwrap(),
            ["claude", "-p", "--mcp-config", "{mcp_config}", "--allowedTools", "mcp__mbl", "a b"]
        );
        assert_eq!(
            split_command("echo $(id); rm -rf x").unwrap(),
            ["echo", "$(id);", "rm", "-rf", "x"]
        );
        assert_eq!(split_command(r#"x """#).unwrap(), ["x", ""]);
        assert!(split_command("  ").is_err());
        assert!(split_command("echo 'open").is_err());
    }
}
