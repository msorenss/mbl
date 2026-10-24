//! Application use cases and authorization shared by the REST API and the MCP server.
//!
//! Every method takes the acting [`Actor`](mbl_core::Actor) (or is explicitly pre-auth) and
//! performs its own permission checks; adapters never authorize on their own.

pub mod auth;
pub mod users;
mod validate;

use mbl_db::Db;

pub use auth::{AuthService, Authenticated, DeviceInfo, LoginResult};
pub use users::{CreateUser, UpdateUser, UserService};

/// All application services, cheap to clone.
#[derive(Debug, Clone)]
pub struct Services {
    pub auth: AuthService,
    pub users: UserService,
}

impl Services {
    pub fn new(db: Db) -> Self {
        Self {
            auth: AuthService::new(db.clone()),
            users: UserService::new(db),
        }
    }
}

/// Run CPU-heavy work (password hashing) off the async executor.
pub(crate) async fn blocking<T, F>(f: F) -> mbl_core::Result<T>
where
    F: FnOnce() -> mbl_core::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(mbl_core::Error::internal)?
}
