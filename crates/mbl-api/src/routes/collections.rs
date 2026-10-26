//! Collections REST endpoints (M7.6).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::{CollectionId, ItemId};
use mbl_service::{CollectionRow, CreateCollectionRequest, UpdateCollectionRequest};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/collections", get(list_collections).post(create_collection))
        .route(
            "/api/v1/collections/{id}",
            get(get_collection).patch(update_collection).delete(delete_collection),
        )
        .route("/api/v1/collections/{id}/items", get(get_collection_items))
        .route(
            "/api/v1/collections/{id}/items/{itemId}",
            post(add_item_to_collection).delete(remove_item_from_collection),
        )
}

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    responses(
        (status = 200, description = "List all collections", body = Vec<CollectionRow>),
    ),
    security(("bearer_auth" = []))
)]
async fn list_collections(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<CollectionRow>>, ApiError> {
    let list = state.services.collections.list_collections(&auth.actor).await?;
    Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CreateCollectionRequest,
    responses(
        (status = 201, description = "Collection created", body = CollectionRow),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn create_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<CollectionRow>), ApiError> {
    let row = state.services.collections.create_collection(&auth.actor, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection ID")
    ),
    responses(
        (status = 200, description = "Collection detail", body = CollectionRow),
        (status = 404, description = "Collection not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<CollectionRow>, ApiError> {
    let row = state
        .services
        .collections
        .get_collection(&auth.actor, CollectionId(id))
        .await?
        .ok_or(mbl_core::Error::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(
    patch,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection ID")
    ),
    request_body = UpdateCollectionRequest,
    responses(
        (status = 204, description = "Collection updated"),
        (status = 403, description = "Admin required"),
        (status = 404, description = "Collection not found")
    ),
    security(("bearer_auth" = []))
)]
async fn update_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateCollectionRequest>,
) -> Result<StatusCode, ApiError> {
    state.services.collections.update_collection(&auth.actor, CollectionId(id), body).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}",
    params(
        ("id" = i64, Path, description = "Collection ID")
    ),
    responses(
        (status = 204, description = "Collection deleted"),
        (status = 403, description = "Admin required"),
        (status = 404, description = "Collection not found")
    ),
    security(("bearer_auth" = []))
)]
async fn delete_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    state.services.collections.delete_collection(&auth.actor, CollectionId(id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/items",
    params(
        ("id" = i64, Path, description = "Collection ID")
    ),
    responses(
        (status = 200, description = "Item IDs in collection", body = Vec<i64>),
    ),
    security(("bearer_auth" = []))
)]
async fn get_collection_items(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<i64>>, ApiError> {
    let items = state.services.collections.get_items(&auth.actor, CollectionId(id)).await?;
    Ok(Json(items.into_iter().map(|it| it.0).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/items/{itemId}",
    params(
        ("id" = i64, Path, description = "Collection ID"),
        ("itemId" = i64, Path, description = "Media Item ID")
    ),
    responses(
        (status = 204, description = "Item added to collection"),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn add_item_to_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, item_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    state.services.collections.add_item(&auth.actor, CollectionId(id), ItemId(item_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/items/{itemId}",
    params(
        ("id" = i64, Path, description = "Collection ID"),
        ("itemId" = i64, Path, description = "Media Item ID")
    ),
    responses(
        (status = 204, description = "Item removed from collection"),
        (status = 403, description = "Admin required")
    ),
    security(("bearer_auth" = []))
)]
async fn remove_item_from_collection(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((id, item_id)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    state.services.collections.remove_item(&auth.actor, CollectionId(id), ItemId(item_id)).await?;
    Ok(StatusCode::NO_CONTENT)
}
