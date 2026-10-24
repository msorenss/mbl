use std::future::Future;
use std::net::SocketAddr;

use axum::Router;
use mbl_api::{AppState, ServerInfo};
use mbl_db::{Db, DbOptions};

use crate::config::{Config, Paths};

/// A fully wired server, ready to serve.
pub struct Server {
    pub config: Config,
    pub paths: Paths,
    pub db: Db,
    pub router: Router,
}

/// Create directories, open the database and build the router.
pub async fn build(config: Config) -> anyhow::Result<Server> {
    let paths = config.paths();
    for dir in [&paths.data, &paths.cache, &paths.transcode] {
        std::fs::create_dir_all(dir)?;
    }

    let cpus = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(2);
    let db_opts = DbOptions::for_memory(mbl_platform::system::total_memory_bytes(), cpus);
    let db = Db::connect(&paths.db_file, &db_opts).await?;

    match mbl_probe::FfTools::locate(
        &config.transcode.ffmpeg_path,
        &config.transcode.ffprobe_path,
    )
    .await
    {
        Ok(tools) => {
            tracing::info!(version = %tools.version().await.unwrap_or_default(), ffprobe = ?tools.ffprobe, "FFmpeg found")
        }
        Err(e) => {
            tracing::warn!(error = %e, "FFmpeg not available: scanning and playback will not work")
        }
    }

    let info = ServerInfo::current(
        env!("CARGO_PKG_VERSION"),
        mbl_platform::system::detect_profile(),
    );
    let secure_cookies = config.server.public_url.starts_with("https://");
    let router = mbl_api::router(AppState::new(db.clone(), info, secure_cookies));

    Ok(Server {
        config,
        paths,
        db,
        router,
    })
}

impl Server {
    /// Bind and serve until `shutdown` resolves, then close the database.
    pub async fn serve(
        self,
        shutdown: impl Future<Output = ()> + Send + 'static,
    ) -> anyhow::Result<()> {
        let addr = format!("{}:{}", self.config.server.bind, self.config.server.port);
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        tracing::info!(%addr, "listening");
        axum::serve(
            listener,
            self.router
                .into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown)
        .await?;
        self.db.close().await;
        tracing::info!("stopped");
        Ok(())
    }
}
