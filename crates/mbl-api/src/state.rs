use std::sync::Arc;

use mbl_db::Db;
use mbl_hwaccel::NpuProfile;
use mbl_platform::PlatformProfile;
use mbl_service::Services;
use serde::Serialize;

use crate::logs::RecentLogs;
use crate::rate_limit::LoginLimiter;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub services: Services,
    pub info: Arc<ServerInfo>,
    pub login_limiter: Arc<LoginLimiter>,
    pub recent_logs: RecentLogs,
    pub stop: tokio_util::sync::CancellationToken,
    /// Add `Secure` to cookies (set when the public URL is HTTPS).
    pub secure_cookies: bool,
}

impl AppState {
    pub fn new(
        db: Db,
        jobs: mbl_jobs::JobQueue,
        images: mbl_images::ImageStore,
        events: mbl_core::events::EventBus,
        info: ServerInfo,
        secure_cookies: bool,
    ) -> Self {
        Self {
            services: Services::new(db.clone(), jobs, images, events),
            db,
            info: Arc::new(info),
            login_limiter: Arc::new(LoginLimiter::default()),
            recent_logs: RecentLogs::default(),
            stop: tokio_util::sync::CancellationToken::new(),
            secure_cookies,
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub git_commit: &'static str,
    pub build_time: &'static str,
    pub started_at: i64,
    pub os: &'static str,
    pub arch: &'static str,
    pub platform_profile: PlatformProfile,
    pub npu_profile: NpuProfile,
    /// Stable random identifier of this server installation (persisted in settings).
    pub server_id: String,
    #[serde(skip)]
    pub listen: ListenInfo,
}

/// Where the HTTP listener is reachable; used to advertise client endpoints.
#[derive(Debug, Clone, Default)]
pub struct ListenInfo {
    pub port: u16,
    pub public_url: String,
}

impl ServerInfo {
    pub fn current(
        version: &'static str,
        git_commit: &'static str,
        build_time: &'static str,
        platform_profile: PlatformProfile,
    ) -> Self {
        let npu_profile = mbl_hwaccel::detect_fingerprint().npu_profile;
        Self {
            name: "MBL",
            version,
            git_commit,
            build_time,
            started_at: mbl_core::time::now_ms(),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            platform_profile,
            npu_profile,
            server_id: String::new(),
            listen: ListenInfo::default(),
        }
    }

    /// Attach the persisted server id and HTTP listener details.
    #[must_use]
    pub fn with_network(mut self, server_id: String, port: u16, public_url: String) -> Self {
        self.server_id = server_id;
        self.listen = ListenInfo { port, public_url };
        self
    }
}
