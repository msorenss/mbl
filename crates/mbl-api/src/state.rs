use std::sync::Arc;

use mbl_db::Db;
use mbl_platform::PlatformProfile;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub info: Arc<ServerInfo>,
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
