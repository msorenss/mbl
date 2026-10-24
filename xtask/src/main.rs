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
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::GenMedia(args) => gen_media::run(&args),
    }
}

/// Workspace root (the parent of the `xtask` crate).
pub fn workspace_root() -> Utf8PathBuf {
    Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Utf8Path::to_owned)
        .unwrap_or_else(|| Utf8PathBuf::from("."))
}
