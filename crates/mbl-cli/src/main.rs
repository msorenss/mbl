//! MBL admin command-line tool. Subcommands are added as their milestones land (e.g. `mcp` in M2.11).

pub mod mcp;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "mbl-cli", version, about = "MBL admin command-line tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a stdio ↔ Streamable HTTP MCP proxy (`MBL_TOKEN` read from environment).
    Mcp(mcp::McpArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Mcp(args)) => mcp::run(args).await?,
        None => {}
    }
    Ok(())
}
