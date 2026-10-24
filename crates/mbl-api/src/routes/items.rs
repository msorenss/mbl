//! Browse, detail, search and hubs.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use mbl_core::{Cursor, ItemId, LibraryId, MediaKind, Paginated};
use mbl_db::repo::items::{ItemDetail, ItemSummary, ListQuery, SortField, SortOrder};
use mbl_service::{Hub, SearchGroup};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries/{id}/items", get(list))
        .route("/api/v1/items/{id}", get(detail))
        .route("/api/v1/items/{id}/children", get(children))
        .route("/api/v1/search", get(search))
        .route("/api/v1/hubs/home", get(home))
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListParams {
    /// Item kind; defaults to the library's top-level items.
    pub kind: Option<MediaKind>,
    pub sort: Option<SortField>,
    pub order: Option<SortOrder>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub unwatched: Option<bool>,
    pub cursor: Option<String>,
    /// 1–200, default 50.
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    pub q: String,
}

/// Items of a library, paginated.
#[utoipa::path(get, path = "/api/v1/libraries/{id}/items", tag = "browse",
    params(("id" = i64, Path), ListParams),
    responses((status = 200, body = Paginated<ItemSummary>), (status = 404)))]
async fn list(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<LibraryId>,
    Query(p): Query<ListParams>,
) -> Result<Json<Paginated<ItemSummary>>, ApiError> {
    let query = ListQuery {
        kind: p.kind,
        sort: p.sort.unwrap_or_default(),
        order: p.order,
        genre: p.genre,
        year: p.year,
        unwatched: p.unwatched.unwrap_or(false),
        cursor: p.cursor.map(Cursor),
        limit: p.limit.unwrap_or(50),
    };
    Ok(Json(state.services.items.list(&auth.actor, id, &query).await?))
}

/// Item details with versions, streams, people, genres, artwork and watch state.
#[utoipa::path(get, path = "/api/v1/items/{id}", tag = "browse",
    params(("id" = i64, Path)),
    responses((status = 200, body = ItemDetail), (status = 404)))]
async fn detail(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<ItemDetail>, ApiError> {
    Ok(Json(state.services.items.get(&auth.actor, id).await?))
}

/// Seasons, episodes, albums or tracks under an item.
#[utoipa::path(get, path = "/api/v1/items/{id}/children", tag = "browse",
    params(("id" = i64, Path)),
    responses((status = 200, body = Vec<ItemSummary>), (status = 404)))]
async fn children(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<ItemSummary>>, ApiError> {
    Ok(Json(state.services.items.children(&auth.actor, id).await?))
}

/// Title search grouped by kind, at most 10 per group.
#[utoipa::path(get, path = "/api/v1/search", tag = "browse", params(SearchParams),
    responses((status = 200, body = Vec<SearchGroup>)))]
async fn search(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(p): Query<SearchParams>,
) -> Result<Json<Vec<SearchGroup>>, ApiError> {
    Ok(Json(state.services.items.search(&auth.actor, &p.q).await?))
}

/// Home screen hubs: Continue Watching, Up Next, Recently Added per library.
#[utoipa::path(get, path = "/api/v1/hubs/home", tag = "browse",
    responses((status = 200, body = Vec<Hub>)))]
async fn home(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
) -> Result<Json<Vec<Hub>>, ApiError> {
    Ok(Json(state.services.items.home(&auth.actor).await?))
}
