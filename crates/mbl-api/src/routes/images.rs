use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use mbl_core::ArtworkId;
use serde::Deserialize;

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/images/{id}", get(image))
}

#[derive(Debug, Deserialize)]
pub struct ImageQuery {
    pub w: Option<u32>,
    pub q: Option<u8>,
    /// Content hash from the item DTO; when it matches, the response is cached as immutable.
    pub v: Option<String>,
}

async fn image(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ArtworkId>,
    Query(q): Query<ImageQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let file =
        state.services.images.get(&auth.actor, id, q.w.unwrap_or(480), q.q.unwrap_or(80)).await?;
    let etag = format!("\"{}-{}-{}\"", file.hash, file.width, file.quality);
    let cache = if q.v.as_deref() == Some(file.hash.as_str()) {
        "private, max-age=31536000, immutable"
    } else {
        "private, max-age=3600"
    };
    let mut response =
        if headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok()) == Some(&etag) {
            StatusCode::NOT_MODIFIED.into_response()
        } else {
            let bytes = tokio::fs::read(&file.path).await.map_err(mbl_core::Error::from)?;
            let mut r = Body::from(bytes).into_response();
            r.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
            r
        };
    let h = response.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    Ok(response)
}
