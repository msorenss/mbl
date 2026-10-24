//! Application use cases and authorization shared by the REST API and the MCP server.
//!
//! Every method takes the acting [`Actor`](mbl_core::Actor) (or is explicitly pre-auth) and
//! performs its own permission checks; adapters never authorize on their own.

pub mod auth;
pub mod events;
pub mod images;
pub mod items;
pub mod libraries;
pub mod paths;
pub mod users;
mod validate;

use mbl_db::Db;

pub use auth::{AuthService, Authenticated, DeviceInfo, LoginResult};
pub use events::EventService;
pub use images::{ArtworkFetchHandler, ImageFile, ImageService};
pub use items::{Hub, ItemService, SearchGroup};
pub use libraries::{CreateLibrary, LibraryService, UpdateLibrary};
pub use users::{CreateUser, UpdateUser, UserService};

/// All application services, cheap to clone.
#[derive(Debug, Clone)]
pub struct Services {
    pub auth: AuthService,
    pub users: UserService,
    pub libraries: LibraryService,
    pub images: ImageService,
    pub items: ItemService,
    pub events: EventService,
}

impl Services {
    pub fn new(
        db: Db,
        jobs: mbl_jobs::JobQueue,
        images: mbl_images::ImageStore,
        events: mbl_core::events::EventBus,
    ) -> Self {
        Self {
            auth: AuthService::new(db.clone()),
            users: UserService::new(db.clone()),
            libraries: LibraryService::new(db.clone(), jobs),
            images: ImageService::new(db.clone(), images),
            items: ItemService::new(db.clone()),
            events: EventService::new(db, events),
        }
    }
}

/// Run CPU-heavy work (password hashing) off the async executor.
pub(crate) async fn blocking<T, F>(f: F) -> mbl_core::Result<T>
where
    F: FnOnce() -> mbl_core::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(mbl_core::Error::internal)?
}
