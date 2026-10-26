//! Movie Night Matchmaker ("Swipe to Match") endpoints (M7.19).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_service::{
    CreateMatchmakerSessionRequest, MatchmakerMatchResult, MatchmakerSessionRow,
    MatchmakerVoteRequest,
};

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/matchmaker/sessions", post(create_matchmaker_session))
        .route("/api/v1/matchmaker/sessions/{id}", get(get_matchmaker_session))
        .route("/api/v1/matchmaker/sessions/{id}/vote", post(record_vote))
}

#[utoipa::path(
    post,
    path = "/api/v1/matchmaker/sessions",
    request_body = CreateMatchmakerSessionRequest,
    responses(
        (status = 201, description = "Matchmaker session started", body = MatchmakerSessionRow),
    ),
    security(("bearer_auth" = []))
)]
async fn create_matchmaker_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateMatchmakerSessionRequest>,
) -> Result<(StatusCode, Json<MatchmakerSessionRow>), ApiError> {
    let row = state.services.matchmaker.create_session(&auth.actor, body).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[utoipa::path(
    get,
    path = "/api/v1/matchmaker/sessions/{id}",
    params(
        ("id" = String, Path, description = "Matchmaker session ID")
    ),
    responses(
        (status = 200, description = "Session status", body = MatchmakerSessionRow),
        (status = 404, description = "Session not found")
    ),
    security(("bearer_auth" = []))
)]
async fn get_matchmaker_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MatchmakerSessionRow>, ApiError> {
    let row = state
        .services
        .matchmaker
        .get_session(&auth.actor, &id)
        .await?
        .ok_or(mbl_core::Error::NotFound)?;
    Ok(Json(row))
}

#[utoipa::path(
    post,
    path = "/api/v1/matchmaker/sessions/{id}/vote",
    params(
        ("id" = String, Path, description = "Matchmaker session ID")
    ),
    request_body = MatchmakerVoteRequest,
    responses(
        (status = 200, description = "Vote recorded and match evaluated", body = MatchmakerMatchResult),
    ),
    security(("bearer_auth" = []))
)]
async fn record_vote(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<MatchmakerVoteRequest>,
) -> Result<Json<MatchmakerMatchResult>, ApiError> {
    let res = state.services.matchmaker.record_vote(&auth.actor, &id, body, 2).await?;
    Ok(Json(res))
}
