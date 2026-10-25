//! Embedded web UI assets and SPA fallback handler.

use axum::http::header::{ACCEPT, CONTENT_TYPE};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
pub struct WebAssets;

/// Serve static assets embedded from `web/dist`, with SPA fallback to `index.html`
/// for browser navigations (requests accepting `text/html`).
pub async fn static_handler(uri: Uri, headers: HeaderMap) -> Response {
    let raw_path = uri.path().trim_start_matches('/');
    let path = if raw_path.is_empty() { "index.html" } else { raw_path };

    // 1. Try exact asset match
    if let Some(file) = WebAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return ([(CONTENT_TYPE, mime.as_ref())], file.data).into_response();
    }

    // 2. Reject API, health and MCP paths from falling back to index.html
    if raw_path.starts_with("api/") || raw_path == "health" || raw_path.starts_with("mcp") {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    // 3. SPA fallback to index.html if navigating (Accept: text/html) or root
    let accepts_html = headers
        .get(ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/html"))
        .unwrap_or(false);

    if accepts_html || path == "index.html" {
        return match WebAssets::get("index.html") {
            Some(index) => {
                ([(CONTENT_TYPE, "text/html; charset=utf-8")], index.data).into_response()
            }
            None => (StatusCode::NOT_FOUND, "Not found").into_response(),
        };
    }

    (StatusCode::NOT_FOUND, "Not found").into_response()
}
