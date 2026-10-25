//! Application use cases and authorization shared by the REST API and the MCP server.
//!
//! Every method takes the acting [`Actor`](mbl_core::Actor) (or is explicitly pre-auth) and
//! performs its own permission checks; adapters never authorize on their own.

pub mod auth;
pub mod events;
pub mod images;
pub mod items;
pub mod libraries;
pub mod mcp;
pub mod paths;
pub mod playback;
pub mod stream;
pub mod users;
mod validate;
pub mod watch;

use std::sync::{Arc, RwLock};

use mbl_db::Db;

pub use auth::{
    AuthService, Authenticated, AuthenticatedApiToken, CreateApiTokenRequest, CreatedApiToken,
    DeviceInfo, LoginResult,
};
pub use events::EventService;
pub use images::{ArtworkFetchHandler, ImageFile, ImageService};
pub use items::{Hub, ItemService, SearchGroup};
pub use libraries::{CreateLibrary, LibraryService, UpdateLibrary};
pub use mbl_playback::{PlayMethod, StreamAction, SubtitleDelivery};
pub use mcp::{
    McpConfig, McpConfigSnippets, McpPreset, McpService, McpSnippetsRequest, McpStatus,
    UpdateMcpStatusRequest, generate_snippets, mcp_presets,
};
pub use playback::{
    ActivePlaybackSession, PlaybackMarker, PlaybackPreview, PlaybackProfileInput,
    PlaybackProgressRequest, PlaybackRequest, PlaybackResponse, PlaybackService, PlaybackState,
    PlaybackStopRequest, PlaybackSubtitleTrack,
};
pub use stream::{ResolvedPart, StreamService};
pub use users::{CreateUser, UpdateUser, UserService};
pub use watch::{WatchService, is_resume_threshold, is_watched_threshold};

/// All application services, cheap to clone.
#[derive(Debug, Clone)]
pub struct Services {
    pub auth: AuthService,
    pub users: UserService,
    pub libraries: LibraryService,
    pub images: ImageService,
    pub items: ItemService,
    pub events: EventService,
    pub stream: StreamService,
    pub watch: WatchService,
    pub playback: PlaybackService,
    pub mcp: McpService,
}

impl Services {
    pub fn new(
        db: Db,
        jobs: mbl_jobs::JobQueue,
        images: mbl_images::ImageStore,
        events: mbl_core::events::EventBus,
    ) -> Self {
        let auth = AuthService::new(db.clone());
        let stream = StreamService::new(db.clone());
        let watch = WatchService::new(db.clone(), events.clone());
        let playback =
            PlaybackService::new(db.clone(), auth.clone(), stream.clone(), watch.clone());
        let mcp = McpService::new(db.clone(), Arc::new(RwLock::new(McpConfig::default())));
        Self {
            auth,
            users: UserService::new(db.clone()),
            libraries: LibraryService::new(db.clone(), jobs),
            images: ImageService::new(db.clone(), images),
            items: ItemService::new(db.clone()),
            events: EventService::new(db, events),
            stream,
            watch,
            playback,
            mcp,
        }
    }

    pub fn with_mcp_config(mut self, config: Arc<RwLock<McpConfig>>) -> Self {
        self.mcp = McpService::new(self.mcp.db(), config);
        self
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
