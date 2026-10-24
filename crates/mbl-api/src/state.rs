use std::sync::Arc;

use mbl_db::Db;
use mbl_platform::PlatformProfile;
use mbl_service::Services;
use serde::Serialize;

use crate::rate_limit::LoginLimiter;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub services: Services,
    pub info: Arc<ServerInfo>,
    pub login_limiter: Arc<LoginLimiter>,
    /// Add `Secure` to cookies (set when the public URL is HTTPS).
    pub secure_cookies: bool,
}

impl AppState {
    pub fn new(db: Db, jobs: mbl_jobs::JobQueue, info: ServerInfo, secure_cookies: bool) -> Self {
        Self {
            services: Services::new(db.clone(), jobs),
            db,
            info: Arc::new(info),
            login_limiter: Arc::new(LoginLimiter::default()),
            secure_cookies,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub os: &'static str,
    pub arch: &'static str,
    pub platform_profile: PlatformProfile,
}

impl ServerInfo {
    pub fn current(version: &'static str, platform_profile: PlatformProfile) -> Self {
        Self {
            name: "MBL",
            version,
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            platform_profile,
        }
    }
}
