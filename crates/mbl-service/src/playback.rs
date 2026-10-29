//! Playback session creation (`decide` -> DirectPlay / HLS), progress reporting, and stop handling.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tokio::io::AsyncReadExt;

use camino::Utf8PathBuf;
use mbl_core::time::now_ms;
use mbl_core::{Actor, Error, ItemId, PartId, Result, StreamKind, UserId};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::media;
use mbl_db::repo::watch::{self, NewWatchHistory};
use mbl_jobs::ActivityProbe;
use mbl_playback::{
    Decision, DecisionContext, DeviceProfile, DeviceProfileOverlay, Network, PlayMethod, Reason,
    StreamAction, SubtitleDelivery, SubtitleMethod, decide,
};
use mbl_probe::{Chapter, MediaInfo, StreamInfo};
use mbl_stream::hls::{HlsRendition, render_master_playlist, render_media_playlist};
use mbl_stream::subtitles::is_image_subtitle;
use mbl_transcode::{
    AudioCodec, AudioOp, Backend, CreateSessionReq, HlsOutput, Pipeline, Rendition, SubtitleOp,
    VideoCodec, VideoOp,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::AuthService;
use crate::stream::{StreamService, verify_under_library_roots};
use crate::watch::WatchService;

const DEFAULT_SEGMENT_SECS: u32 = 6;

/// Flexible device profile input accepted by `POST /api/v1/playback`:
/// - a built-in profile name (`"chrome"`, `"firefox"`, `"safari"`, `"edge"`, `"generic-hls"`),
/// - a full [`DeviceProfile`] object, or
/// - a [`DeviceProfileOverlay`] merged onto `"chrome"`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum PlaybackProfileInput {
    Named(String),
    Full(Box<DeviceProfile>),
    Overlay(DeviceProfileOverlay),
}

impl PlaybackProfileInput {
    pub fn resolve(&self) -> Result<DeviceProfile> {
        match self {
            Self::Named(name) => DeviceProfile::builtin(name),
            Self::Full(profile) => {
                profile.validate()?;
                Ok((**profile).clone())
            }
            Self::Overlay(overlay) => overlay.clone().resolve(),
        }
    }
}

/// Request body for `POST /api/v1/playback`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackRequest {
    pub item_id: ItemId,
    pub part_id: Option<PartId>,
    #[schema(value_type = Option<Object>)]
    pub profile: Option<PlaybackProfileInput>,
    pub max_bitrate: Option<u64>,
    pub audio_stream_index: Option<u32>,
    pub subtitle_stream_index: Option<u32>,
    pub start_ms: Option<u64>,
    /// Positive delays audio; negative advances audio (−10000..10000 ms).
    /// Encodes audio; ignored for media with no audio stream.
    #[serde(default)]
    pub audio_offset_ms: i64,
    /// Wait for readable source / first complete HLS media before acknowledging startup.
    /// Opt-in for existing API clients; enabled by the web player for atomic stream changes.
    #[serde(default)]
    pub wait_for_ready: bool,
    /// Set by the trusted HTTP adapter, never accepted from a request body.
    #[serde(skip)]
    pub network: Option<Network>,
}

/// Subtitle track descriptor returned in [`PlaybackResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSubtitleTrack {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub format: String,
    pub url: String,
}

/// Audio streams available in the selected media version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackAudioTrack {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub channels: Option<u8>,
    pub is_default: bool,
}

/// Selectable subtitles, including image tracks requiring a new encoded video stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackSubtitleOption {
    pub index: u32,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub requires_transcode: bool,
}

/// Timeline marker (intro, credits, chapter) returned in [`PlaybackResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackMarker {
    pub kind: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub title: Option<String>,
}

/// Response body for `POST /api/v1/playback`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackResponse {
    pub audio_tracks: Vec<PlaybackAudioTrack>,
    pub audio_stream_index: Option<u32>,
    pub subtitle_options: Vec<PlaybackSubtitleOption>,
    pub burned_subtitle_stream_index: Option<u32>,
    pub audio_offset_ms: i64,
    pub item_id: ItemId,
    pub part_id: PartId,
    pub content_type: String,
    pub session_id: String,
    pub method: PlayMethod,
    pub reasons: Vec<String>,
    pub url: String,
    pub subtitles: Vec<PlaybackSubtitleTrack>,
    pub start_ms: u64,
    pub markers: Vec<PlaybackMarker>,
    pub duration_ms: u64,
}

/// Decision preview returned by `preview_playback`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackPreview {
    pub audio_offset_ms: i64,
    pub item_id: ItemId,
    pub part_id: PartId,
    pub method: PlayMethod,
    pub reasons: Vec<String>,
    pub container: String,
    pub video: StreamAction,
    pub audio: StreamAction,
    pub subtitle: SubtitleDelivery,
    pub duration_ms: u64,
}

/// Client playback state reported to `POST /api/v1/playback/{session}/progress`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    #[default]
    Playing,
    Paused,
    Buffering,
}

/// Request body for `POST /api/v1/playback/{session}/progress`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackProgressRequest {
    pub position_ms: u64,
    #[serde(default)]
    pub state: PlaybackState,
}

/// Optional request body for `POST /api/v1/playback/{session}/stop`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackStopRequest {
    pub position_ms: Option<u64>,
}

/// Live playback session metadata tracked in memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlaybackSession {
    #[serde(skip)]
    record_history: bool,
    pub audio_stream_index: Option<u32>,
    pub burned_subtitle_stream_index: Option<u32>,
    pub audio_offset_ms: i64,
    pub title: String,
    pub session_id: String,
    pub user_id: UserId,
    pub item_id: ItemId,
    pub part_id: PartId,
    pub method: PlayMethod,
    pub reasons: Vec<String>,
    pub profile_name: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub state: PlaybackState,
    pub started_at: i64,
    pub last_updated_at: i64,
    pub is_hls: bool,
    pub marked_watched: bool,
    pub network: Network,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TranscodeDiagnostics {
    pub backend: String,
    pub process_running: bool,
    pub throttled: bool,
    pub restarts: u32,
    pub last_exit_failed: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackDiagnostics {
    #[serde(flatten)]
    pub session: ActivePlaybackSession,
    pub transcode: Option<TranscodeDiagnostics>,
}

#[derive(Debug, Clone)]
pub struct PlaybackService {
    db: Db,
    libraries: LibrariesRepo,
    auth: AuthService,
    stream: StreamService,
    watch: WatchService,
    hw: Option<crate::hw::HwService>,
    thermal_root: Option<std::path::PathBuf>,
    sessions: Arc<RwLock<HashMap<String, ActivePlaybackSession>>>,
    starting: Arc<AtomicUsize>,
    closing: Arc<AtomicBool>,
    bandwidth: Arc<mbl_remote::BandwidthManager>,
    public_url: Option<String>,
}

impl ActivityProbe for PlaybackService {
    fn playback_active(&self) -> bool {
        if self.starting.load(Ordering::Acquire) > 0
            || self
                .stream
                .hls_manager()
                .is_some_and(|m| m.active_count() > 0 || m.active_video_transcode_count() > 0)
        {
            return true;
        }
        let now = now_ms();
        let Ok(map) = self.sessions.read() else {
            return false;
        };
        map.values().any(|s| now.saturating_sub(s.last_updated_at) <= 60_000)
    }

    fn thermal_throttled(&self) -> bool {
        match &self.thermal_root {
            Some(root) => mbl_platform::thermal::read_thermal_status_from_root(root),
            None => mbl_platform::thermal::read_thermal_status(),
        }
        .should_pause_idle_jobs()
    }
}

impl PlaybackService {
    pub fn new(db: Db, auth: AuthService, stream: StreamService, watch: WatchService) -> Self {
        Self {
            libraries: LibrariesRepo::new(db.clone()),
            db,
            auth,
            stream,
            watch,
            hw: None,
            thermal_root: None,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            starting: Arc::default(),
            closing: Arc::default(),
            bandwidth: Arc::new(mbl_remote::BandwidthManager::default()),
            public_url: None,
        }
    }

    pub fn with_remote_policy(
        mut self,
        bandwidth: Arc<mbl_remote::BandwidthManager>,
        public_url: &str,
    ) -> Self {
        self.bandwidth = bandwidth;
        self.public_url =
            (!public_url.trim().is_empty()).then(|| public_url.trim_end_matches('/').to_owned());
        self
    }

    pub fn with_stream(mut self, stream: StreamService) -> Self {
        self.stream = stream.clone();
        if let Some(hw) = self.hw.take() {
            self.hw = Some(hw.with_stream(stream));
        }
        self
    }

    pub fn with_hw(mut self, hw: crate::hw::HwService) -> Self {
        self.hw = Some(hw);
        self
    }

    pub fn with_thermal_root(mut self, root: impl Into<std::path::PathBuf>) -> Self {
        self.thermal_root = Some(root.into());
        self
    }

    async fn build_decision_context(&self, req: &PlaybackRequest) -> DecisionContext {
        let mut can_transcode_video = true;
        let mut hw = None;

        if let Ok(rows) = mbl_db::repo::hw::list_all(&self.db).await
            && !rows.is_empty()
            && let Some(hw_svc) = &self.hw
            && let Ok((cap_ok, hw_sum, _)) = hw_svc.decision_summary().await
        {
            can_transcode_video = cap_ok;
            hw = hw_sum;
        }

        let network = req.network.unwrap_or_default();
        let max_bitrate = if network == Network::Wan {
            let recommended =
                self.bandwidth.calculate_wan_fair_share(None).saturating_mul(1000).min(8_000_000);
            Some(req.max_bitrate.map_or(recommended, |n| n.min(recommended)))
        } else {
            req.max_bitrate
        };
        DecisionContext {
            max_bitrate,
            network,
            can_transcode_video,
            hw,
            audio_index: req.audio_stream_index,
            subtitle_index: req.subtitle_stream_index,
        }
    }

    /// Resolve the best part for playback: when `part_id` is omitted and the item has multiple
    /// versions (e.g. an optimized H.264+AAC MP4 version from M5.4), prefer a version that can
    /// `DirectPlay` on `profile` + `ctx` over one that requires transcoding.
    async fn select_best_part(
        &self,
        req: &PlaybackRequest,
        profile: &DeviceProfile,
        ctx: &DecisionContext,
    ) -> Result<(media::StreamPart, MediaInfo, Decision)> {
        if let Some(pid) = req.part_id {
            let part = media::part_for_stream(&self.db, pid).await?.ok_or(Error::NotFound)?;
            if part.item_id != req.item_id {
                return Err(Error::NotFound);
            }
            let info = self.load_media_info(&part).await?;
            let dec = decide(&info, profile, ctx);
            return Ok((part, info, dec));
        }

        let parts = media::parts_for_item(&self.db, req.item_id).await?;
        let available: Vec<media::StreamPart> =
            parts.iter().filter(|p| p.missing_since.is_none()).cloned().collect();
        let mut candidates = if available.is_empty() { parts } else { available };
        candidates.sort_by_key(|p| p.path.contains(".optimized-"));
        let first = candidates.first().cloned().ok_or(Error::NotFound)?;
        let first_info = self.load_media_info(&first).await?;
        let first_dec = decide(&first_info, profile, ctx);

        if first_dec.method == PlayMethod::DirectPlay || candidates.len() == 1 {
            return Ok((first, first_info, first_dec));
        }

        for cand in candidates.into_iter().skip(1) {
            if let Ok(info) = self.load_media_info(&cand).await {
                let dec = decide(&info, profile, ctx);
                if dec.method == PlayMethod::DirectPlay {
                    return Ok((cand, info, dec));
                }
            }
        }

        Ok((first, first_info, first_dec))
    }

    /// Preview playback decision without creating an active session or updating watch state.
    pub async fn preview(&self, actor: &Actor, req: PlaybackRequest) -> Result<PlaybackPreview> {
        let profile = match &req.profile {
            Some(input) => input.resolve()?,
            None => DeviceProfile::builtin("chrome")?,
        };
        let ctx = self.build_decision_context(&req).await;
        let (part, media_info, mut decision) = self.select_best_part(&req, &profile, &ctx).await?;

        if !actor.can_access_library(part.library_id) {
            return Err(Error::NotFound);
        }
        let rating = mbl_db::repo::m6::get_item_content_rating(&self.db, part.item_id).await?;
        if !actor.can_view_rating(rating.as_deref()) {
            return Err(Error::NotFound);
        }
        let lib = self.libraries.get(part.library_id).await?.ok_or(Error::NotFound)?;
        let _ = verify_under_library_roots(&part.path, &lib.paths)?;

        validate_stream_selection(&req, &media_info)?;
        let audio_offset_ms =
            apply_audio_offset(&mut decision, &profile, &media_info, req.audio_offset_ms)?;
        let duration_ms = if media_info.duration_ms > 0 {
            media_info.duration_ms
        } else {
            part.duration_ms.and_then(|d| u64::try_from(d).ok()).unwrap_or(0)
        };
        let mut reasons: Vec<String> = decision.reasons.iter().map(Reason::message).collect();
        if audio_offset_ms != 0 {
            reasons.push(format!("Audio synchronization: {} ms (audio encoded)", audio_offset_ms));
        }

        Ok(PlaybackPreview {
            audio_offset_ms,
            item_id: part.item_id,
            part_id: part.id,
            method: decision.method,
            reasons,
            container: decision.container,
            video: decision.video,
            audio: decision.audio,
            subtitle: decision.subtitle,
            duration_ms,
        })
    }

    /// Start a playback session: run the pure decision engine, create an HLS session when remuxing
    /// or transcoding, and return the stream URL, subtitle tracks, start offset, and markers.
    pub async fn start(&self, actor: &Actor, req: PlaybackRequest) -> Result<PlaybackResponse> {
        if self.closing.load(Ordering::Acquire) {
            return Err(Error::Unavailable("playback server is shutting down".into()));
        }
        let item = crate::ItemService::new(self.db.clone()).get(actor, req.item_id).await?;
        let profile = match &req.profile {
            Some(input) => input.resolve()?,
            None => DeviceProfile::builtin("chrome")?,
        };
        let ctx = self.build_decision_context(&req).await;
        let (part, media_info, mut decision) = self.select_best_part(&req, &profile, &ctx).await?;

        if !actor.can_access_library(part.library_id) {
            return Err(Error::NotFound);
        }
        let rating = mbl_db::repo::m6::get_item_content_rating(&self.db, part.item_id).await?;
        if !actor.can_view_rating(rating.as_deref()) {
            return Err(Error::NotFound);
        }
        let lib = self.libraries.get(part.library_id).await?.ok_or(Error::NotFound)?;
        let verified_path = verify_under_library_roots(&part.path, &lib.paths)?;

        validate_stream_selection(&req, &media_info)?;
        let audio_offset_ms =
            apply_audio_offset(&mut decision, &profile, &media_info, req.audio_offset_ms)?;
        let duration_ms = if media_info.duration_ms > 0 {
            media_info.duration_ms
        } else {
            part.duration_ms.and_then(|d| u64::try_from(d).ok()).unwrap_or(0)
        };

        let start_ms = match req.start_ms {
            Some(ms) => ms,
            None => {
                let ws = watch::get(&self.db, actor.user_id, part.item_id).await?;
                u64::try_from(ws.view_offset_ms.max(0)).unwrap_or(0)
            }
        };

        let stream_token = self.auth.sign_stream_token(actor, part.id);
        let subtitles = build_subtitle_tracks(&media_info, &profile, part.id, &stream_token);
        let markers = self.load_markers(part.item_id, part.id, &media_info).await;

        let _starting = PlaybackStarting::new(self.starting.clone());
        let (session_id, url, is_hls) = match decision.method {
            PlayMethod::DirectPlay => {
                if req.wait_for_ready {
                    let mut file = tokio::fs::File::open(&verified_path).await.map_err(|_| {
                        Error::Unavailable("media source could not be opened".into())
                    })?;
                    if file.read(&mut [0u8; 1]).await.map_err(Error::internal)? == 0 {
                        return Err(Error::Unavailable("media source is empty".into()));
                    }
                }
                let sid = generate_session_id()?;
                let url = format!("/api/v1/stream/direct/{}?token={stream_token}", part.id.0);
                (sid, url, false)
            }
            PlayMethod::DirectStream | PlayMethod::Transcode => {
                let sid = self
                    .start_hls_session(
                        actor.user_id,
                        part.id,
                        &verified_path,
                        &media_info,
                        &decision,
                        &ctx,
                        start_ms,
                        duration_ms,
                        audio_offset_ms,
                        &lib.paths,
                        req.wait_for_ready,
                    )
                    .await?;
                let url = format!("/api/v1/stream/hls/{sid}/master.m3u8");
                (sid, url, true)
            }
        };

        if self.closing.load(Ordering::Acquire) {
            if is_hls && let Some(manager) = self.stream.hls_manager() {
                manager.stop_session(&session_id).await;
            }
            return Err(Error::Unavailable("playback server is shutting down".into()));
        }

        let mut reasons: Vec<String> = decision.reasons.iter().map(Reason::message).collect();
        if audio_offset_ms != 0 {
            reasons.push(format!("Audio synchronization: {} ms (audio encoded)", audio_offset_ms));
        }
        let audio_stream_index = req.audio_stream_index.or_else(|| {
            media_info
                .streams_of(StreamKind::Audio)
                .find(|s| s.default)
                .or_else(|| media_info.streams_of(StreamKind::Audio).next())
                .map(|s| s.index)
        });
        let burned_subtitle_stream_index =
            if decision.subtitle == SubtitleDelivery::BurnIn { ctx.subtitle_index } else { None };
        let audio_tracks = media_info
            .streams_of(StreamKind::Audio)
            .map(|s| PlaybackAudioTrack {
                index: s.index,
                codec: s.codec.clone(),
                language: s.language.clone(),
                title: s.title.clone(),
                channels: s.channels,
                is_default: s.default,
            })
            .collect();
        let subtitle_options = media_info
            .streams_of(StreamKind::Subtitle)
            .map(|s| PlaybackSubtitleOption {
                index: s.index,
                codec: s.codec.clone(),
                language: s.language.clone(),
                title: s.title.clone(),
                is_default: s.default,
                is_forced: s.forced,
                requires_transcode: subtitle_requires_transcode(&s.codec, &profile),
            })
            .collect();
        let now = now_ms();
        let active = ActivePlaybackSession {
            record_history: !req.wait_for_ready,
            audio_stream_index,
            burned_subtitle_stream_index,
            audio_offset_ms,
            title: item.title.clone(),
            session_id: session_id.clone(),
            user_id: actor.user_id,
            item_id: part.item_id,
            part_id: part.id,
            method: decision.method,
            reasons: reasons.clone(),
            profile_name: profile.name.clone(),
            position_ms: start_ms,
            duration_ms,
            state: if req.wait_for_ready {
                PlaybackState::Buffering
            } else {
                PlaybackState::Playing
            },
            started_at: now,
            last_updated_at: now,
            is_hls,
            marked_watched: false,
            network: ctx.network,
        };

        let accepted = if let Ok(mut map) = self.sessions.write() {
            if self.closing.load(Ordering::Acquire) {
                false
            } else {
                if ctx.network == Network::Wan {
                    self.bandwidth.register_session(&session_id);
                }
                map.insert(session_id.clone(), active);
                true
            }
        } else {
            false
        };
        if !accepted {
            if is_hls && let Some(manager) = self.stream.hls_manager() {
                manager.stop_session(&session_id).await;
            }
            return Err(Error::Unavailable("playback server is shutting down".into()));
        }
        let receiver_url = |path: String| -> String {
            if profile.name == "chromecast"
                && let Some(base) = &self.public_url
            {
                format!("{base}{path}")
            } else {
                path
            }
        };
        let content_type = if is_hls {
            "application/vnd.apple.mpegurl"
        } else if media_info.primary_video().is_some() {
            "video/mp4"
        } else if media_info.container.split(',').any(|n| n == "mp3") {
            "audio/mpeg"
        } else if media_info.container == "aac" {
            "audio/aac"
        } else {
            "audio/mp4"
        };
        let subtitles = subtitles
            .into_iter()
            .map(|mut subtitle| {
                subtitle.url = receiver_url(subtitle.url);
                subtitle
            })
            .collect();
        Ok(PlaybackResponse {
            audio_tracks,
            audio_stream_index,
            subtitle_options,
            burned_subtitle_stream_index,
            audio_offset_ms,
            item_id: part.item_id,
            part_id: part.id,
            content_type: content_type.into(),
            session_id,
            method: decision.method,
            reasons,
            url: receiver_url(url),
            subtitles,
            start_ms,
            markers,
            duration_ms,
        })
    }

    /// Report playback progress (`POST /api/v1/playback/{session}/progress`).
    pub async fn progress(
        &self,
        actor: &Actor,
        session_id: &str,
        req: PlaybackProgressRequest,
    ) -> Result<()> {
        // Check ownership before renewing the HLS lease. Diagnostics and foreign heartbeats
        // must never keep another user's stream alive or revive an evicted session.
        let existing = self.get_session(actor, session_id).ok_or(Error::NotFound)?;
        if existing.user_id != actor.user_id {
            return Err(Error::NotFound);
        }
        if existing.is_hls {
            self.stream.hls_session(session_id)?.renew_activity().await?;
        }
        let (item_id, duration_ms, already_watched, record_progress) = {
            let mut map = self
                .sessions
                .write()
                .map_err(|_| Error::Unavailable("session lock poisoned".into()))?;
            let session = map.get_mut(session_id).ok_or(Error::NotFound)?;
            if session.user_id != actor.user_id {
                return Err(Error::NotFound);
            }
            session.record_history |= req.state != PlaybackState::Buffering;
            session.position_ms = req.position_ms;
            session.state = req.state;
            session.last_updated_at = now_ms();
            (session.item_id, session.duration_ms, session.marked_watched, session.record_history)
        };

        self.bandwidth.record_feedback(session_id, None, None);
        if !record_progress {
            return Ok(());
        }
        let newly_watched = self
            .watch
            .record_progress(actor, item_id, req.position_ms, duration_ms, !already_watched)
            .await?;

        if newly_watched
            && let Ok(mut map) = self.sessions.write()
            && let Some(session) = map.get_mut(session_id)
        {
            session.marked_watched = true;
        }

        Ok(())
    }

    /// Stop an active playback session (`POST /api/v1/playback/{session}/stop`), recording a
    /// `watch_history` entry and tearing down any associated HLS session.
    pub async fn stop(
        &self,
        actor: &Actor,
        session_id: &str,
        req: PlaybackStopRequest,
    ) -> Result<()> {
        let session = {
            let mut map = self
                .sessions
                .write()
                .map_err(|_| Error::Unavailable("session lock poisoned".into()))?;
            let existing = map.get(session_id).ok_or(Error::NotFound)?;
            if existing.user_id != actor.user_id {
                return Err(Error::NotFound);
            }
            map.remove(session_id).ok_or(Error::NotFound)?
        };

        self.finish_session(actor, session, req.position_ms, true).await
    }

    async fn finish_session(
        &self,
        actor: &Actor,
        session: ActivePlaybackSession,
        position_ms: Option<u64>,
        record_final_progress: bool,
    ) -> Result<()> {
        let session_id = &session.session_id;
        self.bandwidth.unregister_session(session_id);
        if session.is_hls
            && let Some(hls_mgr) = self.stream.hls_manager()
        {
            hls_mgr.stop_session(session_id).await;
        }
        if !session.record_history {
            return Ok(());
        }
        let final_pos = position_ms.unwrap_or(session.position_ms);
        // Expiry/shutdown use the last persisted heartbeat. Replaying it could overwrite
        // newer progress from a replacement session for the same title.
        if record_final_progress {
            let _ = self
                .watch
                .record_progress(
                    actor,
                    session.item_id,
                    final_pos,
                    session.duration_ms,
                    !session.marked_watched,
                )
                .await;
        }

        let method_str = match session.method {
            PlayMethod::DirectPlay => "directPlay",
            PlayMethod::DirectStream => "directStream",
            PlayMethod::Transcode => "transcode",
        };
        watch::insert_history(
            &self.db,
            &NewWatchHistory {
                user_id: actor.user_id,
                item_id: session.item_id,
                started_at: session.started_at,
                stopped_at: now_ms(),
                method: method_str,
                client: Some(&session.profile_name),
                network: if session.network == Network::Wan {
                    "wan"
                } else if session.network.is_remote() {
                    "tailnet"
                } else {
                    "lan"
                },
            },
        )
        .await?;

        Ok(())
    }

    /// Reap HLS leases and matching playback metadata, including abandoned direct sessions.
    pub async fn maintain_sessions(&self) {
        let manager = self.stream.hls_manager();
        if let Some(manager) = manager {
            manager.run_maintenance_once().await;
        }
        let timeout = manager
            .map_or(mbl_transcode::DEFAULT_INACTIVITY_TIMEOUT, |m| m.config().inactivity_timeout);
        let timeout_ms = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
        let now = now_ms();
        // Decide and remove under one lock, so a renewed direct lease cannot be reaped
        // using an older snapshot. HLS expiry is decided atomically by its own manager.
        let expired = if let Ok(mut map) = self.sessions.write() {
            let ids: Vec<_> = map
                .values()
                .filter(|session| {
                    if session.is_hls {
                        manager.is_none_or(|m| m.get_session(&session.session_id).is_none())
                    } else {
                        now.saturating_sub(session.last_updated_at) >= timeout_ms
                    }
                })
                .map(|s| s.session_id.clone())
                .collect();
            ids.into_iter().filter_map(|id| map.remove(&id)).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for session in expired {
            let actor = Actor::admin(session.user_id);
            if let Err(error) = self.finish_session(&actor, session, None, false).await {
                tracing::warn!(%error, "failed to finalize expired playback session");
            }
        }
    }

    /// Reject new starts and tear down children before the server closes its database.
    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        if let Some(manager) = self.stream.hls_manager() {
            manager.close().await;
        }
        let sessions = self
            .sessions
            .write()
            .map(|mut m| m.drain().map(|(_, s)| s).collect::<Vec<_>>())
            .unwrap_or_default();
        for session in sessions {
            let actor = Actor::admin(session.user_id);
            if let Err(error) = self.finish_session(&actor, session, None, false).await {
                tracing::warn!(%error, "failed to finalize playback during shutdown");
            }
        }
    }

    /// List active playback sessions visible to `actor` (admins see all, users see their own).
    pub fn list_active(&self, actor: &Actor) -> Vec<ActivePlaybackSession> {
        let Ok(map) = self.sessions.read() else {
            return Vec::new();
        };
        map.values().filter(|s| actor.is_admin || s.user_id == actor.user_id).cloned().collect()
    }

    /// Get an active playback session by ID visible to `actor`.
    pub async fn diagnostics(&self, actor: &Actor, id: &str) -> Result<PlaybackDiagnostics> {
        let session = self.get_session(actor, id).ok_or(Error::NotFound)?;
        let hls = self.stream.hls_manager().and_then(|manager| manager.get_session(id));
        let transcode = if let Some(hls) = hls {
            let runtime = hls.runtime_status().await;
            Some(TranscodeDiagnostics {
                backend: runtime.backend.as_str().into(),
                process_running: runtime.process_running,
                throttled: runtime.throttled,
                restarts: runtime.restarts,
                last_exit_failed: runtime.last_exit_failed,
            })
        } else {
            None
        };
        Ok(PlaybackDiagnostics { session, transcode })
    }

    pub fn get_session(&self, actor: &Actor, session_id: &str) -> Option<ActivePlaybackSession> {
        let Ok(map) = self.sessions.read() else {
            return None;
        };
        map.get(session_id).filter(|s| actor.is_admin || s.user_id == actor.user_id).cloned()
    }

    async fn load_media_info(&self, part: &media::StreamPart) -> Result<MediaInfo> {
        let db_streams = media::streams_for_part(&self.db, part.id).await?;
        if let Ok(mut info) = serde_json::from_str::<MediaInfo>(&part.probe)
            && !info.container.is_empty()
        {
            for ds in db_streams {
                let idx = u32::try_from(ds.idx).unwrap_or(0);
                if !info.streams.iter().any(|s| s.index == idx) {
                    info.streams.push(StreamInfo {
                        index: idx,
                        kind: ds.kind,
                        codec: ds.codec,
                        profile: None,
                        level: None,
                        language: ds.language,
                        title: ds.title,
                        default: ds.is_default,
                        forced: ds.is_forced,
                        external_path: ds.external_path,
                        width: None,
                        height: None,
                        bit_depth: None,
                        frame_rate: None,
                        interlaced: false,
                        hdr: None,
                        channels: None,
                        channel_layout: None,
                        sample_rate: None,
                    });
                }
            }
            return Ok(info);
        }

        // Fallback when `part.probe` is `{}` in synthetic tests: build `MediaInfo` from DB rows.
        let streams = if db_streams.is_empty() {
            vec![
                StreamInfo {
                    index: 0,
                    kind: StreamKind::Video,
                    codec: "h264".into(),
                    profile: Some("high".into()),
                    level: Some(41),
                    language: None,
                    title: None,
                    default: true,
                    forced: false,
                    external_path: None,
                    width: Some(1920),
                    height: Some(1080),
                    bit_depth: Some(8),
                    frame_rate: Some(24.0),
                    interlaced: false,
                    hdr: None,
                    channels: None,
                    channel_layout: None,
                    sample_rate: None,
                },
                StreamInfo {
                    index: 1,
                    kind: StreamKind::Audio,
                    codec: "aac".into(),
                    profile: None,
                    level: None,
                    language: Some("en".into()),
                    title: None,
                    default: true,
                    forced: false,
                    external_path: None,
                    width: None,
                    height: None,
                    bit_depth: None,
                    frame_rate: None,
                    interlaced: false,
                    hdr: None,
                    channels: Some(2),
                    channel_layout: Some("stereo".into()),
                    sample_rate: Some(48_000),
                },
            ]
        } else {
            db_streams
                .into_iter()
                .map(|ds| StreamInfo {
                    index: u32::try_from(ds.idx).unwrap_or(0),
                    kind: ds.kind,
                    codec: ds.codec,
                    profile: None,
                    level: None,
                    language: ds.language,
                    title: ds.title,
                    default: ds.is_default,
                    forced: ds.is_forced,
                    external_path: ds.external_path,
                    width: (ds.kind == StreamKind::Video).then_some(1920),
                    height: (ds.kind == StreamKind::Video).then_some(1080),
                    bit_depth: (ds.kind == StreamKind::Video).then_some(8),
                    frame_rate: (ds.kind == StreamKind::Video).then_some(24.0),
                    interlaced: false,
                    hdr: None,
                    channels: (ds.kind == StreamKind::Audio).then_some(2),
                    channel_layout: None,
                    sample_rate: None,
                })
                .collect()
        };

        Ok(MediaInfo {
            container: part.container.clone().unwrap_or_else(|| "mp4".into()),
            duration_ms: part.duration_ms.and_then(|d| u64::try_from(d).ok()).unwrap_or(60_000),
            bitrate: Some(4_000_000),
            size: u64::try_from(part.size.max(0)).unwrap_or(0),
            chapters: Vec::new(),
            streams,
            tags: BTreeMap::new(),
        })
    }

    async fn load_markers(
        &self,
        item_id: ItemId,
        part_id: PartId,
        media_info: &MediaInfo,
    ) -> Vec<PlaybackMarker> {
        let chapters: Vec<Chapter> = if !media_info.chapters.is_empty() {
            media_info.chapters.clone()
        } else if let Ok(Some(raw)) = media::part_chapters(&self.db, part_id).await {
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut out: Vec<PlaybackMarker> = chapters
            .into_iter()
            .map(|c| {
                let lower = c.title.as_deref().unwrap_or("").to_ascii_lowercase();
                let kind = if lower.contains("intro") || lower.contains("opening") {
                    "intro"
                } else if lower.contains("credit") || lower.contains("ending") {
                    "credits"
                } else {
                    "chapter"
                };
                PlaybackMarker {
                    kind: kind.to_owned(),
                    start_ms: c.start_ms,
                    end_ms: c.end_ms,
                    title: c.title,
                }
            })
            .collect();

        if let Ok(db_rows) =
            mbl_db::repo::m5::MarkersRepo::new(self.db.clone()).list_for_item(item_id).await
        {
            for r in db_rows {
                let start_ms = u64::try_from(r.start_ms.max(0)).unwrap_or(0);
                let end_ms = u64::try_from(r.end_ms.max(0)).unwrap_or(0);
                if !out.iter().any(|m| m.kind == r.kind && m.start_ms.abs_diff(start_ms) <= 1_000) {
                    out.push(PlaybackMarker { kind: r.kind, start_ms, end_ms, title: r.title });
                }
            }
        }
        out.sort_by_key(|m| m.start_ms);
        out
    }

    #[allow(clippy::too_many_arguments)]
    async fn start_hls_session(
        &self,
        user_id: UserId,
        part_id: PartId,
        verified_path: &std::path::Path,
        media_info: &MediaInfo,
        decision: &Decision,
        ctx: &DecisionContext,
        start_ms: u64,
        duration_ms: u64,
        audio_offset_ms: i64,
        library_roots: &[String],
        wait_for_ready: bool,
    ) -> Result<String> {
        let Some(hls_mgr) = self.stream.hls_manager() else {
            return Err(Error::Unavailable("HLS session manager is unavailable".into()));
        };

        let video_stream = media_info.primary_video();
        let audio_stream = ctx
            .audio_index
            .and_then(|idx| {
                media_info.streams.iter().find(|s| s.kind == StreamKind::Audio && s.index == idx)
            })
            .or_else(|| {
                media_info
                    .streams_of(StreamKind::Audio)
                    .find(|s| s.default)
                    .or_else(|| media_info.streams_of(StreamKind::Audio).next())
            });

        let selected_backend = ctx
            .hw
            .as_ref()
            .and_then(|h| h.backend.parse::<Backend>().ok())
            .unwrap_or(Backend::Software);
        let can_tonemap = ctx.hw.as_ref().is_some_and(|h| h.can_tonemap);
        let needs_tonemap = decision.reasons.contains(&Reason::HdrUnsupported) && can_tonemap;

        let video_op = match &decision.video {
            StreamAction::Copy => VideoOp::Copy,
            StreamAction::Transcode { codec, bitrate, width } => VideoOp::Encode {
                codec: codec.parse::<VideoCodec>().unwrap_or(VideoCodec::H264),
                width: *width,
                max_bitrate: *bitrate,
                deinterlace: video_stream.is_some_and(|v| v.interlaced),
                tonemap: needs_tonemap,
            },
            StreamAction::Drop => VideoOp::None,
        };

        let audio_index = audio_stream.map_or(1, |a| a.index);
        let mut audio_op = match &decision.audio {
            StreamAction::Copy => AudioOp::Copy { index: audio_index },
            StreamAction::Transcode { codec, bitrate, .. } => AudioOp::Encode {
                index: audio_index,
                codec: codec.parse::<AudioCodec>().unwrap_or(AudioCodec::Aac),
                channels: audio_stream.and_then(|a| a.channels).unwrap_or(2).min(6),
                bitrate: *bitrate,
            },
            StreamAction::Drop => AudioOp::None,
        };

        if audio_offset_ms != 0
            && let AudioOp::Encode { index, codec, channels, bitrate } = audio_op
        {
            audio_op = AudioOp::EncodeShifted {
                index,
                codec,
                channels,
                bitrate,
                offset_ms: audio_offset_ms,
            };
        }

        let subtitle_op = match (&decision.subtitle, ctx.subtitle_index) {
            (SubtitleDelivery::BurnIn, Some(sub_idx)) => {
                let is_img = media_info
                    .streams
                    .iter()
                    .find(|s| s.index == sub_idx)
                    .is_some_and(|s| is_image_subtitle(&s.codec));
                if is_img {
                    SubtitleOp::BurnImage { index: sub_idx }
                } else {
                    let selected = media_info
                        .streams_of(StreamKind::Subtitle)
                        .find(|s| s.index == sub_idx)
                        .ok_or(Error::NotFound)?;
                    if let Some(path) = &selected.external_path {
                        let path = verify_under_library_roots(path, library_roots)?;
                        SubtitleOp::BurnTextFile {
                            path: Utf8PathBuf::from_path_buf(path).map_err(|_| {
                                Error::Validation("subtitle path is not UTF-8".into())
                            })?,
                        }
                    } else {
                        // The subtitles filter counts subtitle streams, unlike FFmpeg's absolute -map indices.
                        let ordinal = media_info
                            .streams_of(StreamKind::Subtitle)
                            .filter(|s| s.external_path.is_none())
                            .position(|s| s.index == sub_idx)
                            .ok_or(Error::NotFound)?;
                        SubtitleOp::BurnText {
                            index: u32::try_from(ordinal).map_err(Error::internal)?,
                        }
                    }
                }
            }
            _ => SubtitleOp::None,
        };

        let max_bitrate = match &decision.video {
            StreamAction::Transcode { bitrate, .. } => *bitrate,
            _ => media_info.bitrate.unwrap_or(6_000_000),
        };
        let width = match &decision.video {
            StreamAction::Transcode { width: Some(w), .. } => Some(*w),
            _ => video_stream.and_then(|v| v.width),
        };
        let height = video_stream.and_then(|v| v.height);

        let initial_seg = segment_index_for_ms(start_ms, DEFAULT_SEGMENT_SECS);
        let input_utf8 = Utf8PathBuf::from_path_buf(verified_path.to_path_buf())
            .map_err(|_| Error::Validation("media path is not valid UTF-8".into()))?;

        let pipeline = Pipeline {
            input: input_utf8,
            start_ms,
            video: video_op,
            audio: audio_op,
            subtitle: subtitle_op,
            output: HlsOutput {
                dir: Utf8PathBuf::from("/tmp/mbl-hls"),
                segment_secs: DEFAULT_SEGMENT_SECS,
                start_number: initial_seg,
                renditions: vec![Rendition { name: "0".to_owned(), width, max_bitrate }],
            },
            backend: selected_backend,
        };

        let eff_duration = if duration_ms == 0 { 6_000 } else { duration_ms };
        let video_codec_str = match &decision.video {
            StreamAction::Transcode { codec, .. } => Some(codec.clone()),
            _ => Some("h264".to_owned()),
        };
        let master = render_master_playlist(&[HlsRendition {
            name: "0".to_owned(),
            width,
            height,
            bandwidth: max_bitrate,
            video_codec: video_codec_str,
            audio_codec: audio_stream.map(|_| match &decision.audio {
                StreamAction::Transcode { codec, .. } => codec.clone(),
                _ => audio_stream.map_or("aac", |s| s.codec.as_str()).to_owned(),
            }),
        }]);
        let media_m3u8 = render_media_playlist(eff_duration, DEFAULT_SEGMENT_SECS);

        // Admission failure must leave existing playback untouched. The client
        // releases its prior session only after a new session has been accepted.
        let session = hls_mgr
            .create_session(CreateSessionReq {
                user_id,
                part_id,
                duration_ms: eff_duration,
                pipeline,
                master_playlist: master,
                media_playlists: vec![("0".to_owned(), media_m3u8)],
                fast_start_segments: true,
            })
            .await?;
        let mut candidate = HlsStartup { session: Some((hls_mgr.clone(), session.clone())) };
        if wait_for_ready && let Err(error) = session.prepare().await {
            candidate.abort().await;
            return Err(error);
        }
        candidate.session = None;
        Ok(session.id.clone())
    }
}

/// Counts in-flight starts so idle analysis yields while FFmpeg prepares its first fragment.
struct PlaybackStarting(Arc<AtomicUsize>);
impl PlaybackStarting {
    fn new(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self(count)
    }
}
impl Drop for PlaybackStarting {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A cancelled HTTP future must not leave a candidate child or capacity reservation behind.
struct HlsStartup {
    session: Option<(mbl_transcode::SessionManager, Arc<mbl_transcode::HlsSession>)>,
}
impl HlsStartup {
    async fn abort(&mut self) {
        if let Some((manager, session)) = &self.session
            && !manager.stop_session(&session.id).await
        {
            session.shutdown().await;
        }
        self.session = None;
    }
}
impl Drop for HlsStartup {
    fn drop(&mut self) {
        if let Some((manager, session)) = self.session.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            runtime.spawn(async move {
                if !manager.stop_session(&session.id).await {
                    session.shutdown().await;
                }
            });
        }
    }
}

fn validate_stream_selection(req: &PlaybackRequest, info: &MediaInfo) -> Result<()> {
    for (index, kind, label) in [
        (req.audio_stream_index, StreamKind::Audio, "audioStreamIndex"),
        (req.subtitle_stream_index, StreamKind::Subtitle, "subtitleStreamIndex"),
    ] {
        if let Some(index) = index
            && !info.streams_of(kind).any(|s| s.index == index)
        {
            return Err(Error::Validation(format!(
                "{label} does not identify an available stream"
            )));
        }
    }
    Ok(())
}

fn subtitle_requires_transcode(codec: &str, profile: &DeviceProfile) -> bool {
    match profile.subtitle_rule(codec) {
        Some(rule) => rule.method == SubtitleMethod::Burn,
        None => !matches!(
            codec.to_ascii_lowercase().as_str(),
            "subrip" | "srt" | "ass" | "ssa" | "webvtt" | "vtt" | "mov_text" | "text"
        ),
    }
}

fn apply_audio_offset(
    decision: &mut Decision,
    profile: &DeviceProfile,
    info: &MediaInfo,
    offset: i64,
) -> Result<i64> {
    if !(-10_000..=10_000).contains(&offset) {
        return Err(Error::Validation("audioOffsetMs must be within -10000..10000".into()));
    }
    if offset == 0 || info.streams_of(StreamKind::Audio).next().is_none() {
        return Ok(0);
    }
    let codec = profile
        .transcode_target
        .audio_codecs
        .iter()
        .find_map(|name| name.parse::<AudioCodec>().ok())
        .ok_or_else(|| Error::Validation("profile has no supported audio encoder".into()))?;
    decision.audio =
        StreamAction::Transcode { codec: codec.as_str().into(), bitrate: 192_000, width: None };
    decision.method = PlayMethod::Transcode;
    decision.container = profile.transcode_target.container.clone();
    Ok(offset)
}

fn segment_index_for_ms(start_ms: u64, segment_secs: u32) -> u32 {
    if start_ms == 0 {
        return 0;
    }
    let seg_ms = u64::from(segment_secs.max(1)) * 1_000;
    if segment_secs >= 6 {
        if start_ms < 3_000 {
            0
        } else if start_ms < 6_000 {
            1
        } else {
            2 + u32::try_from((start_ms - 6_000) / seg_ms).unwrap_or(0)
        }
    } else {
        u32::try_from(start_ms / seg_ms).unwrap_or(0)
    }
}

fn build_subtitle_tracks(
    media_info: &MediaInfo,
    profile: &DeviceProfile,
    part_id: PartId,
    stream_token: &str,
) -> Vec<PlaybackSubtitleTrack> {
    media_info
        .streams
        .iter()
        .filter(|s| {
            s.kind == StreamKind::Subtitle
                && !is_image_subtitle(&s.codec)
                && !subtitle_requires_transcode(&s.codec, profile)
        })
        .map(|s| {
            let ass_target = profile
                .subtitle_rule(&s.codec)
                .and_then(|r| r.target_format.as_deref())
                .is_some_and(|f| f.eq_ignore_ascii_case("ass") || f.eq_ignore_ascii_case("ssa"));
            let format = if ass_target { "ass" } else { "vtt" };
            PlaybackSubtitleTrack {
                index: s.index,
                codec: s.codec.clone(),
                language: s.language.clone(),
                title: s.title.clone(),
                is_default: s.default,
                is_forced: s.forced,
                format: format.to_owned(),
                url: format!(
                    "/api/v1/stream/subtitles/{}/{}.{}?token={stream_token}",
                    part_id.0, s.index, format
                ),
            }
        })
        .collect()
}

fn generate_session_id() -> Result<String> {
    use std::fmt::Write as _;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| Error::Internal(Box::new(e)))?;
    let mut hex = String::with_capacity(32);
    for b in bytes {
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}
