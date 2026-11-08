use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use axum::extract::{MatchedPath, Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use utoipa::ToSchema;

const CAPACITY: usize = 1_000;

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerLogEntry {
    pub timestamp_ms: i64,
    pub level: String,
    pub target: String,
    pub message: String,
    pub details: BTreeMap<String, String>,
}

/// A bounded snapshot of recent operational events for the admin UI.
#[derive(Debug, Clone, Default)]
pub struct RecentLogs(Arc<Mutex<VecDeque<ServerLogEntry>>>);

impl RecentLogs {
    fn entries(&self) -> MutexGuard<'_, VecDeque<ServerLogEntry>> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn record(&self, entry: ServerLogEntry) {
        let mut entries = self.entries();
        if entries.len() == CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    pub(crate) fn snapshot(&self) -> Vec<ServerLogEntry> {
        self.entries().iter().rev().cloned().collect()
    }
}

/// Record failures and meaningful actions using route templates, never request URIs.
pub(crate) async fn log_http(
    State(state): State<crate::AppState>,
    req: Request,
    next: Next,
) -> Response {
    let route = req.extensions().get::<MatchedPath>().cloned();
    let method = match *req.method() {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        _ => "OTHER",
    };
    let started = Instant::now();
    let response = next.run(req).await;
    let status = response.status();
    if let Some(route) = route {
        let path = route.as_str();
        let action = matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
            && !["/progress", "/heartbeat", "/{file}"].iter().any(|suffix| path.ends_with(suffix));
        if path.starts_with("/api/")
            && path != "/api/v1/system/logs"
            && (action || status.is_client_error() || status.is_server_error())
        {
            let level = if status.is_server_error() {
                "error"
            } else if status.is_client_error() {
                "warn"
            } else {
                "info"
            };
            state.recent_logs.record(ServerLogEntry {
                timestamp_ms: mbl_core::time::now_ms(),
                level: level.to_owned(),
                target: "mbl_api::http".to_owned(),
                message: format!("{method} {path} → {}", status.as_u16()),
                details: BTreeMap::from([(
                    "elapsed_ms".to_owned(),
                    started.elapsed().as_millis().to_string(),
                )]),
            });
        }
    }
    response
}
