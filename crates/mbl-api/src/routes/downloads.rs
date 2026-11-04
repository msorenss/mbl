//! Offline downloads for client devices (M11.1 S4).

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::ItemId;
use mbl_service::{CreateDownloadRequest, DownloadDto};

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/items/{id}/downloads", post(create))
        .route("/api/v1/downloads", get(list))
        .route("/api/v1/downloads/{id}", get(get_one).delete(remove))
        .route("/api/v1/downloads/{id}/file", get(file))
}

/// Request an offline copy of an item. Idempotent per part and quality; optimized
/// qualities are transcoded in the background (poll the ticket until `ready`).
#[utoipa::path(post, path = "/api/v1/items/{id}/downloads", tag = "downloads",
    operation_id = "create_download", params(("id" = i64, Path)),
    request_body = CreateDownloadRequest,
    responses((status = 200, body = DownloadDto), (status = 401), (status = 404),
        (status = 503, description = "Transcoded downloads need a cache directory")))]
pub(crate) async fn create(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
    Json(req): Json<CreateDownloadRequest>,
) -> Result<Json<DownloadDto>, ApiError> {
    Ok(Json(state.services.downloads.create(&auth.actor, id, req).await?))
}

/// The caller's download tickets, newest first.
#[utoipa::path(get, path = "/api/v1/downloads", tag = "downloads", operation_id = "list_downloads",
    responses((status = 200, body = Vec<DownloadDto>), (status = 401)))]
pub(crate) async fn list(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<DownloadDto>>, ApiError> {
    Ok(Json(state.services.downloads.list(&auth.actor).await?))
}

/// One of the caller's download tickets.
#[utoipa::path(get, path = "/api/v1/downloads/{id}", tag = "downloads",
    operation_id = "get_download", params(("id" = i64, Path)),
    responses((status = 200, body = DownloadDto), (status = 401), (status = 404)))]
pub(crate) async fn get_one(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<Json<DownloadDto>, ApiError> {
    Ok(Json(state.services.downloads.get(&auth.actor, id).await?))
}

/// Delete a download ticket. The cached transcode is removed when unused.
#[utoipa::path(delete, path = "/api/v1/downloads/{id}", tag = "downloads",
    operation_id = "delete_download", params(("id" = i64, Path)),
    responses((status = 204), (status = 401), (status = 404)))]
pub(crate) async fn remove(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.downloads.delete(&auth.actor, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Fetch the file of a ready download (supports `Range` and `HEAD` for resumable transfers).
#[utoipa::path(get, path = "/api/v1/downloads/{id}/file", tag = "downloads",
    operation_id = "download_file", params(("id" = i64, Path)),
    responses((status = 200, description = "File body"), (status = 206, description = "Partial content"),
        (status = 401), (status = 404), (status = 409, description = "Download not ready"),
        (status = 416, description = "Range not satisfiable")))]
pub(crate) async fn file(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    method: Method,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let resolved = state.services.downloads.resolve_file(&auth.actor, id).await?;
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    let mut res = mbl_stream::serve_direct_file(
        &resolved.path,
        resolved.container.as_deref(),
        range,
        method == Method::HEAD,
    )
    .await?;
    if let Ok(v) = HeaderValue::from_str(&content_disposition(&resolved.file_name)) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

/// `attachment` with an ASCII fallback and an RFC 5987 UTF-8 name.
fn content_disposition(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if (c.is_ascii_graphic() && c != '"' && c != '\\') || c == ' ' { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::content_disposition;

    #[test]
    fn content_disposition_escapes_non_ascii_and_quotes() {
        assert_eq!(
            content_disposition("Så \"bra\".mkv"),
            "attachment; filename=\"S_ _bra_.mkv\"; filename*=UTF-8''S%C3%A5%20%22bra%22.mkv"
        );
    }
}
