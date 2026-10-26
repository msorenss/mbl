//! Photo libraries date timeline endpoint (M7.5).

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::LibraryId;
use mbl_db::repo::m7::{self, PhotoTimelineBucket};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/libraries/{id}/timeline", get(get_photo_timeline))
}

#[utoipa::path(
    get,
    path = "/api/v1/libraries/{id}/timeline",
    params(
        ("id" = i64, Path, description = "Library ID")
    ),
    responses(
        (status = 200, description = "Date timeline breakdown for photos", body = Vec<PhotoTimelineBucket>),
        (status = 404, description = "Library not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_photo_timeline(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<PhotoTimelineBucket>>, ApiError> {
    let lib_id = LibraryId(id);
    if !auth.actor.can_access_library(lib_id) {
        return Err(mbl_core::Error::NotFound.into());
    }

    let buckets = m7::get_photo_timeline(&state.db, lib_id).await?;
    Ok(Json(buckets))
}
