//! Photo libraries date timeline endpoint (M7.5).

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::LibraryId;
use mbl_db::repo::m7::{self, PhotoTimelineBucket};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries/{id}/timeline", get(get_photo_timeline))
        .route("/api/v1/libraries/{id}/photos", get(list_photos))
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
    let library = state.services.libraries.get(&auth.actor, LibraryId(id)).await?;
    if library.kind != mbl_core::LibraryKind::Photo {
        return Err(mbl_core::Error::NotFound.into());
    }
    let buckets = m7::get_photo_timeline(
        &state.db,
        LibraryId(id),
        &mbl_db::repo::items::ItemFilter::for_user(&auth.actor),
    )
    .await?;
    Ok(Json(buckets))
}

#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PhotoPageQuery {
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}
#[utoipa::path(get, path = "/api/v1/libraries/{id}/photos", params(("id" = i64, Path), PhotoPageQuery), responses((status = 200, body = mbl_core::Paginated<m7::PhotoSummary>), (status = 404), (status = 422)))]
pub(crate) async fn list_photos(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(query): Query<PhotoPageQuery>,
) -> Result<Json<mbl_core::Paginated<m7::PhotoSummary>>, ApiError> {
    let library = state.services.libraries.get(&auth.actor, LibraryId(id)).await?;
    if library.kind != mbl_core::LibraryKind::Photo {
        return Err(mbl_core::Error::NotFound.into());
    }
    Ok(Json(
        m7::list_photos(
            &state.db,
            LibraryId(id),
            &mbl_db::repo::items::ItemFilter::for_user(&auth.actor),
            query.year,
            query.month,
            query.cursor.as_deref(),
            query.limit.unwrap_or(60),
        )
        .await?,
    ))
}
