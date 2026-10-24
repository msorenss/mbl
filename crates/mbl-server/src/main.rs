use std::io::Write;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use mbl_server::{CliOverrides, Config};

#[derive(Debug, Parser)]
#[command(name = "mbl-server", version, about = "MBL media server")]
struct Cli {
    /// Path to the TOML config file.
    #[arg(long, global = true)]
    config: Option<Utf8PathBuf>,
    /// Override the data directory.
    #[arg(long, global = true)]
    data_dir: Option<Utf8PathBuf>,
    /// Override the HTTP port.
    #[arg(long, global = true)]
    port: Option<u16>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the server (default).
    Run,
    /// Print the OpenAPI document as JSON (stub until M1.9).
    Openapi,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.as_ref().unwrap_or(&Command::Run) {
        Command::Openapi => print_openapi(),
        Command::Run => {
            let overrides =
                CliOverrides { config_file: cli.config, data_dir: cli.data_dir, port: cli.port };
            let config = Config::load(&overrides)?;
            let paths = config.paths();
            let _log_guard = mbl_server::logging::init(&config.log, &paths.logs)?;
            tracing::info!(version = env!("CARGO_PKG_VERSION"), data = %paths.data, "starting MBL");

            tokio::runtime::Builder::new_multi_thread().enable_all().build()?.block_on(async {
                let server = mbl_server::build(config).await?;
                server.serve(mbl_server::shutdown::signal()).await
            })
        }
    }
}

fn print_openapi() -> anyhow::Result<()> {
    let doc = serde_json::json!({
        "openapi": "3.1.0",
        "info": { "title": "MBL API", "version": env!("CARGO_PKG_VERSION") },
        "paths": {}
    });
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, &doc)?;
    writeln!(out)?;
    Ok(())
}
