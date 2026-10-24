use mbl_core::{Actor, Error, LibraryAccess, Result, SessionId, time::now_ms};
use mbl_db::Db;
use mbl_db::repo::sessions::{NewSession, SessionInfo, SessionsRepo};
use mbl_db::repo::users::{NewUser, User, UsersRepo};

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

#[derive(Debug, Clone)]
pub struct AuthService {
    users: UsersRepo,
    sessions: SessionsRepo,
}

impl AuthService {
    pub fn new(db: Db) -> Self {
        Self { users: UsersRepo::new(db.clone()), sessions: SessionsRepo::new(db) }
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
        let creds = self.users.get_credentials(username.trim()).await?;
        let hash =
            creds.as_ref().map_or_else(|| mbl_auth::dummy_hash().to_owned(), |(_, h)| h.clone());
        let password = password.to_owned();
        let ok = blocking(move || Ok(mbl_auth::verify_password(&password, &hash))).await?;

        match creds {
            Some((user, _)) if ok => self.start_session(user, device).await,
            _ => Err(Error::Unauthorized),
        }
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

    /// Resolve a bearer/cookie token to an actor. Expired or unknown tokens are `Unauthorized`.
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
        Ok(Authenticated {
            actor: Actor {
                user_id: session.user_id,
                is_admin: session.is_admin,
                library_access: LibraryAccess::All,
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
}
