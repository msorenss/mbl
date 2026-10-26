//! Application use cases and authorization shared by the REST API and the MCP server.
//!
//! Every method takes the acting [`Actor`](mbl_core::Actor) (or is explicitly pre-auth) and
//! performs its own permission checks; adapters never authorize on their own.

pub mod auth;
pub mod events;
pub mod hw;
pub mod images;
pub mod intelligence;
pub mod items;
pub mod libraries;
pub mod mcp;
pub mod paths;
pub mod playback;
pub mod remote;
pub mod stream;
pub mod users;
mod validate;
pub mod watch;

use std::sync::{Arc, RwLock};

use mbl_db::Db;

pub use auth::{
    AuthService, Authenticated, AuthenticatedApiToken, CreateApiTokenRequest, CreateInviteRequest,
    CreatedApiToken, DeviceInfo, LoginRequest, LoginResult, RegisterInviteRequest,
};
pub use events::EventService;
pub use hw::{HwReport, HwSelftestResponse, HwService};
pub use images::{ArtworkFetchHandler, ImageFile, ImageService};
pub use intelligence::{
    CreateOptimizedRequest, CreateOptimizedResponse, IndexScenesRequest, MediaIntelligenceService,
    OptimizeJobHandler, QuickConnectAuthorizeRequest, QuickConnectInitRequest,
    QuickConnectInitResponse, QuickConnectPollRequest, QuickConnectPollResponse, SceneHintInput,
    SceneSearchHit, SystemHealthReport,
};
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
pub use remote::{
    BandwidthUsageResponse, RemoteService, RemoteStatusResponse, UpdateRemotePolicyRequest,
};
pub use stream::{ResolvedPart, StreamService};
pub use users::{
    CreateUser, SetPinRequest, UpdateUser, UpdateUserPermissionsRequest, UserService,
    VerifyPinRequest,
};
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
    pub hw: HwService,
    pub intelligence: MediaIntelligenceService,
    pub remote: RemoteService,
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
        let hw = HwService::new(db.clone(), stream.clone());
        let playback =
            PlaybackService::new(db.clone(), auth.clone(), stream.clone(), watch.clone())
                .with_hw(hw.clone());
        let intelligence =
            MediaIntelligenceService::new(db.clone(), auth.clone(), stream.clone(), jobs.clone());
        let mcp = McpService::new(db.clone(), Arc::new(RwLock::new(McpConfig::default())));
        let tailscale = Arc::new(mbl_remote::TailscaleClient::from_platform_default());
        let bandwidth = Arc::new(mbl_remote::BandwidthManager::default());
        let remote = RemoteService::new(db.clone(), tailscale, bandwidth);

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
            hw,
            intelligence,
            remote,
        }
    }

    pub fn with_mcp_config(mut self, config: Arc<RwLock<McpConfig>>) -> Self {
        self.mcp = McpService::new(self.mcp.db(), config);
        self
    }

    pub fn with_tailscale_client(mut self, client: Arc<mbl_remote::TailscaleClient>) -> Self {
        self.remote =
            RemoteService::new(self.remote.db().clone(), client, self.remote.bandwidth().clone());
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
