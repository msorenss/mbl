//! OAuth 2.1 for MCP per the MCP Authorization spec (M6.17).
//!
//! Features:
//! - Protected resource & authorization server metadata under `/.well-known/`
//! - Dynamic client registration (`/oauth/register`)
//! - Authorization Code flow with strict PKCE (`S256` only)
//! - Audience-bound access tokens for `/mcp`
//! - Refresh token rotation and token revocation (`/oauth/revoke`)

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mbl_core::{Error, Result, time::now_ms};
use mbl_db::repo::m6::{
    OAuthClient, create_oauth_auth_code, exchange_oauth_code_for_tokens, find_oauth_grant_by_code,
    find_oauth_grant_by_refresh_token_hash, get_oauth_client, register_oauth_client,
    revoke_oauth_grant_by_token,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::extract::AuthUser;
use crate::{ApiError, AppState};

const ACCESS_TOKEN_TTL_MS: i64 = 60 * 60 * 1000; // 1 hour
const AUTH_CODE_TTL_MS: i64 = 10 * 60 * 1000; // 10 minutes

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/oauth-authorization-server", get(oauth_authorization_server_metadata))
        .route("/.well-known/oauth-protected-resource", get(oauth_protected_resource_metadata))
        .route("/oauth/register", post(register_client))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", post(token_endpoint))
        .route("/oauth/revoke", post(revoke_token))
}

// -----------------------------------------------------------------------------
// Metadata Endpoints
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OAuthAuthorizationServerMetadata {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub registration_endpoint: String,
    pub revocation_endpoint: String,
    pub response_types_supported: Vec<String>,
    pub grant_types_supported: Vec<String>,
    pub code_challenge_methods_supported: Vec<String>,
    pub token_endpoint_auth_methods_supported: Vec<String>,
    pub scopes_supported: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct OAuthProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/.well-known/oauth-authorization-server",
    tag = "oauth",
    security(),
    responses((status = 200, body = OAuthAuthorizationServerMetadata))
)]
pub(crate) async fn oauth_authorization_server_metadata() -> Json<OAuthAuthorizationServerMetadata>
{
    Json(OAuthAuthorizationServerMetadata {
        issuer: "/".into(),
        authorization_endpoint: "/oauth/authorize".into(),
        token_endpoint: "/oauth/token".into(),
        registration_endpoint: "/oauth/register".into(),
        revocation_endpoint: "/oauth/revoke".into(),
        response_types_supported: vec!["code".into()],
        grant_types_supported: vec!["authorization_code".into(), "refresh_token".into()],
        code_challenge_methods_supported: vec!["S256".into()],
        token_endpoint_auth_methods_supported: vec!["none".into()],
        scopes_supported: vec![
            "read".into(),
            "playback".into(),
            "library".into(),
            "users".into(),
            "remote".into(),
            "admin".into(),
        ],
    })
}

#[utoipa::path(
    get,
    path = "/.well-known/oauth-protected-resource",
    tag = "oauth",
    security(),
    responses((status = 200, body = OAuthProtectedResourceMetadata))
)]
pub(crate) async fn oauth_protected_resource_metadata() -> Json<OAuthProtectedResourceMetadata> {
    Json(OAuthProtectedResourceMetadata {
        resource: "/mcp".into(),
        authorization_servers: vec!["/".into()],
        scopes_supported: vec![
            "read".into(),
            "playback".into(),
            "library".into(),
            "users".into(),
            "remote".into(),
            "admin".into(),
        ],
    })
}

// -----------------------------------------------------------------------------
// Dynamic Client Registration
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RegisterClientRequest {
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    #[serde(default)]
    pub grant_types: Option<Vec<String>>,
    #[serde(default)]
    pub response_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RegisterClientResponse {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
}

#[utoipa::path(
    post,
    path = "/oauth/register",
    tag = "oauth",
    security(),
    request_body = RegisterClientRequest,
    responses((status = 201, body = RegisterClientResponse))
)]
pub(crate) async fn register_client(
    State(state): State<AppState>,
    Json(req): Json<RegisterClientRequest>,
) -> Result<(StatusCode, Json<RegisterClientResponse>), ApiError> {
    let client_id = format!("mbl_client_{}", &mbl_auth::new_token()?.value[..16]);
    let client = OAuthClient {
        client_id: client_id.clone(),
        client_name: req.client_name.clone(),
        redirect_uris: req.redirect_uris.clone(),
        grant_types: req
            .grant_types
            .unwrap_or_else(|| vec!["authorization_code".into(), "refresh_token".into()]),
        response_types: req.response_types.unwrap_or_else(|| vec!["code".into()]),
        token_endpoint_auth: "none".into(),
        created_at: now_ms(),
    };

    register_oauth_client(&state.db, &client).await?;

    Ok((
        StatusCode::CREATED,
        Json(RegisterClientResponse {
            client_id: client.client_id,
            client_name: client.client_name,
            redirect_uris: client.redirect_uris,
            grant_types: client.grant_types,
            response_types: client.response_types,
            token_endpoint_auth_method: "none".into(),
        }),
    ))
}

// -----------------------------------------------------------------------------
// Authorization Endpoint
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AuthorizeParams {
    pub client_id: String,
    pub redirect_uri: String,
    pub response_type: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub scope: Option<String>,
    pub state: Option<String>,
}

#[utoipa::path(
    get,
    path = "/oauth/authorize",
    tag = "oauth",
    params(
        ("client_id" = String, Query, description = "Registered client ID"),
        ("redirect_uri" = String, Query, description = "Redirect URI matching client registration"),
        ("response_type" = String, Query, description = "Must be 'code'"),
        ("code_challenge" = String, Query, description = "PKCE code challenge (BASE64URL(SHA256(verifier)))"),
        ("code_challenge_method" = String, Query, description = "Must be 'S256'"),
        ("scope" = Option<String>, Query, description = "Requested space-separated scopes"),
        ("state" = Option<String>, Query, description = "Opaque client state"),
    ),
    responses((status = 302, description = "Redirect to client redirect_uri with code"), (status = 400), (status = 401))
)]
pub(crate) async fn authorize(
    State(state): State<AppState>,
    AuthUser(auth): AuthUser,
    Query(params): Query<AuthorizeParams>,
) -> Result<Response, ApiError> {
    // 1. Validate PKCE code_challenge_method = S256
    if params.code_challenge_method != "S256" {
        return Err(Error::Validation(
            "only code_challenge_method=S256 is supported in OAuth 2.1".into(),
        )
        .into());
    }
    if params.response_type != "code" {
        return Err(Error::Validation("response_type must be 'code'".into()).into());
    }

    // 2. Validate client and redirect_uri
    let client =
        get_oauth_client(&state.db, &params.client_id).await?.ok_or_else(|| Error::NotFound)?;

    if !client.redirect_uris.contains(&params.redirect_uri) {
        return Err(
            Error::Validation("redirect_uri is not registered for this client".into()).into()
        );
    }

    // 3. Issue Authorization Code
    let code = format!("mbl_code_{}", mbl_auth::new_token()?.value);
    let scopes: Vec<String> = params
        .scope
        .as_deref()
        .unwrap_or("read")
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let expires_at = now_ms() + AUTH_CODE_TTL_MS;

    create_oauth_auth_code(
        &state.db,
        &code,
        &params.code_challenge,
        &params.code_challenge_method,
        &params.client_id,
        auth.actor.user_id,
        &scopes,
        &params.redirect_uri,
        expires_at,
    )
    .await?;

    let redirect_url = match params.state {
        Some(st) => format!("{}?code={}&state={}", params.redirect_uri, code, st),
        None => format!("{}?code={}", params.redirect_uri, code),
    };

    Ok(axum::response::Redirect::to(&redirect_url).into_response())
}

// -----------------------------------------------------------------------------
// Token Endpoint
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct TokenForm {
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub scope: String,
}

#[utoipa::path(
    post,
    path = "/oauth/token",
    tag = "oauth",
    security(),
    request_body(content = TokenForm, content_type = "application/x-www-form-urlencoded"),
    responses((status = 200, body = TokenResponse), (status = 400), (status = 401))
)]
pub(crate) async fn token_endpoint(
    State(state): State<AppState>,
    Form(form): Form<TokenForm>,
) -> Result<Json<TokenResponse>, ApiError> {
    match form.grant_type.as_str() {
        "authorization_code" => {
            let code = form.code.ok_or_else(|| Error::Validation("missing code".into()))?;
            let verifier = form
                .code_verifier
                .ok_or_else(|| Error::Validation("missing code_verifier".into()))?;
            let redirect_uri = form
                .redirect_uri
                .ok_or_else(|| Error::Validation("missing redirect_uri".into()))?;

            let grant = find_oauth_grant_by_code(&state.db, &code)
                .await?
                .ok_or_else(|| Error::Unauthorized)?;

            let now = now_ms();
            if grant.code_expires_at.unwrap_or(0) <= now {
                return Err(Error::Unauthorized.into());
            }

            if grant.redirect_uri != redirect_uri {
                return Err(Error::Validation("redirect_uri mismatch".into()).into());
            }

            // Verify PKCE S256: BASE64URL(SHA256(verifier)) == challenge
            let challenge = grant.code_challenge.as_deref().unwrap_or("");
            let computed_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
            if computed_challenge != challenge {
                return Err(Error::Unauthorized.into());
            }

            // Issue tokens
            let access_raw = format!("mbl_at_{}", mbl_auth::new_token()?.value);
            let refresh_raw = format!("mbl_rt_{}", mbl_auth::new_token()?.value);

            let at_hash = Sha256::digest(access_raw.as_bytes()).to_vec();
            let rt_hash = Sha256::digest(refresh_raw.as_bytes()).to_vec();

            let at_expires = now + ACCESS_TOKEN_TTL_MS;

            exchange_oauth_code_for_tokens(
                &state.db,
                grant.id,
                &at_hash,
                Some(&rt_hash),
                at_expires,
            )
            .await?;

            Ok(Json(TokenResponse {
                access_token: access_raw,
                token_type: "Bearer".into(),
                expires_in: 3600,
                refresh_token: refresh_raw,
                scope: grant.scopes.join(" "),
            }))
        }
        "refresh_token" => {
            let rt = form
                .refresh_token
                .ok_or_else(|| Error::Validation("missing refresh_token".into()))?;
            let rt_hash = Sha256::digest(rt.as_bytes()).to_vec();

            let grant = find_oauth_grant_by_refresh_token_hash(&state.db, &rt_hash)
                .await?
                .ok_or_else(|| Error::Unauthorized)?;

            if grant.revoked_at.is_some() {
                return Err(Error::Unauthorized.into());
            }

            let now = now_ms();
            let new_access_raw = format!("mbl_at_{}", mbl_auth::new_token()?.value);
            let new_refresh_raw = format!("mbl_rt_{}", mbl_auth::new_token()?.value);

            let new_at_hash = Sha256::digest(new_access_raw.as_bytes()).to_vec();
            let new_rt_hash = Sha256::digest(new_refresh_raw.as_bytes()).to_vec();

            exchange_oauth_code_for_tokens(
                &state.db,
                grant.id,
                &new_at_hash,
                Some(&new_rt_hash),
                now + ACCESS_TOKEN_TTL_MS,
            )
            .await?;

            Ok(Json(TokenResponse {
                access_token: new_access_raw,
                token_type: "Bearer".into(),
                expires_in: 3600,
                refresh_token: new_refresh_raw,
                scope: grant.scopes.join(" "),
            }))
        }
        _ => Err(Error::Validation("unsupported grant_type".into()).into()),
    }
}

// -----------------------------------------------------------------------------
// Revocation Endpoint
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct RevokeForm {
    pub token: String,
}

#[utoipa::path(
    post,
    path = "/oauth/revoke",
    tag = "oauth",
    security(),
    request_body(content = RevokeForm, content_type = "application/x-www-form-urlencoded"),
    responses((status = 200, description = "Token revoked"))
)]
pub(crate) async fn revoke_token(
    State(state): State<AppState>,
    Form(form): Form<RevokeForm>,
) -> StatusCode {
    let hash = Sha256::digest(form.token.as_bytes()).to_vec();
    let _ = revoke_oauth_grant_by_token(&state.db, &hash).await;
    StatusCode::OK
}
