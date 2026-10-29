//! Application use cases and authorization shared by the REST API and the MCP server.
//!
//! Every method takes the acting [`Actor`](mbl_core::Actor) (or is explicitly pre-auth) and
//! performs its own permission checks; adapters never authorize on their own.

pub mod auth;
pub mod collections;
pub mod companion;
mod dvr;
pub mod events;
pub mod hw;
pub mod images;
pub mod import;
pub mod intelligence;
pub mod items;
pub mod libraries;
mod live_relay;
pub mod livetv;
pub mod matchmaker;
pub mod mcp;
pub mod nodes;
pub mod paths;
pub mod playback;
pub mod players;
pub mod plugins;
pub mod remote;
pub mod requests;
pub mod stream;
pub mod users;
mod validate;
pub mod watch;
pub mod webhooks;
pub mod workers;

use std::sync::{Arc, RwLock};

use mbl_db::Db;

pub use auth::{
    AuthService, Authenticated, AuthenticatedApiToken, CreateApiTokenRequest, CreateInviteRequest,
    CreatedApiToken, DeviceInfo, LoginRequest, LoginResult, RegisterInviteRequest,
};
pub use collections::{
    CollectionRow, CollectionService, CreateCollectionRequest, CreatePlaylistRequest, PlaylistRow,
    PlaylistService, UpdateCollectionRequest, UpdatePlaylistRequest,
};
pub use companion::{CompanionReaction, CompanionService, LiveSessionContext};
pub use events::EventService;
pub use hw::{HwReport, HwSelftestResponse, HwService};
pub use images::{ArtworkFetchHandler, ImageFile, ImageService};
pub use import::{
    ImportService, ImportSummary, JellyfinImportItem, JellyfinImportRequest, PlexImportItem,
    PlexImportRequest,
};
pub use intelligence::{
    CreateOptimizedRequest, CreateOptimizedResponse, IndexScenesRequest, MediaIntelligenceService,
    OptimizeJobHandler, QuickConnectAuthorizeRequest, QuickConnectInitRequest,
    QuickConnectInitResponse, QuickConnectPollRequest, QuickConnectPollResponse, SceneHintInput,
    SceneSearchHit, SystemHealthReport, TrickplayJobHandler,
};
pub use items::{Hub, ItemService, SearchGroup};
pub use libraries::{CreateLibrary, LibraryService, UpdateLibrary};
pub use livetv::LiveTvService;
pub use matchmaker::{
    CreateMatchmakerSessionRequest, MatchmakerMatchResult, MatchmakerService, MatchmakerSessionRow,
    MatchmakerVoteRequest,
};
pub use mbl_playback::{Network as PlaybackNetwork, PlayMethod, StreamAction, SubtitleDelivery};
pub use mcp::{
    McpConfig, McpConfigSnippets, McpPreset, McpService, McpSnippetsRequest, McpStatus,
    UpdateMcpStatusRequest, generate_snippets, mcp_presets,
};
pub use nodes::RemoteNodeService;
pub use playback::{
    ActivePlaybackSession, PlaybackAudioTrack, PlaybackMarker, PlaybackPreview,
    PlaybackProfileInput, PlaybackProgressRequest, PlaybackRequest, PlaybackResponse,
    PlaybackService, PlaybackState, PlaybackStopRequest, PlaybackSubtitleOption,
    PlaybackSubtitleTrack,
};
pub use players::{PlayerCommand, PlayerInfo, PlayerRegistry};
pub use plugins::PluginService;
pub use remote::{
    BandwidthUsageResponse, RemoteService, RemoteStatusResponse, UpdateRemotePolicyRequest,
};
pub use requests::{
    CreateMediaRequest, MediaRequestRow, MediaRequestService, MissingEpisodeInfo,
    StorageHardlinkReport,
};
pub use stream::{ResolvedPart, StreamService};
pub use users::{
    CreateUser, SetPinRequest, UpdateUser, UpdateUserPermissionsRequest, UserService,
    VerifyPinRequest,
};
pub use watch::{WatchService, is_resume_threshold, is_watched_threshold};
pub use webhooks::{CreateWebhookRequest, WebhookRow, WebhookService};
pub use workers::{WorkerBatchEstimate, WorkerService};

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
    pub collections: CollectionService,
    pub playlists: PlaylistService,
    pub players: PlayerRegistry,
    pub companion: CompanionService,
    pub webhooks: WebhookService,
    pub requests: MediaRequestService,
    pub matchmaker: MatchmakerService,
    pub workers: WorkerService,
    pub nodes: RemoteNodeService,
    pub livetv: LiveTvService,
    pub plugins: PluginService,
    pub import: ImportService,
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
        let db_arc = Arc::new(db.clone());
        let collections = CollectionService::new(db_arc.clone());
        let playlists = PlaylistService::new(db_arc.clone());
        let players = PlayerRegistry::new();
        let companion = CompanionService::new().with_playback(playback.clone());
        let webhooks = WebhookService::new(db_arc.clone());
        let requests = MediaRequestService::new(db_arc.clone());
        let matchmaker = MatchmakerService::new(db_arc.clone());
        let workers = WorkerService::new(db_arc.clone());
        let nodes = RemoteNodeService::new(db_arc.clone());
        let livetv = LiveTvService::new(db_arc.clone());
        let plugins = PluginService::new(db_arc.clone());
        let import = ImportService::new(db_arc.clone());

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
            collections,
            playlists,
            players,
            companion,
            webhooks,
            requests,
            matchmaker,
            workers,
            nodes,
            livetv,
            plugins,
            import,
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
