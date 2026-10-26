//! MCP resources (`mbl://...`) and resource templates per M2.13 and PLAN §9.5.4.

use mbl_auth::tokens::Scope;
use mbl_core::{ItemId, JobId};
use mbl_db::Db;
use mbl_db::repo::jobs::JobsRepo;
use mbl_platform::PlatformProfile;
use mbl_service::Services;
use serde_json::json;

use crate::caller::Caller;
use crate::server::{
    JsonRpcError, ResourceContents, ResourceDefinition, ResourceTemplateDefinition,
};

pub const URI_SERVER_INFO: &str = "mbl://server/info";
pub const URI_SERVER_HEALTH: &str = "mbl://server/health";
pub const URI_SESSIONS_ACTIVE: &str = "mbl://sessions/active";
pub const URI_HW_REPORT: &str = "mbl://hw/report";
pub const TEMPLATE_ITEM: &str = "mbl://items/{id}";
pub const TEMPLATE_JOB: &str = "mbl://jobs/{id}";

pub fn list_resources() -> Vec<ResourceDefinition> {
    vec![
        ResourceDefinition {
            uri: URI_SERVER_INFO.into(),
            name: "Server Info".into(),
            description: Some("MBL server identity, OS, arch, and platform profile".into()),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: URI_SERVER_HEALTH.into(),
            name: "Server Health".into(),
            description: Some("System health, memory, and runtime stats".into()),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: URI_SESSIONS_ACTIVE.into(),
            name: "Active Playback Sessions".into(),
            description: Some("Currently active playback sessions".into()),
            mime_type: Some("application/json".into()),
        },
        ResourceDefinition {
            uri: URI_HW_REPORT.into(),
            name: "Hardware Acceleration Report".into(),
            description: Some(
                "Hardware fingerprint, NPU profile, Hailo status, capability matrix, and transcode capacity".into(),
            ),
            mime_type: Some("application/json".into()),
        },
    ]
}

pub fn list_resource_templates() -> Vec<ResourceTemplateDefinition> {
    vec![
        ResourceTemplateDefinition {
            uri_template: TEMPLATE_ITEM.into(),
            name: "Media Item".into(),
            description: Some("Metadata and version details for a media item by ID".into()),
            mime_type: Some("application/json".into()),
        },
        ResourceTemplateDefinition {
            uri_template: TEMPLATE_JOB.into(),
            name: "Background Job".into(),
            description: Some(
                "Status, progress, and error details for a background job by ID".into(),
            ),
            mime_type: Some("application/json".into()),
        },
    ]
}

pub async fn read_resource(
    uri: &str,
    caller: &Caller,
    db: &Db,
    services: &Services,
) -> Result<ResourceContents, JsonRpcError> {
    if caller.require(Scope::Read).is_err() {
        return Err(JsonRpcError::forbidden("reading resources requires 'read' scope"));
    }

    match uri {
        URI_SERVER_INFO => {
            let profile: PlatformProfile = mbl_platform::system::detect_profile();
            let npu_profile = services.hw.fingerprint().npu_profile;
            let body = json!({
                "name": "mbl",
                "version": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "platformProfile": profile,
                "npuProfile": npu_profile,
            });
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&body).unwrap_or_default(),
            })
        }
        URI_SERVER_HEALTH => {
            let active = services.playback.list_active(&caller.actor).len();
            let body = services.intelligence.system_health(active);
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&body).unwrap_or_default(),
            })
        }
        URI_SESSIONS_ACTIVE => {
            let sessions = services.playback.list_active(&caller.actor);
            let body = json!({ "sessions": sessions });
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&body).unwrap_or_default(),
            })
        }
        URI_HW_REPORT => {
            let report = services
                .hw
                .get_report(&caller.actor)
                .await
                .map_err(crate::server::map_service_error)?;
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&report).unwrap_or_default(),
            })
        }
        _ if uri.starts_with("mbl://items/") => {
            let id_str = uri.trim_start_matches("mbl://items/").trim_end_matches('/');
            let id: i64 = id_str.parse().map_err(|_| {
                JsonRpcError::invalid_params(format!("invalid item id: '{id_str}'"))
            })?;
            let item = services.items.get(&caller.actor, ItemId(id)).await.map_err(|e| {
                JsonRpcError::forbidden(format!("item not found or inaccessible: {e}"))
            })?;
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&item).unwrap_or_default(),
            })
        }
        _ if uri.starts_with("mbl://jobs/") => {
            let id_str = uri.trim_start_matches("mbl://jobs/").trim_end_matches('/');
            let id: i64 = id_str
                .parse()
                .map_err(|_| JsonRpcError::invalid_params(format!("invalid job id: '{id_str}'")))?;
            let repo = JobsRepo::new(db.clone());
            let job = repo
                .get(JobId(id))
                .await
                .map_err(|e| JsonRpcError::internal(e.to_string()))?
                .ok_or_else(|| JsonRpcError::invalid_params(format!("job {id} not found")))?;
            Ok(ResourceContents {
                uri: uri.into(),
                mime_type: Some("application/json".into()),
                text: serde_json::to_string_pretty(&job).unwrap_or_default(),
            })
        }
        _ => Err(JsonRpcError::invalid_params(format!("unknown resource URI: '{uri}'"))),
    }
}
