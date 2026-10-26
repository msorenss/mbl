//! OpenAPI document and the Scalar reference UI.

use axum::Router;
use axum::http::header;
use axum::response::Html;
use axum::routing::get;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::AppState;
use crate::routes::{
    auth, collections, companion, events, hw, images, intelligence, invites, items, libraries,
    matchmaker, mcp, media_audio, oauth, photos, playback, players, playlists, remote, requests,
    stream, system, users, webhooks,
};

#[derive(OpenApi)]
#[openapi(
    info(title = "MBL API", description = "MBL media server REST API."),
    paths(
        system::health, system::system_health, system::info, system::setup_status, system::setup,
        auth::login, auth::logout, auth::me, auth::list_sessions, auth::revoke_session,
        auth::create_token, auth::list_tokens, auth::revoke_token,
        auth::totp_setup, auth::totp_verify, auth::totp_disable, auth::register_invite, auth::login_tailscale,
        users::list, users::create, users::get_one, users::update, users::remove,
        users::list_profiles, users::get_preferences, users::update_preferences, users::update_home_layout,
        users::get_permissions, users::update_permissions, users::set_pin, users::verify_pin,
        invites::create, invites::list, invites::remove, invites::inspect,
        remote::get_status, remote::test_reachability, remote::update_policy, remote::get_bandwidth,
        oauth::oauth_authorization_server_metadata, oauth::oauth_protected_resource_metadata,
        oauth::register_client, oauth::authorize, oauth::token_endpoint, oauth::revoke_token,
        libraries::list, libraries::get_one, libraries::create, libraries::update,
        libraries::remove, libraries::scan, libraries::browse,
        items::list, items::detail, items::children, items::search, items::home,
        items::mark_watched, items::mark_unwatched,
        items::playback_preview_get, items::playback_preview_post,
        playback::start, playback::progress, playback::stop,
        images::image, events::events, stream::direct, stream::hls_master,
        stream::hls_rendition_file, stream::subtitle,
        mcp::list_audit, mcp::get_status, mcp::update_status, mcp::create_snippets, mcp::list_presets,
        hw::get_report, hw::get_capacity, hw::run_test,
        intelligence::optimize_item, intelligence::get_markers, intelligence::detect_season_intros,
        intelligence::trickplay_manifest, intelligence::trickplay_sprite,
        intelligence::quick_connect_init, intelligence::quick_connect_authorize, intelligence::quick_connect_poll,
        intelligence::index_item_scenes, intelligence::search_scenes, intelligence::spoiler_free_recap,
        intelligence::hailo_benchmark,
        collections::list_collections, collections::create_collection, collections::get_collection,
        collections::update_collection, collections::delete_collection, collections::get_collection_items,
        collections::add_item_to_collection, collections::remove_item_from_collection,
        playlists::list_playlists, playlists::create_playlist, playlists::get_playlist,
        playlists::update_playlist, playlists::delete_playlist, playlists::get_playlist_items,
        playlists::add_item_to_playlist, playlists::remove_item_from_playlist,
        webhooks::list_webhooks, webhooks::create_webhook, webhooks::toggle_webhook, webhooks::delete_webhook,
        requests::list_requests, requests::create_request, requests::update_request_status,
        requests::inspect_missing_episodes, requests::analyze_hardlinks,
        matchmaker::create_matchmaker_session, matchmaker::get_matchmaker_session, matchmaker::record_vote,
        players::list_players, players::register_player, players::send_player_command,
        companion::join_companion_session, companion::send_companion_reaction, companion::get_companion_context,
        media_audio::stream_audio, photos::get_photo_timeline,
    ),
    components(schemas(
        mbl_db::repo::items::SortField,
        mbl_db::repo::items::SortOrder,
        mbl_db::repo::users::UserPreferences,
        mbl_db::repo::users::SubtitleStylePreferences,
        mbl_db::repo::users::UpdateUserPreferences,
        mbl_db::repo::users::HomeLayoutRequest,
        crate::dto::UserProfileDto,
        mbl_service::PlaybackPreview,
        mbl_service::PlayMethod,
        mbl_service::StreamAction,
        mbl_service::SubtitleDelivery,
        mbl_service::HwReport,
        mbl_service::HwSelftestResponse,
        mbl_service::SystemHealthReport,
        mbl_service::CreateOptimizedRequest,
        mbl_service::CreateOptimizedResponse,
        mbl_service::QuickConnectInitRequest,
        mbl_service::QuickConnectInitResponse,
        mbl_service::QuickConnectAuthorizeRequest,
        mbl_service::QuickConnectPollRequest,
        mbl_service::QuickConnectPollResponse,
        mbl_service::SceneHintInput,
        mbl_service::IndexScenesRequest,
        mbl_service::SceneSearchHit,
        intelligence::OptimizeItemBody,
        mbl_platform::PlatformDefaults,
        mbl_platform::ThermalStatus,
        mbl_analysis::OptimizationProfile,
        mbl_analysis::OptimizationEstimate,
        mbl_analysis::TrickplayManifest,
        mbl_analysis::TrickplaySprite,
        mbl_analysis::TrickplayTile,
        mbl_db::repo::m5::MarkerRow,
        mbl_db::repo::m5::SceneEmbeddingRow,
        mbl_hailo::SpoilerFreeRecap,
        mbl_hailo::HailoEmbeddingBenchmark,
        mbl_hwaccel::HardwareFingerprint,
        mbl_hwaccel::NpuProfile,
        mbl_hwaccel::DrmDevice,
        mbl_hwaccel::V4l2Device,
        mbl_hwaccel::NvidiaGpu,
        mbl_hwaccel::DxgiAdapter,
        mbl_hwaccel::HailoDeviceInfo,
        mbl_hwaccel::FfmpegProbeCaps,
        mbl_hwaccel::HwCapEntry,
        mbl_hwaccel::TranscodeCapacity,
        mbl_hwaccel::BadBackendFailure,
        mbl_hwaccel::HailoRuntimeStatus,
        mbl_hwaccel::HailoSharingMode,
        mbl_hwaccel::HailoPipelineCapability,
        mbl_db::repo::m6::UserSecurityInfo,
        mbl_db::repo::m6::InviteRow,
        mbl_db::repo::m6::OAuthClient,
        mbl_service::UpdateUserPermissionsRequest,
        mbl_service::SetPinRequest,
        mbl_service::VerifyPinRequest,
        mbl_service::CreateInviteRequest,
        mbl_service::RegisterInviteRequest,
        mbl_service::RemoteStatusResponse,
        mbl_service::UpdateRemotePolicyRequest,
        mbl_service::BandwidthUsageResponse,
        mbl_auth::totp::TotpSetup,
        mbl_remote::ReachabilityReport,
        mbl_remote::RemoteRecommendation,
        mbl_remote::TailscaleStatus,
        mbl_remote::TailscalePeer,
        mbl_remote::TailscaleWhois,
        mbl_remote::NetworkClass,
        remote::ReachabilityTestRequest,
        oauth::OAuthAuthorizationServerMetadata,
        oauth::OAuthProtectedResourceMetadata,
        oauth::RegisterClientRequest,
        oauth::RegisterClientResponse,
        oauth::AuthorizeParams,
        oauth::TokenForm,
        oauth::TokenResponse,
        oauth::RevokeForm,
        mbl_service::CollectionRow,
        mbl_service::CreateCollectionRequest,
        mbl_service::UpdateCollectionRequest,
        mbl_service::PlaylistRow,
        mbl_service::CreatePlaylistRequest,
        mbl_service::UpdatePlaylistRequest,
        mbl_service::WebhookRow,
        mbl_service::CreateWebhookRequest,
        webhooks::UpdateWebhookStateRequest,
        mbl_service::MediaRequestRow,
        mbl_service::CreateMediaRequest,
        requests::UpdateRequestStatusInput,
        mbl_service::MissingEpisodeInfo,
        mbl_service::StorageHardlinkReport,
        mbl_service::MatchmakerSessionRow,
        mbl_service::CreateMatchmakerSessionRequest,
        mbl_service::MatchmakerVoteRequest,
        mbl_service::MatchmakerMatchResult,
        mbl_service::PlayerInfo,
        mbl_service::PlayerCommand,
        mbl_service::LiveSessionContext,
        mbl_service::CompanionReaction,
        companion::JoinSessionRequest,
        companion::SendReactionRequest,
        mbl_db::repo::m7::PhotoTimelineBucket,
    )),
    modifiers(&Security),
    security(("bearer" = []), ("cookie" = [])),
)]
pub struct ApiDoc;

struct Security;

impl Modify for Security {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(HttpBuilder::new().scheme(HttpAuthScheme::Bearer).build()),
        );
        components.add_security_scheme(
            "cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::new(
                crate::extract::SESSION_COOKIE,
            ))),
        );
    }
}

/// The spec as pretty JSON with a trailing newline (the committed `docs/openapi.json` format).
pub fn spec_json() -> String {
    let mut doc = ApiDoc::openapi();
    doc.info.version = env!("CARGO_PKG_VERSION").to_owned();
    let mut json = doc.to_pretty_json().unwrap_or_default();
    json.push('\n');
    json
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/openapi.json",
            get(|| async { ([(header::CONTENT_TYPE, "application/json")], spec_json()) }),
        )
        .route("/api/docs", get(|| async { Html(SCALAR_HTML) }))
}

const SCALAR_HTML: &str = r#"<!doctype html>
<html>
<head>
  <title>MBL API</title>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
</head>
<body>
  <script id="api-reference" data-url="/api/openapi.json"></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>
"#;
