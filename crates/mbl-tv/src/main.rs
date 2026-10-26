use clap::Parser;
use mbl_platform::system::detect_profile;
use mbl_tv::{
    AudioPassthroughConfig, MpvConfig, TvAppState, discover_server, select_display_backend,
};

#[derive(Debug, Parser)]
#[command(name = "mbl-tv", version, about = "MBL 10-foot TV Client")]
struct Cli {
    /// MBL server URL (auto-discovered via mDNS / local probe if omitted).
    #[arg(long, env = "MBL_SERVER_URL")]
    server: Option<String>,

    /// Color theme (`dark`, `oled-black`, `light`, `high-contrast`).
    #[arg(long, default_value = "dark")]
    theme: String,

    /// Print resolved hardware, display backend, and libmpv options as JSON and exit.
    #[arg(long)]
    print_config: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let profile = detect_profile();
    let has_desktop =
        std::env::var_os("WAYLAND_DISPLAY").is_some() || std::env::var_os("DISPLAY").is_some();
    let backend = select_display_backend(profile, has_desktop);
    let audio = AudioPassthroughConfig::default();
    let mpv = MpvConfig::for_platform(profile, backend, true, &audio);

    let mut state = TvAppState::default();
    state.set_theme_by_name(&cli.theme);

    if cli.print_config {
        let out = serde_json::json!({
            "profile": profile,
            "displayBackend": backend,
            "theme": cli.theme,
            "mpv": mpv,
        });
        use std::io::Write;
        writeln!(std::io::stdout(), "{}", serde_json::to_string_pretty(&out)?)?;
        return Ok(());
    }

    let candidates = match cli.server {
        Some(url) => vec![url],
        None => vec!["http://mbl.local:8420".into(), "http://127.0.0.1:8420".into()],
    };
    if let Some(srv) = discover_server(&candidates).await {
        tracing::info!(name = %srv.name, url = %srv.url, version = %srv.version, "connected to MBL server");
    }

    Ok(())
}
