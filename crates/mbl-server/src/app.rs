use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mbl_api::{AppState, ServerInfo};
use mbl_core::events::EventBus;
use mbl_db::{Db, DbOptions};
use mbl_jobs::{JobRunner, RunnerConfig};
use mbl_platform::PlatformProfile;
use mbl_scanner::{ScanHandler, Scanner};
use tokio_util::sync::CancellationToken;

use crate::config::{Config, Paths};

/// A fully wired server, ready to serve.
pub struct Server {
    pub config: Config,
    pub paths: Paths,
    pub db: Db,
    pub events: EventBus,
    pub router: Router,
    runner: JobRunner,
}

/// Create directories, open the database, register background jobs and build the router.
pub async fn build(config: Config) -> anyhow::Result<Server> {
    let paths = config.paths();
    for dir in [&paths.data, &paths.cache, &paths.transcode] {
        std::fs::create_dir_all(dir)?;
    }

    let cpus = std::thread::available_parallelism().map(usize::from).unwrap_or(2);
    let db_opts = DbOptions::for_memory(mbl_platform::system::total_memory_bytes(), cpus);
    let db = Db::connect(&paths.db_file, &db_opts).await?;
    let profile = mbl_platform::system::detect_profile();

    let tools = match mbl_probe::FfTools::locate(
        &config.transcode.ffmpeg_path,
        &config.transcode.ffprobe_path,
    )
    .await
    {
        Ok(tools) => {
            tracing::info!(version = %tools.version().await.unwrap_or_default(), ffprobe = ?tools.ffprobe, "FFmpeg found");
            Some(tools)
        }
        Err(e) => {
            tracing::warn!(error = %e, "FFmpeg not available: scanning and playback will not work");
            None
        }
    };

    let events = EventBus::default();
    let mut runner =
        JobRunner::new(db.clone(), events.clone(), RunnerConfig { profile, ..Default::default() });
    let probe_concurrency = match config.scanner.probe_concurrency {
        0 => default_probe_concurrency(profile, cpus),
        n => usize::try_from(n).unwrap_or(1),
    };
    let scanner =
        Scanner::new(db.clone(), tools, events.clone(), Some(runner.queue()), probe_concurrency);
    runner.register(Arc::new(ScanHandler { scanner }));

    let tmdb = match config.metadata.tmdb_api_key.trim() {
        "" => None,
        key => {
            let mut tmdb = mbl_metadata::TmdbConfig::new(key, config.metadata.language.clone());
            tmdb.cache_dir = Some(paths.cache.join("http/tmdb").into_std_path_buf());
            Some(mbl_metadata::TmdbClient::new(tmdb)?)
        }
    };
    let matcher = mbl_metadata::Matcher::new(db.clone(), tmdb, events.clone());
    runner.register(Arc::new(mbl_metadata::MatchHandler { matcher }));
    runner
        .register(Arc::new(mbl_metadata::RefreshHandler { db: db.clone(), jobs: runner.queue() }));

    let info = ServerInfo::current(env!("CARGO_PKG_VERSION"), profile);
    let secure_cookies = config.server.public_url.starts_with("https://");
    let router = mbl_api::router(AppState::new(db.clone(), runner.queue(), info, secure_cookies));

    Ok(Server { config, paths, db, events, router, runner })
}

fn default_probe_concurrency(profile: PlatformProfile, cpus: usize) -> usize {
    match profile {
        PlatformProfile::Pi4 | PlatformProfile::Pi5 => 2,
        _ => (cpus / 2).max(1),
    }
}

impl Server {
    /// Start background jobs, bind and serve until `shutdown` resolves, then close the database.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let stop = CancellationToken::new();
        let jobs = self.runner.start(stop.clone()).await?;

        let addr = format!("{}:{}", self.config.server.bind, self.config.server.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!(%addr, "listening");
        let stop_on_signal = stop.clone();
        axum::serve(listener, self.router.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(async move {
                shutdown.await;
                stop_on_signal.cancel();
            })
            .await?;

        stop.cancel();
        let _ = jobs.await;
        self.db.close().await;
        tracing::info!("stopped");
        Ok(())
    }

    /// Start the job runner without serving HTTP (used by integration tests).
    pub async fn start_jobs(self, stop: CancellationToken) -> anyhow::Result<(Router, Db)> {
        self.runner.start(stop).await?;
        Ok((self.router, self.db))
    }
}
