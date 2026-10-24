//! Developer automation. Run with `cargo xtask <command>`.

mod gen_media;

use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about = "MBL developer automation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate the synthetic test media library (needs ffmpeg + ffprobe on PATH).
    GenMedia(gen_media::Args),
    /// Fail if `docs/openapi.json` differs from the generated spec (`--write` updates it).
    OpenapiCheck {
        #[arg(long)]
        write: bool,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::GenMedia(args) => gen_media::run(&args),
        Command::OpenapiCheck { write } => openapi_check(write),
    }
}

fn openapi_check(write: bool) -> anyhow::Result<()> {
    let root = workspace_root();
    let out = std::process::Command::new(env!("CARGO"))
        .args(["run", "-q", "-p", "mbl-server", "--", "openapi"])
        .current_dir(&root)
        .output()?;
    anyhow::ensure!(
        out.status.success(),
        "mbl-server openapi failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = root.join("docs/openapi.json");
    if write {
        std::fs::write(&path, &out.stdout)?;
        eprintln!("wrote {path}");
        return Ok(());
    }
    let committed = std::fs::read(&path).unwrap_or_default();
    anyhow::ensure!(
        committed == out.stdout,
        "{path} is out of date; run `cargo xtask openapi-check --write` and commit it"
    );
    eprintln!("{path} is up to date");
    Ok(())
}

/// Workspace root (the parent of the `xtask` crate).
pub fn workspace_root() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Utf8Path::to_owned)
        .unwrap_or_else(|| Utf8PathBuf::from("."))
}
