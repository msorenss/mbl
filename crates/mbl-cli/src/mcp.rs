//! `mbl-cli mcp --server <url>`: stdio ↔ Streamable HTTP MCP proxy.

use clap::Args;

#[derive(Debug, Clone, Args)]
pub struct McpArgs {
    /// Base URL or `/mcp` endpoint of the MBL server (e.g. `http://localhost:8420`).
    #[arg(long, default_value = "http://localhost:8420")]
    pub server: String,
}

pub async fn run(args: McpArgs) -> anyhow::Result<()> {
    let token =
        std::env::var("MBL_TOKEN").ok().map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    mbl_mcp::run_stdio_proxy(&args.server, token).await?;
    Ok(())
}
