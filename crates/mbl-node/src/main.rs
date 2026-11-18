//! MBL storage node CLI: `pair`, `library add|list|remove`, `run`.
#![allow(clippy::print_stdout)] // A command-line tool reports to the terminal.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use mbl_core::{LibraryKind, NodeCapabilities, NodePairRequest, NodePairResponse};
use mbl_node::{NodeConfigFile, default_config_path};
use tokio_util::sync::CancellationToken;
use tracing::info;

#[derive(Parser)]
#[command(
    name = "mbl-node",
    version,
    about = "MBL storage node: share media folders with an MBL server"
)]
struct Cli {
    /// Configuration file (default: %ProgramData%\MBL\node.json on Windows, ./node.json elsewhere)
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pair this node with an MBL server using a one-time code (N-123456)
    Pair {
        /// Base URL of the MBL server, e.g. http://192.168.1.50:8420
        #[arg(long)]
        server: String,
        /// One-time pairing code from Administration -> Distributed Nodes
        #[arg(long)]
        code: String,
        /// Address the server uses to reach this node, e.g. http://192.168.1.60:8430
        #[arg(long)]
        base_url: String,
        /// Friendly name (defaults to the host name)
        #[arg(long)]
        name: Option<String>,
        /// Socket to listen on (default 0.0.0.0 and the port of --base-url)
        #[arg(long)]
        listen: Option<String>,
    },
    /// Manage shared folders
    Library {
        #[command(subcommand)]
        command: LibraryCommand,
    },
    /// Serve files and keep the server's catalog in sync
    Run {
        /// FFprobe binary (default: next to this program, then PATH)
        #[arg(long)]
        ffprobe: Option<String>,
        /// Minutes between catalog syncs
        #[arg(long, default_value_t = 5)]
        sync_minutes: u64,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// Share a folder as a library
    Add {
        /// Library name shown in MBL
        #[arg(long)]
        name: String,
        /// movie, show, music, photo or homevideo
        #[arg(long)]
        kind: String,
        /// Folder to share
        #[arg(long)]
        path: PathBuf,
    },
    /// List shared folders
    List,
    /// Stop sharing a folder (the server removes its items at the next sync)
    Remove {
        #[arg(long)]
        id: i64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(default_config_path);
    match cli.command {
        Commands::Pair { server, code, base_url, name, listen } => {
            pair(&config_path, server, code, base_url, name, listen).await
        }
        Commands::Library { command } => library(&config_path, command),
        Commands::Run { ffprobe, sync_minutes } => {
            let config = NodeConfigFile::load(&config_path)?;
            let tools = mbl_probe::FfTools::locate("", ffprobe.as_deref().unwrap_or(""))
                .await
                .context("FFmpeg/FFprobe not found; install FFmpeg or pass --ffprobe")?;
            let stop = CancellationToken::new();
            let on_signal = stop.clone();
            tokio::spawn(async move {
                let _ = tokio::signal::ctrl_c().await;
                info!("shutting down");
                on_signal.cancel();
            });
            mbl_node::run(config, tools, Duration::from_secs(sync_minutes.max(1) * 60), stop).await
        }
    }
}

fn library(config_path: &std::path::Path, command: LibraryCommand) -> Result<()> {
    let mut config = NodeConfigFile::load(config_path)?;
    match command {
        LibraryCommand::Add { name, kind, path } => {
            let kind: LibraryKind =
                serde_json::from_value(serde_json::Value::String(kind.to_ascii_lowercase()))
                    .context("kind must be movie, show, music, photo or homevideo")?;
            let id = config.add_library(&name, kind, &path)?;
            config.save(config_path)?;
            println!("Added library {id}: {name}. Restart `mbl-node run` to share it.");
        }
        LibraryCommand::List => {
            if config.libraries.is_empty() {
                println!("No shared folders. Add one with `mbl-node library add`.");
            }
            for l in &config.libraries {
                println!("{}\t{}\t{:?}\t{}", l.id, l.name, l.kind, l.path.display());
            }
        }
        LibraryCommand::Remove { id } => {
            let before = config.libraries.len();
            config.libraries.retain(|l| l.id != id);
            if config.libraries.len() == before {
                bail!("no library with id {id}");
            }
            config.save(config_path)?;
            println!("Removed library {id}. Restart `mbl-node run`.");
        }
    }
    Ok(())
}

async fn pair(
    config_path: &std::path::Path,
    server: String,
    code: String,
    base_url: String,
    name: Option<String>,
    listen: Option<String>,
) -> Result<()> {
    let hostname = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "storage-node".into());
    let name = name.unwrap_or_else(|| format!("Node-{hostname}"));
    let base_url = base_url.trim().trim_end_matches('/').to_owned();
    reqwest::Url::parse(&base_url)
        .context("--base-url must be a URL like http://192.168.1.60:8430")?;
    let server = server.trim().trim_end_matches('/').to_owned();
    let request = NodePairRequest {
        pairing_code: code,
        name: name.clone(),
        base_url: base_url.clone(),
        capabilities: NodeCapabilities::default(),
        client_cert_fingerprint: None,
    };
    let res = reqwest::Client::new()
        .post(format!("{server}/api/v1/nodes/pair"))
        .json(&request)
        .send()
        .await
        .context("cannot connect to the MBL server")?;
    if !res.status().is_success() {
        let status = res.status();
        bail!("server rejected pairing ({status}): {}", res.text().await.unwrap_or_default());
    }
    let paired: NodePairResponse = res.json().await.context("invalid pairing response")?;
    let Some(stream_secret) = paired.stream_secret else {
        bail!("the server is too old for storage nodes (needs MBL 1.3.9 or later)");
    };
    // Keep folders from an earlier pairing of this node.
    let libraries = NodeConfigFile::load(config_path).map(|c| c.libraries).unwrap_or_default();
    let config = NodeConfigFile {
        server_url: server,
        node_id: paired.node_id,
        auth_token: paired.auth_token,
        name,
        base_url,
        stream_secret: Some(stream_secret),
        listen,
        libraries,
    };
    config.save(config_path)?;
    println!(
        "Paired as node {}. Configuration: {}\nShare a folder with `mbl-node library add --name Movies --kind movie --path <folder>`, then start `mbl-node run`.",
        paired.node_id.0,
        config_path.display()
    );
    Ok(())
}
