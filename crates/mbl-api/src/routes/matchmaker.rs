//! Movie Night Matchmaker ("Swipe to Match") endpoints (M7.19).

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use mbl_core::ArtworkId;
use mbl_service::{
    CreateMatchmakerSessionRequest, JoinMatchmakerAsGuest, MatchmakerGuestJoined,
    MatchmakerGuestLink, MatchmakerGuestView, MatchmakerGuestVote, MatchmakerMatchResult,
    MatchmakerSessionRow, MatchmakerVoteRequest,
};
use serde::Deserialize;

use crate::error::ApiError;
use crate::extract::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/matchmaker/sessions", post(create_matchmaker_session))
        .route("/api/v1/matchmaker/sessions/{id}", get(get_matchmaker_session))
        .route("/api/v1/matchmaker/sessions/{id}/vote", post(record_vote))
        .route("/api/v1/matchmaker/sessions/{id}/join", post(join_session))
        .route("/api/v1/matchmaker/sessions/{id}/guests", post(allow_guests).delete(stop_guests))
        .route("/api/v1/matchmaker/guest/join/{code}", post(join_as_guest))
        .route("/api/v1/matchmaker/guest", get(guest_view))
        .route("/api/v1/matchmaker/guest/vote", post(guest_vote))
        .route("/api/v1/matchmaker/guest/images/{id}", get(guest_image))
}

/// Header that carries a movie-night guest's token.
const GUEST_HEADER: &str = "x-mbl-guest";

fn guest_token(headers: &HeaderMap) -> &str {
    headers.get(GUEST_HEADER).and_then(|v| v.to_str().ok()).unwrap_or_default()
}

#[utoipa::path(
    post,
    path = "/api/v1/matchmaker/sessions/{id}/guests",
    params(("id" = String, Path, description = "Matchmaker session ID")),
    responses(
        (status = 200, description = "Guests may join with this code (the existing one if already allowed)", body = MatchmakerGuestLink),
        (status = 403, description = "Only the host or an admin"),
        (status = 404),
        (status = 409, description = "The movie night is over")
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn allow_guests(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MatchmakerGuestLink>, ApiError> {
    Ok(Json(state.services.matchmaker.guest_link(&auth.actor, &id).await?))
}

#[utoipa::path(
    delete,
    path = "/api/v1/matchmaker/sessions/{id}/guests",
    params(("id" = String, Path, description = "Matchmaker session ID")),
    responses(
        (status = 204, description = "Guests removed and the guest link turned off"),
        (status = 403, description = "Only the host or an admin"),
        (status = 404)
    ),
    security(("bearer_auth" = []))
)]
pub(crate) async fn stop_guests(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.matchmaker.stop_guests(&auth.actor, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Join a movie night without an account. The token in the answer goes in the `X-Mbl-Guest`
/// header of the other guest calls.
#[utoipa::path(
    post,
    path = "/api/v1/matchmaker/guest/join/{code}",
    params(("code" = String, Path, description = "Code from the host's guest link")),
    request_body = JoinMatchmakerAsGuest,
    responses(
        (status = 201, description = "Joined", body = MatchmakerGuestJoined),
        (status = 404, description = "Unknown code, guests turned off, or the movie night is over"),
        (status = 409, description = "The movie night is full"),
        (status = 422, description = "Missing or too long name")
    )
)]
pub(crate) async fn join_as_guest(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Json(body): Json<JoinMatchmakerAsGuest>,
) -> Result<(StatusCode, Json<MatchmakerGuestJoined>), ApiError> {
    let joined = state.services.matchmaker.join_as_guest(&code, body).await?;
    Ok((StatusCode::CREATED, Json(joined)))
}

#[utoipa::path(
    get,
    path = "/api/v1/matchmaker/guest",
    params(("x-mbl-guest" = String, Header, description = "Guest token")),
    responses(
        (status = 200, description = "Titles to vote on (what the host may see) and the result", body = MatchmakerGuestView),
        (status = 401, description = "Unknown guest token")
    )
)]
pub(crate) async fn guest_view(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<MatchmakerGuestView>, ApiError> {
    Ok(Json(state.services.matchmaker.guest_view(guest_token(&headers)).await?))
}

#[utoipa::path(
    post,
    path = "/api/v1/matchmaker/guest/vote",
    params(("x-mbl-guest" = String, Header, description = "Guest token")),
    request_body = MatchmakerGuestVote,
    responses(
        (status = 200, description = "Vote recorded and match evaluated", body = MatchmakerMatchResult),
        (status = 401, description = "Unknown guest token"),
        (status = 404, description = "Not one of this movie night's titles")
    )
)]
pub(crate) async fn guest_vote(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MatchmakerGuestVote>,
) -> Result<Json<MatchmakerMatchResult>, ApiError> {
    Ok(Json(state.services.matchmaker.guest_vote(guest_token(&headers), body).await?))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct GuestImageQuery {
    /// Guest token (images are loaded by `<img>`, which cannot send headers).
    pub token: String,
    pub w: Option<u32>,
}

/// A poster for a guest, as the host may see it.
#[utoipa::path(
    get,
    path = "/api/v1/matchmaker/guest/images/{id}",
    params(("id" = i64, Path), GuestImageQuery),
    responses((status = 200, content_type = "image/jpeg"), (status = 401), (status = 404))
)]
pub(crate) async fn guest_image(
    State(state): State<AppState>,
    Path(id): Path<ArtworkId>,
    Query(q): Query<GuestImageQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response, ApiError> {
    let host = state.services.matchmaker.guest_viewer(&q.token).await?;
    let image = super::images::ImageQuery { w: q.w, q: None, v: None, blur: None };
    super::images::image_response(&state, &host, id, &image, &headers).await
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

#[utoipa::path(post, path = "/api/v1/matchmaker/sessions/{id}/join", params(("id" = String, Path)), responses((status = 204), (status = 404), (status = 409)))]
pub(crate) async fn join_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.services.matchmaker.join_session(&auth.actor, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
