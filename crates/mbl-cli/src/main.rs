//! MBL admin command-line tool. Subcommands are added as their milestones land (e.g. `mcp` in M2.11).

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "mbl-cli", version, about = "MBL admin command-line tool")]
struct Cli {}

fn main() -> anyhow::Result<()> {
    let _cli = Cli::parse();
    Ok(())
}
