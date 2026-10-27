use std::sync::Arc;

use mbl_auth::tokens::{ScopeSet, new_api_token, validate_allowed_networks};
use mbl_auth::totp::TotpSetup;
use mbl_auth::{STREAM_TOKEN_TTL_MS, StreamTokenClaims};
use mbl_core::{
    Actor, Error, LibraryAccess, LibraryId, PartId, Result, SessionId, UserId, time::now_ms,
};
use mbl_db::Db;
use mbl_db::repo::m6::{self, InviteRow, get_remote_setting};
use mbl_db::repo::mcp::{self, ApiTokenInfo, NewApiToken};
use mbl_db::repo::sessions::{NewSession, SessionInfo, SessionsRepo};
use mbl_db::repo::users::{NewUser, User, UsersRepo};
use mbl_remote::TailscaleAuthIdentity;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{blocking, validate};

const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Sliding expiry is refreshed at most this often to avoid a DB write per request.
const TOUCH_INTERVAL_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Clone, Default)]
pub struct DeviceInfo {
    pub device_name: Option<String>,
    pub client_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoginResult {
    /// Shown to the client once; only its hash is stored.
    pub token: String,
    pub session_id: SessionId,
    pub user: User,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authenticated {
    pub actor: Actor,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub totp_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateInviteRequest {
    #[serde(default)]
    pub library_ids: Vec<LibraryId>,
    #[serde(default)]
    pub content_rating_ceiling: Option<String>,
    /// Lifetime in hours (default 72 hours).
    #[serde(default = "default_invite_ttl")]
    pub ttl_hours: u32,
}

fn default_invite_ttl() -> u32 {
    72
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegisterInviteRequest {
    pub code: String,
    pub username: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateApiTokenRequest {
    pub name: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub allowed_networks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatedApiToken {
    /// Shown once upon creation; never stored in plaintext.
    pub token: String,
    #[serde(flatten)]
    pub info: ApiTokenInfo,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedApiToken {
    pub token_id: i64,
    pub token_name: String,
    pub actor: Actor,
    pub scopes: ScopeSet,
}

#[derive(Debug, Clone)]
pub struct AuthService {
    db: Db,
    users: UsersRepo,
    sessions: SessionsRepo,
    stream_secret: Arc<[u8; 32]>,
}

impl AuthService {
    pub fn new(db: Db) -> Self {
        let secret = mbl_auth::new_secret_key().unwrap_or([0x5au8; 32]);
        Self {
            users: UsersRepo::new(db.clone()),
            sessions: SessionsRepo::new(db.clone()),
            db,
            stream_secret: Arc::new(secret),
        }
    }

    pub async fn needs_setup(&self) -> Result<bool> {
        Ok(self.users.count().await? == 0)
    }

    /// Create the first admin and log them in. Fails with `Conflict` once any user exists.
    pub async fn setup(
        &self,
        username: &str,
        password: &str,
        display_name: Option<&str>,
        device: &DeviceInfo,
    ) -> Result<LoginResult> {
        let username = validate::username(username)?;
        validate::password(password)?;
        let display_name = validate::display_name(display_name, &username)?;
        if !self.needs_setup().await? {
            return Err(Error::Conflict("setup already completed".into()));
        }

        let password = password.to_owned();
        let hash = blocking(move || mbl_auth::hash_password(&password)).await?;
        let new_user = NewUser {
            username: &username,
            display_name: &display_name,
            password_hash: &hash,
            is_admin: true,
        };
        let id = self
            .users
            .insert_first_admin(&new_user, now_ms())
            .await?
            .ok_or_else(|| Error::Conflict("setup already completed".into()))?;
        let user = self.users.get(id).await?.ok_or(Error::NotFound)?;
        tracing::info!(user_id = %id, "initial admin created");
        self.start_session(user, device).await
    }

    pub async fn login(
        &self,
        username: &str,
        password: &str,
        device: &DeviceInfo,
    ) -> Result<LoginResult> {
        self.login_with_options(username, password, None, device, false).await
    }

    pub async fn login_with_options(
        &self,
        username: &str,
        password: &str,
        totp_code: Option<&str>,
        device: &DeviceInfo,
        is_wan: bool,
    ) -> Result<LoginResult> {
        let creds = self.users.get_credentials(username.trim()).await?;
        let hash =
            creds.as_ref().map_or_else(|| mbl_auth::dummy_hash().to_owned(), |(_, h)| h.clone());
        let password = password.to_owned();
        let ok = blocking(move || Ok(mbl_auth::verify_password(&password, &hash))).await?;

        let Some((user, _)) = creds else {
            return Err(Error::Unauthorized);
        };
        if !ok {
            return Err(Error::Unauthorized);
        }

        // Check TOTP 2FA
        let totp_info = m6::get_user_totp(&self.db, user.id).await?;
        let (totp_secret, totp_enabled) = totp_info.unwrap_or((None, false));

        // M6.3: Admins on WAN connections require TOTP 2FA
        if user.is_admin && is_wan && !totp_enabled {
            return Err(Error::Forbidden);
        }

        if totp_enabled {
            let Some(secret) = totp_secret else {
                return Err(Error::Internal("TOTP enabled but secret missing".into()));
            };
            let Some(candidate_code) = totp_code else {
                return Err(Error::Unauthorized);
            };
            let now_secs = (now_ms() / 1000) as u64;
            if !mbl_auth::totp::verify_code(&secret, candidate_code, now_secs) {
                return Err(Error::Unauthorized);
            }
        }

        self.start_session(user, device).await
    }

    /// Issue a new session token for an already-authenticated user (used by Quick-Connect in M5.8).
    pub async fn create_session_for_user(
        &self,
        user_id: UserId,
        device: &DeviceInfo,
    ) -> Result<String> {
        let user = self.users.get(user_id).await?.ok_or(Error::NotFound)?;
        let res = self.start_session(user, device).await?;
        Ok(res.token)
    }

    async fn start_session(&self, user: User, device: &DeviceInfo) -> Result<LoginResult> {
        let token = mbl_auth::new_token()?;
        let now = now_ms();
        let session_id = self
            .sessions
            .insert(&NewSession {
                user_id: user.id,
                token_hash: &token.hash,
                device_name: device.device_name.as_deref(),
                client_name: device.client_name.as_deref(),
                now,
                expires_at: now + SESSION_TTL_MS,
            })
            .await?;
        Ok(LoginResult { token: token.value, session_id, user })
    }

    /// Resolve a bearer/cookie session token to an actor. Expired or unknown tokens are `Unauthorized`.
    pub async fn authenticate(&self, token: &str) -> Result<Authenticated> {
        let hash = mbl_auth::hash_token(token);
        let session = self.sessions.find_by_token_hash(&hash).await?.ok_or(Error::Unauthorized)?;
        let now = now_ms();
        if session.expires_at <= now {
            self.sessions.delete(session.id).await?;
            return Err(Error::Unauthorized);
        }
        if now - session.last_seen_at >= TOUCH_INTERVAL_MS {
            self.sessions.touch(session.id, now, now + SESSION_TTL_MS).await?;
        }
        let (library_access, is_managed, content_rating_ceiling) =
            self.resolve_actor_permissions(session.user_id, session.is_admin).await?;
        Ok(Authenticated {
            actor: Actor {
                user_id: session.user_id,
                is_admin: session.is_admin,
                library_access,
                is_managed,
                content_rating_ceiling,
            },
            session_id: session.id,
        })
    }

    pub async fn logout(&self, session_id: SessionId) -> Result<()> {
        self.sessions.delete(session_id).await?;
        Ok(())
    }

    pub async fn list_sessions(&self, actor: &Actor) -> Result<Vec<SessionInfo>> {
        self.sessions.list_for_user(actor.user_id).await
    }

    /// Revoke one of the actor's own sessions.
    pub async fn revoke_session(&self, actor: &Actor, id: SessionId) -> Result<()> {
        if self.sessions.delete_for_user(actor.user_id, id).await? {
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    pub async fn me(&self, actor: &Actor) -> Result<User> {
        self.users.get(actor.user_id).await?.ok_or(Error::Unauthorized)
    }

    pub async fn purge_expired_sessions(&self) -> Result<u64> {
        self.sessions.delete_expired(now_ms()).await
    }

    /// Create a scoped API token (`POST /api/v1/auth/tokens`).
    /// Scopes are capped by the user's own rights; non-admins cannot get `admin`, `users`, or `remote`.
    pub async fn create_api_token(
        &self,
        actor: &Actor,
        req: CreateApiTokenRequest,
    ) -> Result<CreatedApiToken> {
        let name = req.name.trim();
        if name.is_empty() || name.chars().count() > 100 {
            return Err(Error::Validation("token name must be 1..=100 characters".into()));
        }

        let scope_set = ScopeSet::parse_list(&req.scopes)?;
        scope_set.validate_for_user(actor.is_admin)?;

        let allowed_networks = validate_allowed_networks(req.allowed_networks.as_deref())?;
        let now = now_ms();
        if let Some(exp) = req.expires_at
            && exp <= now
        {
            return Err(Error::Validation("expiresAt must be in the future".into()));
        }

        let minted = new_api_token()?;
        let scopes_json = serde_json::to_string(&scope_set.to_strings())
            .map_err(|e| Error::Internal(Box::new(e)))?;
        let networks_json =
            serde_json::to_string(&allowed_networks).map_err(|e| Error::Internal(Box::new(e)))?;

        let info = mcp::insert_token(
            &self.db,
            &NewApiToken {
                user_id: actor.user_id,
                name,
                token_hash: &minted.hash,
                scopes_json: &scopes_json,
                allowed_networks_json: &networks_json,
                created_at: now,
                expires_at: req.expires_at,
            },
        )
        .await?;

        Ok(CreatedApiToken { token: minted.value, info })
    }

    /// List active API tokens owned by `actor.user_id` (`GET /api/v1/auth/tokens`).
    pub async fn list_api_tokens(&self, actor: &Actor) -> Result<Vec<ApiTokenInfo>> {
        mcp::list_tokens_for_user(&self.db, actor.user_id).await
    }

    /// List all API tokens across all users (`GET /api/v1/auth/tokens?all=true`, admin only).
    pub async fn list_all_api_tokens(&self, actor: &Actor) -> Result<Vec<ApiTokenInfo>> {
        actor.require_admin()?;
        mcp::list_all_tokens(&self.db).await
    }

    /// Revoke an API token (`DELETE /api/v1/auth/tokens/{id}`).
    pub async fn revoke_api_token(&self, actor: &Actor, id: i64) -> Result<()> {
        if mcp::revoke_token(&self.db, actor.user_id, actor.is_admin, id, now_ms()).await? {
            Ok(())
        } else {
            Err(Error::NotFound)
        }
    }

    /// Authenticate a bearer token against `api_tokens`, verifying expiry, revocation, and network allowlist.
    /// Returns `Ok(None)` if the hash is not in `api_tokens` (allowing fallback to session tokens).
    pub async fn authenticate_api_token(
        &self,
        token: &str,
        network: &str,
    ) -> Result<Option<AuthenticatedApiToken>> {
        let hash = mbl_auth::hash_token(token);
        let Some(row) = mcp::find_by_token_hash(&self.db, &hash).await? else {
            return Ok(None);
        };
        let now = now_ms();
        if row.revoked_at.is_some() || row.expires_at.is_some_and(|exp| exp <= now) {
            return Err(Error::Unauthorized);
        }
        if !row.allowed_networks.iter().any(|n| n.eq_ignore_ascii_case(network)) {
            return Err(Error::Forbidden);
        }
        let scopes = ScopeSet::parse_list(&row.scopes)?.capped_for_user(row.is_admin);
        if scopes.is_empty() {
            return Err(Error::Forbidden);
        }
        if row.last_used_at.is_none_or(|last| now - last >= 60_000) {
            let _ = mcp::touch_token(&self.db, row.id, now).await;
        }
        let (library_access, is_managed, content_rating_ceiling) =
            self.resolve_actor_permissions(row.user_id, row.is_admin).await?;
        Ok(Some(AuthenticatedApiToken {
            token_id: row.id,
            token_name: row.name,
            actor: Actor {
                user_id: row.user_id,
                is_admin: row.is_admin,
                library_access,
                is_managed,
                content_rating_ceiling,
            },
            scopes,
        }))
    }

    /// Mint a 6-hour HMAC-signed `?token=` scoped to `actor.user_id` and `part_id`.
    pub fn sign_stream_token(&self, actor: &Actor, part_id: PartId) -> String {
        self.sign_stream_token_with_ttl(actor.user_id, part_id, STREAM_TOKEN_TTL_MS)
    }

    /// Mint a signed `?token=` with a custom TTL in milliseconds.
    pub fn sign_stream_token_with_ttl(
        &self,
        user_id: UserId,
        part_id: PartId,
        ttl_ms: i64,
    ) -> String {
        mbl_auth::sign_stream_token(
            &self.stream_secret,
            StreamTokenClaims { user_id, part_id, expires_at_ms: now_ms().saturating_add(ttl_ms) },
        )
    }

    /// Verify a signed `?token=` for `part_id` and resolve the owning user to an [`Actor`].
    pub async fn authenticate_stream_token(&self, token: &str, part_id: PartId) -> Result<Actor> {
        let claims = mbl_auth::verify_stream_token(&self.stream_secret, token, part_id, now_ms())?;
        let user = self.users.get(claims.user_id).await?.ok_or(Error::Unauthorized)?;
        let (library_access, is_managed, content_rating_ceiling) =
            self.resolve_actor_permissions(user.id, user.is_admin).await?;
        Ok(Actor {
            user_id: user.id,
            is_admin: user.is_admin,
            library_access,
            is_managed,
            content_rating_ceiling,
        })
    }

    pub async fn resolve_actor_permissions(
        &self,
        user_id: UserId,
        is_admin: bool,
    ) -> Result<(LibraryAccess, bool, Option<String>)> {
        if is_admin {
            return Ok((LibraryAccess::All, false, None));
        }
        let info = mbl_db::repo::m6::get_user_security_info(&self.db, user_id).await?;
        if let Some(sec) = info {
            let libs = if sec.library_ids.is_empty() && !sec.is_managed {
                LibraryAccess::All
            } else {
                LibraryAccess::Only(sec.library_ids)
            };
            Ok((libs, sec.is_managed, sec.content_rating_ceiling))
        } else {
            Ok((LibraryAccess::All, false, None))
        }
    }

    // -------------------------------------------------------------------------
    // Invites (M6.2)
    // -------------------------------------------------------------------------

    pub async fn create_invite(
        &self,
        actor: &Actor,
        req: CreateInviteRequest,
    ) -> Result<InviteRow> {
        actor.require_admin()?;
        let random_token = mbl_auth::new_token()?;
        let code = format!("mbl_inv_{}", &random_token.value[..16]);
        let ttl_ms = i64::from(req.ttl_hours.clamp(1, 720)) * 3600 * 1000;
        let expires_at = now_ms().saturating_add(ttl_ms);

        let row = m6::create_invite(
            &self.db,
            &code,
            actor.user_id,
            &req.library_ids,
            req.content_rating_ceiling.as_deref(),
            expires_at,
        )
        .await?;
        tracing::info!(code = %row.code, by = %actor.user_id, "share invite created");
        Ok(row)
    }

    pub async fn list_invites(&self, actor: &Actor) -> Result<Vec<InviteRow>> {
        actor.require_admin()?;
        m6::list_invites(&self.db).await
    }

    pub async fn delete_invite(&self, actor: &Actor, id: i64) -> Result<()> {
        actor.require_admin()?;
        if m6::delete_invite(&self.db, id).await? { Ok(()) } else { Err(Error::NotFound) }
    }

    pub async fn inspect_invite(&self, code: &str) -> Result<InviteRow> {
        let invite = m6::get_invite_by_code(&self.db, code).await?.ok_or(Error::NotFound)?;
        if invite.used_at.is_some() || invite.expires_at <= now_ms() {
            return Err(Error::NotFound);
        }
        Ok(invite)
    }

    pub async fn register_invite(
        &self,
        req: RegisterInviteRequest,
        device: &DeviceInfo,
    ) -> Result<LoginResult> {
        let invite = m6::get_invite_by_code(&self.db, &req.code)
            .await?
            .ok_or_else(|| Error::Validation("invalid or expired invite code".into()))?;

        let now = now_ms();
        if invite.used_at.is_some() || invite.expires_at <= now {
            return Err(Error::Validation("invite code has already been used or expired".into()));
        }

        let username = validate::username(&req.username)?;
        validate::password(&req.password)?;
        let display_name = validate::display_name(req.display_name.as_deref(), &username)?;

        let password = req.password;
        let hash = blocking(move || mbl_auth::hash_password(&password)).await?;
        let new_user = NewUser {
            username: &username,
            display_name: &display_name,
            password_hash: &hash,
            is_admin: false,
        };

        let user_id = self.users.insert(&new_user, now).await?;

        // Assign libraries and content rating ceiling from invite
        m6::update_user_permissions(
            &self.db,
            user_id,
            Some(false),
            Some(invite.content_rating_ceiling),
            Some(invite.library_ids),
        )
        .await?;

        // Mark invite consumed
        m6::mark_invite_used(&self.db, &req.code, user_id, now).await?;

        let user = self.users.get(user_id).await?.ok_or(Error::NotFound)?;
        tracing::info!(user_id = %user_id, code = %req.code, "registered new user via invite");
        self.start_session(user, device).await
    }

    // -------------------------------------------------------------------------
    // TOTP 2FA (M6.3)
    // -------------------------------------------------------------------------

    pub async fn totp_setup(&self, actor: &Actor) -> Result<TotpSetup> {
        let secret = mbl_auth::totp::generate_secret()?;
        let user = self.users.get(actor.user_id).await?.ok_or(Error::NotFound)?;
        if !m6::begin_user_totp_setup(&self.db, actor.user_id, &secret).await? {
            return Err(Error::Conflict(
                "TOTP is already enabled; disable it before replacing it".into(),
            ));
        }

        let uri = mbl_auth::totp::otpauth_uri(&secret, &user.username, "MBL");
        Ok(TotpSetup { secret, uri })
    }

    pub async fn totp_verify(&self, actor: &Actor, code: &str) -> Result<bool> {
        let totp = m6::get_user_totp(&self.db, actor.user_id).await?.ok_or(Error::NotFound)?;
        let (Some(secret), _) = totp else {
            return Err(Error::Validation("no pending TOTP setup for user".into()));
        };

        let now_secs = (now_ms() / 1000) as u64;
        if mbl_auth::totp::verify_code(&secret, code, now_secs) {
            m6::set_user_totp(&self.db, actor.user_id, Some(&secret), true).await?;
            tracing::info!(user_id = %actor.user_id, "TOTP 2FA successfully enabled");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn totp_disable(&self, actor: &Actor) -> Result<()> {
        m6::set_user_totp(&self.db, actor.user_id, None, false).await?;
        tracing::info!(user_id = %actor.user_id, "TOTP 2FA disabled");
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Tailscale Identity Login (M6.7)
    // -------------------------------------------------------------------------

    pub async fn login_tailscale(
        &self,
        identity: TailscaleAuthIdentity,
        device: &DeviceInfo,
    ) -> Result<LoginResult> {
        let enabled = get_remote_setting(&self.db, "tailscale_auth_enabled")
            .await?
            .map(|s| s == "true" || s == "1")
            .unwrap_or(false);

        if !enabled {
            return Err(Error::Forbidden);
        }

        let username = identity.login_name.trim();
        let user = match self.users.get_credentials(username).await? {
            Some((u, _)) => u,
            None => {
                // Auto-provision a non-admin account for the verified Tailscale user
                let dummy_pass = mbl_auth::new_token()?.value;
                let hash = blocking(move || mbl_auth::hash_password(&dummy_pass)).await?;
                let new_user = NewUser {
                    username,
                    display_name: &identity.display_name,
                    password_hash: &hash,
                    is_admin: false,
                };
                let id = self.users.insert(&new_user, now_ms()).await?;
                self.users.get(id).await?.ok_or(Error::NotFound)?
            }
        };

        tracing::info!(user_id = %user.id, tailscale_login = %username, "Tailscale identity login succeeded");
        self.start_session(user, device).await
    }
}
