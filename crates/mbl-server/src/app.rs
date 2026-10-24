use std::future::Future;
use std::sync::Arc;

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

    let info = ServerInfo::current(
        env!("CARGO_PKG_VERSION"),
        mbl_platform::system::detect_profile(),
    );
    let router = mbl_api::router(AppState {
        db: db.clone(),
        info: Arc::new(info),
    });

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
        axum::serve(listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await?;
        self.db.close().await;
        tracing::info!("stopped");
        Ok(())
    }
}
