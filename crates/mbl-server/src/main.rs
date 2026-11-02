use std::io::Write;

use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use mbl_server::winservice::ServiceAction;
use mbl_server::{CliOverrides, winservice};

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
    /// Run under the Windows Service Control Manager (SCM) dispatcher.
    #[arg(long, global = true)]
    service: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the server (default).
    Run,
    /// Print the OpenAPI document as JSON.
    Openapi,
    /// Manage the Windows background service (install, uninstall, start, stop).
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let overrides =
        CliOverrides { config_file: cli.config, data_dir: cli.data_dir, port: cli.port };

    match cli.command.as_ref().unwrap_or(&Command::Run) {
        Command::Openapi => print_openapi(),
        Command::Service { action } => winservice::handle_service_command(action, overrides),
        Command::Run => winservice::run_server(overrides, cli.service),
    }
}

fn print_openapi() -> anyhow::Result<()> {
    std::io::stdout().lock().write_all(mbl_api::openapi::spec_json().as_bytes())?;
    Ok(())
}
