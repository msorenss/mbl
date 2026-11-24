//! MCP AI Watch Companion ("Co-watching with Agent", e.g. Hermes) (M7.16).
//!
//! Companions join a playback session, read its live context (position, chapter and the
//! dialogue shown so far) and post short comments to the viewer's overlay. The viewer can
//! write back and choose whether companions comment freely, only answer, or stay silent.

use mbl_core::{Actor, Error, ItemId, PartId, Result, StreamKind, time::now_ms};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::broadcast;
use utoipa::ToSchema;

/// What the viewer allows connected companions to post.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CompanionMode {
    /// Companions may comment whenever they like.
    #[default]
    On,
    /// Companions may only answer the viewer's messages (`replyTo` is required).
    Replies,
    /// Companions may not post anything.
    Off,
}

/// How often companions comment on their own while the viewer allows comments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CompanionFrequency {
    /// About every ten minutes.
    Low,
    /// About every four minutes.
    #[default]
    Medium,
    /// About every ninety seconds.
    High,
}

impl CompanionFrequency {
    /// Time between unprompted comments.
    pub fn interval_ms(self) -> i64 {
        match self {
            Self::Low => 10 * 60_000,
            Self::Medium => 4 * 60_000,
            Self::High => 90_000,
        }
    }
}

/// Why a companion is asked to comment now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum CommentReason {
    /// The viewer pressed "Comment now".
    Requested,
    /// The viewer's chosen frequency says it is time for a comment.
    Scheduled,
}

/// What a waiting companion should react to.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionWake {
    /// Viewer messages newer than `afterId`, oldest first. Untrusted user text.
    pub viewer_messages: Vec<CompanionReaction>,
    /// Set when the companion should post a comment of its own.
    pub comment_due: Option<CommentReason>,
    /// Highest feed ID seen; pass it as `afterId` next time.
    pub last_id: u64,
    /// True when nothing happened before the wait ended; just wait again.
    pub timed_out: bool,
    /// Position, chapter and dialogue right now.
    pub context: LiveSessionContext,
}

/// When the first scheduled comment of a session is due, after the companion joined.
const FIRST_COMMENT_MS: i64 = 60_000;

/// The viewer did something a companion may want to react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompanionActivity {
    pub session_id: String,
    /// The viewer's message, or `None` when they pressed "Comment now".
    pub message_id: Option<u64>,
}

/// What companions say about what a player plays, for a remote control.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerComments {
    /// Playback session the comments belong to; none when the player plays nothing.
    pub session_id: Option<String>,
    pub comments: Vec<PlayerComment>,
    /// Pass as `afterId` next time.
    pub last_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerComment {
    pub id: u64,
    pub bot_name: String,
    pub commentary: String,
}

/// One entry in a session's co-watching feed: a companion comment or a viewer message.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionReaction {
    /// Increasing ID, unique per server run.
    #[serde(default)]
    pub id: u64,
    pub session_id: String,
    /// Companion name, or "Viewer" for messages from the person watching.
    pub bot_name: String,
    pub timestamp_ms: i64,
    pub commentary: String,
    pub sentiment: Option<String>, // "funny", "shocked", "lore", "trivia", "sad"
    pub created_at: i64,
    /// True when the person watching wrote this.
    #[serde(default)]
    pub from_viewer: bool,
    /// ID of the viewer message this answers.
    #[serde(default)]
    pub reply_to: Option<u64>,
}

/// A subtitle line shown before or at the current position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DialogueLine {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// Whether dialogue can be read for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DialogueStatus {
    /// Dialogue is included.
    Ready,
    /// The subtitle track is being extracted; ask again in a little while.
    Preparing,
    /// The track could not be read (image subtitles, missing file, FFmpeg failure).
    Unavailable,
    /// The media has no text subtitle track.
    None,
}

/// How often a screenshot of the scene may be taken for the AI. Off by default, because the
/// picture leaves the server for an outside model.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum CompanionFrames {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "30s")]
    Every30s,
    #[serde(rename = "1m")]
    Every1m,
    #[serde(rename = "5m")]
    Every5m,
}

impl CompanionFrames {
    /// How long one screenshot is reused; `None` when screenshots are off.
    pub fn interval_ms(self) -> Option<i64> {
        match self {
            Self::Off => None,
            Self::Every30s => Some(30_000),
            Self::Every1m => Some(60_000),
            Self::Every5m => Some(300_000),
        }
    }
}

/// What is on screen, as text: from the NPU when there is one (objects, people, a description),
/// otherwise a rough guess from colours that is marked as such.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SceneAnalysis {
    /// Position of the analysed frame (never later than the viewer's).
    pub position_ms: i64,
    /// What the NPU detected, with counts (`2 person`, `cup`). Empty without an NPU.
    pub objects: Vec<String>,
    /// A sentence from a vision-language model, when one is installed.
    pub description: Option<String>,
    /// Rough impressions from the colours (`night`, `snow`…); a guess even with an NPU.
    pub colour_guess: Vec<String>,
    /// `cpu-heuristic`, or the NPU model that looked at the frame.
    pub backend: String,
    /// True when an NPU analysed the frame; false for the colour guess.
    pub npu: bool,
}

/// A screenshot of a playback session at a position, as JPEG.
#[derive(Debug, Clone)]
pub struct CompanionFrame {
    pub jpeg: Arc<Vec<u8>>,
    pub position_ms: i64,
    pub taken_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionContext {
    pub session_id: String,
    pub item_id: ItemId,
    pub title: String,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub current_chapter: Option<String>,
    /// Subtitle line on screen at the current position.
    pub current_subtitle_cue: Option<String>,
    /// Up to the last six lines from the previous 90 seconds, oldest first. Never later lines.
    #[serde(default)]
    pub recent_dialogue: Vec<DialogueLine>,
    #[serde(default = "no_dialogue")]
    pub dialogue_status: DialogueStatus,
    /// Subtitle stream the dialogue comes from.
    #[serde(default)]
    pub dialogue_stream_index: Option<u32>,
    #[serde(default)]
    pub dialogue_language: Option<String>,
    #[serde(default)]
    pub mode: CompanionMode,
    #[serde(default)]
    pub frequency: CompanionFrequency,
    /// Whether (and how often) a screenshot of the scene may be fetched.
    #[serde(default)]
    pub frames: CompanionFrames,
    /// What is on screen, analysed on the server (the picture does not leave it).
    #[serde(default)]
    pub scene: Option<SceneAnalysis>,
    pub connected_companions: Vec<String>,
}

fn no_dialogue() -> DialogueStatus {
    DialogueStatus::None
}

/// One analysis is reused this long (and not across a jump in position).
const SCENE_REUSE_MS: i64 = 60_000;

/// What the vision-language model is asked about a frame.
const SCENE_PROMPT: &str = "Describe this frame from a film in one or two short sentences: who \
is there (without guessing names), where they are and what they are doing.";
const SCENE_TOKENS: u32 = 60;

/// The text up to its last full stop (a model cut off by its token limit stops mid-sentence).
fn whole_sentences(text: &str) -> String {
    let text = text.trim();
    match text.rfind(['.', '!', '?']) {
        Some(end) => text[..=end].to_owned(),
        None => text.to_owned(),
    }
}

/// The model file in a backend label such as `hailort-npu(HAILO10H,yolov11s.hef)` or
/// `hailort-vlm(Qwen2-VL-2B-Instruct.hef)`, for the log.
fn npu_model_name(backend: &str) -> String {
    backend.rsplit([',', '(']).next().unwrap_or(backend).trim_end_matches(')').to_owned()
}

/// Whether a vision-language model is installed for the NPU.
fn vlm_installed() -> bool {
    let fp = mbl_hwaccel::detect_fingerprint_from_root(
        std::path::Path::new("/"),
        std::env::consts::ARCH,
        None,
    );
    mbl_hwaccel::hailo::discover_vlm_hef_model(std::path::Path::new("/"), fp.npu_profile).is_some()
}

/// The NPU engine, opened once (it shares the device with other programs such as Frigate).
fn hailo() -> &'static mbl_hailo::HailoEngine {
    static ENGINE: std::sync::OnceLock<mbl_hailo::HailoEngine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(mbl_hailo::HailoEngine::default)
}

/// One JPEG frame at `position_ms`, scaled to at most [`FRAME_WIDTH`] pixels wide.
async fn grab_frame(
    ffmpeg: &std::path::Path,
    input: &std::path::Path,
    position_ms: i64,
) -> Result<Vec<u8>> {
    crate::stream::grab_jpeg(ffmpeg, input, position_ms, FRAME_WIDTH).await
}

/// Feed entries kept per playback session so the player can show them.
const RECENT_PER_SESSION: usize = 50;
/// Sessions with stored state; the oldest is dropped beyond this.
const RECENT_SESSIONS: usize = 256;
/// Parsed subtitle tracks kept in memory.
const CUE_TRACKS: usize = 8;
/// A failed extraction is tried again after this long.
const CUE_RETRY_MS: i64 = 5 * 60_000;
/// Subtitle files larger than this are not parsed.
const CUE_FILE_LIMIT: u64 = 8 * 1024 * 1024;
const DIALOGUE_WINDOW_MS: i64 = 90_000;
const DIALOGUE_LINES: usize = 6;
const VIEWER_NAME: &str = "Viewer";
const VIEWER_MESSAGE_LIMIT: usize = 500;
/// Longest a companion may wait for something to happen in one call.
pub const COMPANION_WAIT_MAX_MS: u64 = 25_000;
/// A companion counts as listening this long after its last wait.
const LISTENING_MS: i64 = 40_000;

#[derive(Debug, Clone, Default)]
struct SessionState {
    companions: Vec<String>,
    mode: CompanionMode,
    frequency: CompanionFrequency,
    frames: CompanionFrames,
    /// The viewer asked for a comment that no waiting companion has been told about yet.
    comment_requested: bool,
    /// A requested comment was handed out and may be posted even in replies-only mode.
    requested_open: bool,
    joined_at: i64,
    last_comment_at: i64,
    last_due_at: i64,
    /// Companion name and when it last waited for events.
    listening: HashMap<String, i64>,
}

#[derive(Debug, Clone)]
enum CueTrack {
    Preparing,
    Ready(Arc<Vec<DialogueLine>>),
    Failed { at: i64 },
}

#[derive(Debug, Clone)]
pub struct CompanionService {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    recent: Arc<RwLock<HashMap<String, VecDeque<CompanionReaction>>>>,
    cues: Arc<Mutex<HashMap<(PartId, u32), CueTrack>>>,
    next_id: Arc<AtomicU64>,
    playback: Option<crate::PlaybackService>,
    stream: Option<crate::StreamService>,
    db: Option<mbl_db::Db>,
    reactions_tx: broadcast::Sender<CompanionReaction>,
    activity_tx: broadcast::Sender<CompanionActivity>,
    wake: Arc<tokio::sync::Notify>,
    /// Latest screenshot per session.
    frames: Arc<Mutex<HashMap<String, CompanionFrame>>>,
    /// Latest scene analysis per session.
    scenes: Arc<Mutex<HashMap<String, SceneAnalysis>>>,
    /// One screenshot is taken at a time, to spare the CPU (a Pi also encodes video).
    grabbing: Arc<tokio::sync::Semaphore>,
}

/// Sessions whose latest screenshot is kept.
const FRAME_SESSIONS: usize = 32;
/// Width of a screenshot sent to the AI.
const FRAME_WIDTH: u32 = 768;

impl CompanionService {
    pub fn new() -> Self {
        let (reactions_tx, _) = broadcast::channel(128);
        let (activity_tx, _) = broadcast::channel(64);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            recent: Arc::new(RwLock::new(HashMap::new())),
            cues: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
            reactions_tx,
            activity_tx,
            wake: Arc::new(tokio::sync::Notify::new()),
            frames: Arc::new(Mutex::new(HashMap::new())),
            scenes: Arc::new(Mutex::new(HashMap::new())),
            grabbing: Arc::new(tokio::sync::Semaphore::new(1)),
            playback: None,
            stream: None,
            db: None,
        }
    }

    pub fn with_playback(mut self, playback: crate::PlaybackService) -> Self {
        self.playback = Some(playback);
        self
    }

    pub fn with_db(mut self, db: mbl_db::Db) -> Self {
        self.db = Some(db);
        self
    }

    /// Lets the companion read dialogue from subtitle tracks.
    pub fn with_stream(mut self, stream: crate::StreamService) -> Self {
        self.stream = Some(stream);
        self
    }

    /// Live context including the chapter and dialogue at the current position.
    pub async fn live_context(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<LiveSessionContext> {
        let mut context = self.get_live_context(actor, session_id)?;
        let Some(session) = self.playback.as_ref().and_then(|p| p.get_session(actor, session_id))
        else {
            return Ok(context);
        };
        let Some(db) = &self.db else {
            return Ok(context);
        };
        if let Ok(Some(raw)) = mbl_db::repo::media::part_chapters(db, session.part_id).await
            && let Ok(chapters) = serde_json::from_str::<Vec<mbl_probe::Chapter>>(&raw)
        {
            context.current_chapter = chapter_at(&chapters, context.position_ms);
        }

        context.scene = self.scene(actor, session_id, session.part_id, context.position_ms).await;

        let streams =
            mbl_db::repo::media::streams_for_part(db, session.part_id).await.unwrap_or_default();
        let tracks: Vec<TextTrack> = streams
            .into_iter()
            .filter(|s| s.kind == StreamKind::Subtitle)
            .filter(|s| !mbl_stream::subtitles::is_image_subtitle(&s.codec))
            .filter_map(|s| {
                Some(TextTrack {
                    index: u32::try_from(s.idx).ok()?,
                    language: s.language,
                    default: s.is_default,
                    forced: s.is_forced,
                })
            })
            .collect();
        let preferred = [session.shown_subtitle_stream_index, session.burned_subtitle_stream_index];
        let Some(track) = choose_dialogue_track(&tracks, &preferred) else {
            return Ok(context);
        };
        context.dialogue_stream_index = Some(track.index);
        context.dialogue_language = track.language.clone();
        match self.cue_track(actor, session.part_id, track.index) {
            CueTrack::Ready(lines) => {
                context.dialogue_status = DialogueStatus::Ready;
                context.current_subtitle_cue = cue_at(&lines, context.position_ms);
                context.recent_dialogue = recent_dialogue(&lines, context.position_ms);
            }
            CueTrack::Preparing => context.dialogue_status = DialogueStatus::Preparing,
            CueTrack::Failed { .. } => context.dialogue_status = DialogueStatus::Unavailable,
        }
        Ok(context)
    }

    /// Cached cues for a track; starts extraction in the background on the first request.
    fn cue_track(&self, actor: &Actor, part: PartId, index: u32) -> CueTrack {
        let Some(stream) = self.stream.clone() else {
            return CueTrack::Failed { at: now_ms() };
        };
        let Ok(mut cues) = self.cues.lock() else {
            return CueTrack::Failed { at: now_ms() };
        };
        match cues.get(&(part, index)) {
            Some(CueTrack::Failed { at }) if now_ms() - at < CUE_RETRY_MS => {
                return CueTrack::Failed { at: *at };
            }
            Some(CueTrack::Failed { .. }) | None => {}
            Some(found) => return found.clone(),
        }
        if cues.len() >= CUE_TRACKS {
            let evict =
                cues.iter().find(|(_, t)| !matches!(t, CueTrack::Preparing)).map(|(key, _)| *key);
            match evict {
                Some(key) => {
                    cues.remove(&key);
                }
                // Every slot is still being extracted; try again later.
                None => return CueTrack::Preparing,
            }
        }
        cues.insert((part, index), CueTrack::Preparing);
        drop(cues);

        let store = self.cues.clone();
        let actor = actor.clone();
        tokio::spawn(async move {
            let parsed = async {
                let path = stream
                    .resolve_subtitle(
                        &actor,
                        part,
                        index,
                        mbl_stream::subtitles::SubtitleFormat::Vtt,
                        false,
                    )
                    .await?;
                let size = tokio::fs::metadata(&path).await.map_err(Error::internal)?.len();
                if size > CUE_FILE_LIMIT {
                    return Err(Error::Validation("subtitle file is too large".into()));
                }
                let text = tokio::fs::read_to_string(&path).await.map_err(Error::internal)?;
                Ok::<_, Error>(parse_webvtt(&text))
            }
            .await;
            let entry = match parsed {
                Ok(lines) => CueTrack::Ready(Arc::new(lines)),
                Err(e) => {
                    tracing::debug!(part = part.0, index, error = %e, "companion dialogue unavailable");
                    CueTrack::Failed { at: now_ms() }
                }
            };
            if let Ok(mut cues) = store.lock() {
                cues.insert((part, index), entry);
            }
        });
        CueTrack::Preparing
    }

    /// Feed entries for a session, oldest first: those after `after_id` when given, otherwise
    /// those created after `after_ms`.
    /// What a companion said in comment `id` of this session (not the viewer's own messages).
    pub fn comment_text(&self, actor: &Actor, session_id: &str, id: u64) -> Result<String> {
        self.get_live_context(actor, session_id)?;
        let recent = self
            .recent
            .read()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        recent
            .get(session_id)
            .and_then(|list| list.iter().find(|r| r.id == id && !r.from_viewer))
            .map(|r| r.commentary.clone())
            .ok_or(Error::NotFound)
    }

    /// Companion comments for whatever a player plays now, for a remote control. Without
    /// `after_id` it returns no comments, only where the feed stands (`last_id`), so a remote
    /// shows what is said from now on.
    pub fn player_comments(
        &self,
        actor: &Actor,
        players: &crate::players::PlayerRegistry,
        player_id: &str,
        after_id: Option<u64>,
    ) -> Result<PlayerComments> {
        let (owner, item) = players.now_playing(actor, player_id)?;
        let session = item.and_then(|item| {
            self.playback.as_ref().and_then(|p| p.latest_session_for_item(owner, item))
        });
        // Comment IDs count up across the whole server, so "after this ID" also works when the
        // player moves to another title.
        let newest = self.next_id.load(Ordering::Relaxed).saturating_sub(1);
        let Some(session) = session else {
            return Ok(PlayerComments {
                session_id: None,
                comments: Vec::new(),
                last_id: after_id.unwrap_or(newest),
            });
        };
        let feed = self.reactions_since(actor, &session, i64::MAX, after_id.or(Some(0)))?;
        let last_id =
            feed.reactions.iter().map(|r| r.id).max().unwrap_or(0).max(after_id.unwrap_or(newest));
        let comments = if after_id.is_none() {
            Vec::new()
        } else {
            feed.reactions
                .into_iter()
                .filter(|r| !r.from_viewer)
                .map(|r| PlayerComment { id: r.id, bot_name: r.bot_name, commentary: r.commentary })
                .collect()
        };
        Ok(PlayerComments { session_id: Some(session), comments, last_id })
    }

    pub fn reactions_since(
        &self,
        actor: &Actor,
        session_id: &str,
        after_ms: i64,
        after_id: Option<u64>,
    ) -> Result<CompanionFeed> {
        let context = self.get_live_context(actor, session_id)?;
        let recent = self
            .recent
            .read()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        let reactions = recent
            .get(session_id)
            .map(|list| {
                list.iter()
                    .filter(|r| match after_id {
                        Some(id) => r.id > id,
                        None => r.created_at > after_ms,
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let now = now_ms();
        let listening = self
            .sessions
            .read()
            .ok()
            .and_then(|lock| {
                lock.get(session_id).map(|state| {
                    let mut names: Vec<String> = state
                        .listening
                        .iter()
                        .filter(|(_, at)| now - **at < LISTENING_MS)
                        .map(|(name, _)| name.clone())
                        .collect();
                    names.sort();
                    names
                })
            })
            .unwrap_or_default();
        Ok(CompanionFeed {
            companions: context.connected_companions,
            mode: context.mode,
            frequency: context.frequency,
            listening,
            assistant_available: false,
            reactions,
        })
    }

    pub fn join_session(&self, actor: &Actor, session_id: &str, bot_name: &str) -> Result<()> {
        self.get_live_context(actor, session_id)?;
        let bot_name = bot_name.trim();
        if bot_name.is_empty() || bot_name.len() > 100 {
            return Err(Error::Validation("companion name must contain 1–100 characters".into()));
        }
        if bot_name.eq_ignore_ascii_case(VIEWER_NAME) {
            return Err(Error::Validation(format!("`{VIEWER_NAME}` is reserved for the viewer")));
        }
        let mut first = false;
        self.update_state(session_id, |state| {
            if !state.companions.iter().any(|b| b == bot_name) {
                if state.companions.len() >= 16 {
                    return Err(Error::Conflict("too many companions".into()));
                }
                first = state.companions.is_empty();
                state.companions.push(bot_name.to_owned());
                if state.joined_at == 0 {
                    state.joined_at = now_ms();
                }
            }
            Ok(())
        })?;
        if first {
            self.warm_scene(actor, session_id);
        }
        Ok(())
    }

    /// Loading the scene description model onto the NPU takes about 45 seconds and slows other
    /// programs on the board (Frigate) while it happens. When the first companion joins, the
    /// current scene is analysed in the background, so the model loads now rather than in the
    /// middle of a conversation; it then stays loaded while the companion keeps using it.
    fn warm_scene(&self, actor: &Actor, session_id: &str) {
        if !mbl_hailo::npu_enabled() || !vlm_installed() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else { return };
        let Some(session) = self.playback.as_ref().and_then(|p| p.get_session(actor, session_id))
        else {
            return;
        };
        let (service, actor, session_id) = (self.clone(), actor.clone(), session_id.to_owned());
        handle.spawn(async move {
            let _ = service
                .scene(&actor, &session_id, session.part_id, session.position_ms as i64)
                .await;
        });
    }

    /// Choose what companions may post and how often they comment on their own. Only the
    /// person watching can change it.
    pub fn set_mode(
        &self,
        actor: &Actor,
        session_id: &str,
        mode: CompanionMode,
        frequency: Option<CompanionFrequency>,
        frames: Option<CompanionFrames>,
    ) -> Result<()> {
        self.require_viewer(actor, session_id)?;
        self.update_state(session_id, |state| {
            state.mode = mode;
            if let Some(frequency) = frequency {
                state.frequency = frequency;
            }
            if let Some(frames) = frames {
                state.frames = frames;
            }
            if mode == CompanionMode::Off {
                state.comment_requested = false;
                state.requested_open = false;
            }
            Ok(())
        })?;
        self.wake.notify_waiters();
        // The player sends its choice when playback starts: with comments allowed, the
        // built-in companion joins now instead of waiting for the viewer to write.
        if mode != CompanionMode::Off {
            let _ = self
                .activity_tx
                .send(CompanionActivity { session_id: session_id.to_owned(), message_id: None });
        }
        Ok(())
    }

    /// The viewer asks the companions for a comment right now.
    pub fn request_comment(&self, actor: &Actor, session_id: &str) -> Result<()> {
        self.require_viewer(actor, session_id)?;
        self.update_state(session_id, |state| {
            if state.mode == CompanionMode::Off {
                return Err(Error::Conflict("companion comments are turned off".into()));
            }
            state.comment_requested = true;
            Ok(())
        })?;
        self.wake.notify_waiters();
        let _ = self
            .activity_tx
            .send(CompanionActivity { session_id: session_id.to_owned(), message_id: None });
        Ok(())
    }

    /// Waits until the viewer writes something newer than `after_id`, asks for a comment, or
    /// a scheduled comment is due, or until `timeout_ms` has passed (at most
    /// [`COMPANION_WAIT_MAX_MS`]). The companion must have joined the session.
    pub async fn wait_for_events(
        &self,
        actor: &Actor,
        session_id: &str,
        bot_name: &str,
        after_id: Option<u64>,
        timeout_ms: u64,
    ) -> Result<CompanionWake> {
        let deadline = tokio::time::Instant::now()
            + std::time::Duration::from_millis(timeout_ms.min(COMPANION_WAIT_MAX_MS));
        let mut reactions = self.reactions_tx.subscribe();
        loop {
            let notified = self.wake.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let context = self.get_live_context(actor, session_id)?;
            if !context.connected_companions.iter().any(|name| name == bot_name) {
                return Err(not_joined(bot_name));
            }
            let playing = self
                .playback
                .as_ref()
                .and_then(|p| p.get_session(actor, session_id))
                .is_some_and(|s| s.state == crate::playback::PlaybackState::Playing);
            let after = after_id.unwrap_or(0);
            let (viewer_messages, last_id) = self
                .recent
                .read()
                .ok()
                .and_then(|recent| {
                    recent.get(session_id).map(|list| {
                        let messages: Vec<CompanionReaction> = list
                            .iter()
                            .filter(|r| r.from_viewer && r.id > after)
                            .cloned()
                            .collect();
                        let last = list.back().map_or(after, |r| r.id.max(after));
                        (messages, last)
                    })
                })
                .unwrap_or((Vec::new(), after));
            let now = now_ms();
            let mut comment_due = None;
            self.update_state(session_id, |state| {
                state.listening.insert(bot_name.to_owned(), now);
                if state.mode == CompanionMode::Off {
                    return Ok(());
                }
                if state.comment_requested {
                    state.comment_requested = false;
                    state.requested_open = true;
                    comment_due = Some(CommentReason::Requested);
                } else if state.mode == CompanionMode::On && playing {
                    let since = state.last_comment_at.max(state.last_due_at).max(state.joined_at);
                    // The first comment comes after a minute, not a whole interval.
                    let first = state.last_comment_at == 0 && state.last_due_at == 0;
                    let wait = if first {
                        FIRST_COMMENT_MS.min(state.frequency.interval_ms())
                    } else {
                        state.frequency.interval_ms()
                    };
                    if now - since >= wait {
                        state.last_due_at = now;
                        comment_due = Some(CommentReason::Scheduled);
                    }
                }
                Ok(())
            })?;

            let timed_out = tokio::time::Instant::now() >= deadline;
            if !viewer_messages.is_empty() || comment_due.is_some() || timed_out {
                return Ok(CompanionWake {
                    viewer_messages,
                    comment_due,
                    last_id,
                    timed_out: timed_out && comment_due.is_none(),
                    context: self.live_context(actor, session_id).await?,
                });
            }
            // Wake on new feed entries, a request from the viewer, or once a second so that
            // scheduled comments and the end of the session are noticed.
            tokio::select! {
                _ = reactions.recv() => {}
                _ = &mut notified => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                _ = tokio::time::sleep_until(deadline) => {}
            }
        }
    }

    /// A message from the person watching to the connected companions.
    pub fn post_viewer_message(
        &self,
        actor: &Actor,
        session_id: &str,
        text: &str,
    ) -> Result<CompanionReaction> {
        let session = self.require_viewer(actor, session_id)?;
        let text = text.trim();
        if text.is_empty() || text.chars().count() > VIEWER_MESSAGE_LIMIT {
            return Err(Error::Validation(format!(
                "message must contain 1–{VIEWER_MESSAGE_LIMIT} characters"
            )));
        }
        let message = self.publish(CompanionReaction {
            id: 0,
            session_id: session_id.to_owned(),
            bot_name: VIEWER_NAME.to_owned(),
            timestamp_ms: session.position_ms as i64,
            commentary: text.to_owned(),
            sentiment: None,
            created_at: now_ms(),
            from_viewer: true,
            reply_to: None,
        });
        let _ = self.activity_tx.send(CompanionActivity {
            session_id: session_id.to_owned(),
            message_id: Some(message.id),
        });
        Ok(message)
    }

    pub fn get_live_context(&self, actor: &Actor, session_id: &str) -> Result<LiveSessionContext> {
        let session = self
            .playback
            .as_ref()
            .and_then(|p| p.get_session(actor, session_id))
            .ok_or(Error::NotFound)?;
        let lock = self
            .sessions
            .read()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        let state = lock.get(session_id).cloned().unwrap_or_default();
        Ok(LiveSessionContext {
            session_id: session_id.to_owned(),
            item_id: session.item_id,
            title: session.title,
            position_ms: session.position_ms as i64,
            duration_ms: session.duration_ms as i64,
            current_chapter: None,
            current_subtitle_cue: None,
            recent_dialogue: Vec::new(),
            dialogue_status: DialogueStatus::None,
            dialogue_stream_index: None,
            dialogue_language: None,
            mode: state.mode,
            frequency: state.frequency,
            frames: state.frames,
            scene: None,
            connected_companions: state.companions,
        })
    }

    /// What is on screen at the viewer's position, analysed on the server. Reused for a minute
    /// within the same stretch of the film; `None` when no frame could be taken right now.
    async fn scene(
        &self,
        actor: &Actor,
        session_id: &str,
        part: PartId,
        position: i64,
    ) -> Option<SceneAnalysis> {
        let reuse = |s: &SceneAnalysis| {
            s.position_ms <= position && position - s.position_ms < SCENE_REUSE_MS
        };
        let cached = self.scenes.lock().ok().and_then(|m| m.get(session_id).cloned());
        if let Some(scene) = cached.as_ref().filter(|s| reuse(s)) {
            return Some(scene.clone());
        }
        // Shares the screenshot slot: one FFmpeg seek at a time.
        let _permit = self.grabbing.clone().try_acquire_owned().ok()?;
        let stream = self.stream.as_ref()?;
        let resolved = stream.resolve_part(actor, part).await.ok()?;
        let input = stream.media_input(&resolved.path).await.ok()?;
        let jpeg = grab_frame(stream.ffmpeg_bin(), &input, position).await.ok()?;
        let vlm_jpeg = jpeg.clone();
        let analysis = tokio::task::spawn_blocking(move || {
            let image = image::load_from_memory(&jpeg).ok()?;
            // No title or summary as a hint: the description must come from the picture alone.
            Some(hailo().analyze_image(&image, None))
        })
        .await
        .ok()
        .flatten()?;
        let npu = analysis.backend != "cpu-heuristic";
        if npu {
            tracing::info!(
                target: "mbl::npu",
                npu_model = %npu_model_name(&analysis.backend),
                position_ms = position,
                npu_text = %if analysis.objects.is_empty() {
                    "nothing".to_owned()
                } else {
                    analysis.objects.join(", ")
                },
                "NPU looked at the picture"
            );
        }
        // Tags the detections or the description already account for are not colour guesses.
        let colour_guess = analysis
            .tags
            .into_iter()
            .filter(|t| {
                t != "person"
                    && t != "face"
                    && !analysis.objects.iter().any(|o| o.ends_with(t.as_str()))
            })
            .collect();
        let scene = SceneAnalysis {
            position_ms: position,
            objects: analysis.objects,
            description: analysis.description,
            colour_guess,
            backend: analysis.backend,
            npu,
        };
        // Keep the last description while the stretch of the film is the same.
        let mut scene = scene;
        if scene.description.is_none()
            && let Some(previous) = cached.filter(|c| c.position_ms <= position)
            && position - previous.position_ms < 2 * SCENE_REUSE_MS
        {
            scene.description = previous.description;
        }
        if let Ok(mut map) = self.scenes.lock() {
            if map.len() >= FRAME_SESSIONS && !map.contains_key(session_id) {
                map.clear();
            }
            map.insert(session_id.to_owned(), scene.clone());
        }
        self.describe_in_background(session_id, position, vlm_jpeg);
        Some(scene)
    }

    /// A vision-language model takes seconds per frame (and the first time long enough to load),
    /// so its sentence is made in the background and added to the stored analysis when ready.
    fn describe_in_background(&self, session_id: &str, position: i64, jpeg: Vec<u8>) {
        if !vlm_installed() {
            return;
        }
        static BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if BUSY.swap(true, std::sync::atomic::Ordering::AcqRel) {
            return;
        }
        let scenes = self.scenes.clone();
        let session_id = session_id.to_owned();
        tokio::task::spawn_blocking(move || {
            let started = std::time::Instant::now();
            let answer = image::load_from_memory(&jpeg)
                .ok()
                .and_then(|image| hailo().describe_image_vlm(&image, SCENE_PROMPT, SCENE_TOKENS));
            let took_ms = started.elapsed().as_millis() as u64;
            let text =
                answer.as_ref().map(|(text, _)| whole_sentences(text)).filter(|t| !t.is_empty());
            match (&answer, &text) {
                (Some((_, backend)), Some(text)) => tracing::info!(
                    target: "mbl::npu",
                    npu_model = %npu_model_name(backend),
                    position_ms = position,
                    took_ms,
                    npu_text = %text,
                    "NPU described the scene"
                ),
                _ => {
                    tracing::info!(target: "mbl::npu", position_ms = position, took_ms, "NPU gave no scene description")
                }
            }
            if let (Some(text), Ok(mut map)) = (text, scenes.lock())
                && let Some(scene) = map.get_mut(&session_id)
                && scene.position_ms == position
            {
                scene.description = Some(text);
            }
            BUSY.store(false, std::sync::atomic::Ordering::Release);
        });
    }

    /// A screenshot of the scene at the viewer's position, for an AI. Only when the viewer has
    /// turned screenshots on; one picture is reused for the chosen interval, never one from
    /// later than the position. Fails with `Conflict` when screenshots are off and
    /// `Unavailable` when another screenshot is being taken and there is none to reuse.
    pub async fn frame(&self, actor: &Actor, session_id: &str) -> Result<CompanionFrame> {
        let context = self.get_live_context(actor, session_id)?;
        let Some(interval) = context.frames.interval_ms() else {
            return Err(Error::Conflict(
                "screenshots are off for this session; the viewer can turn them on in the \
                 player chat"
                    .into(),
            ));
        };
        let position = context.position_ms;
        let reusable = |f: &CompanionFrame| {
            f.position_ms <= position
                && position - f.position_ms < interval
                && now_ms() - f.taken_at < interval
        };
        let cached = self.frames.lock().ok().and_then(|f| f.get(session_id).cloned());
        if let Some(frame) = cached.as_ref().filter(|f| reusable(f)) {
            return Ok(frame.clone());
        }
        let Ok(_permit) = self.grabbing.clone().try_acquire_owned() else {
            return cached.filter(|f| f.position_ms <= position).ok_or_else(|| {
                Error::Unavailable("a screenshot is being taken; ask again shortly".into())
            });
        };
        let session = self
            .playback
            .as_ref()
            .and_then(|p| p.get_session(actor, session_id))
            .ok_or(Error::NotFound)?;
        let stream = self.stream.as_ref().ok_or_else(|| {
            Error::Unavailable("screenshots are not available on this server".into())
        })?;
        let part = stream.resolve_part(actor, session.part_id).await?;
        let input = stream.media_input(&part.path).await?;
        let jpeg = grab_frame(stream.ffmpeg_bin(), &input, position).await?;
        let frame =
            CompanionFrame { jpeg: Arc::new(jpeg), position_ms: position, taken_at: now_ms() };
        if let Ok(mut frames) = self.frames.lock() {
            if frames.len() >= FRAME_SESSIONS && !frames.contains_key(session_id) {
                let oldest =
                    frames.iter().min_by_key(|(_, f)| f.taken_at).map(|(id, _)| id.clone());
                if let Some(id) = oldest {
                    frames.remove(&id);
                }
            }
            frames.insert(session_id.to_owned(), frame.clone());
        }
        Ok(frame)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_reaction(
        &self,
        actor: &Actor,
        session_id: &str,
        bot_name: &str,
        timestamp_ms: i64,
        commentary: &str,
        sentiment: Option<&str>,
        reply_to: Option<u64>,
    ) -> Result<CompanionReaction> {
        let context = self.get_live_context(actor, session_id)?;
        if !context.connected_companions.iter().any(|name| name == bot_name) {
            return Err(not_joined(bot_name));
        }
        let requested_open = self
            .sessions
            .read()
            .ok()
            .and_then(|lock| lock.get(session_id).map(|state| state.requested_open))
            .unwrap_or(false);
        match context.mode {
            CompanionMode::Off => {
                return Err(Error::Conflict(
                    "the viewer turned companion comments off; do not post".into(),
                ));
            }
            CompanionMode::Replies if reply_to.is_none() && !requested_open => {
                return Err(Error::Conflict(
                    "the viewer only wants answers; set reply_to to a viewer message id".into(),
                ));
            }
            _ => {}
        }
        if let Some(id) = reply_to
            && !self.has_viewer_message(session_id, id)
        {
            return Err(Error::Validation(format!("reply_to {id} is not a viewer message")));
        }
        if timestamp_ms < 0
            || timestamp_ms > context.position_ms.saturating_add(10_000)
            || commentary.trim().is_empty()
            || commentary.len() > 4000
        {
            return Err(Error::Validation("invalid reaction position or commentary".into()));
        }
        let posted_at = now_ms();
        self.update_state(session_id, |state| {
            state.last_comment_at = posted_at;
            if reply_to.is_none() {
                state.requested_open = false;
            }
            Ok(())
        })?;
        Ok(self.publish(CompanionReaction {
            id: 0,
            session_id: session_id.to_owned(),
            bot_name: bot_name.to_owned(),
            timestamp_ms,
            commentary: commentary.to_owned(),
            sentiment: sentiment.map(|s| s.to_owned()),
            created_at: now_ms(),
            from_viewer: false,
            reply_to,
        }))
    }

    pub fn subscribe_reactions(&self) -> broadcast::Receiver<CompanionReaction> {
        self.reactions_tx.subscribe()
    }

    /// Viewer messages and "Comment now" presses, for the built-in companion.
    pub fn subscribe_activity(&self) -> broadcast::Receiver<CompanionActivity> {
        self.activity_tx.subscribe()
    }

    fn require_viewer(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<crate::playback::ActivePlaybackSession> {
        let session = self
            .playback
            .as_ref()
            .and_then(|p| p.get_session(actor, session_id))
            .ok_or(Error::NotFound)?;
        // Admins can read any session, but only the person watching speaks for it.
        if session.user_id != actor.user_id {
            return Err(Error::Forbidden);
        }
        Ok(session)
    }

    fn has_viewer_message(&self, session_id: &str, id: u64) -> bool {
        self.recent.read().ok().and_then(|recent| {
            recent.get(session_id).map(|list| list.iter().any(|r| r.from_viewer && r.id == id))
        }) == Some(true)
    }

    fn update_state(
        &self,
        session_id: &str,
        change: impl FnOnce(&mut SessionState) -> Result<()>,
    ) -> Result<()> {
        let mut lock = self
            .sessions
            .write()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        if !lock.contains_key(session_id) && lock.len() >= RECENT_SESSIONS {
            let ended: Vec<String> =
                lock.keys().filter(|id| !self.session_exists(id)).cloned().collect();
            for id in ended {
                lock.remove(&id);
            }
            if lock.len() >= RECENT_SESSIONS {
                return Err(Error::Unavailable("too many co-watching sessions".into()));
            }
        }
        change(lock.entry(session_id.to_owned()).or_default())
    }

    fn session_exists(&self, session_id: &str) -> bool {
        let system = Actor::admin(mbl_core::UserId(0));
        self.playback.as_ref().is_some_and(|p| p.get_session(&system, session_id).is_some())
    }

    fn publish(&self, mut reaction: CompanionReaction) -> CompanionReaction {
        reaction.id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let session_id = reaction.session_id.clone();
        if let Ok(mut recent) = self.recent.write() {
            if !recent.contains_key(&session_id) && recent.len() >= RECENT_SESSIONS {
                let oldest = recent
                    .iter()
                    .min_by_key(|(_, list)| list.back().map_or(0, |r| r.created_at))
                    .map(|(id, _)| id.clone());
                if let Some(oldest) = oldest {
                    recent.remove(&oldest);
                }
            }
            let list = recent.entry(session_id).or_default();
            if list.len() >= RECENT_PER_SESSION {
                list.pop_front();
            }
            list.push_back(reaction.clone());
        }
        let _ = self.reactions_tx.send(reaction.clone());
        reaction
    }
}

/// Connected companions, the viewer's chosen mode and the latest feed entries for one session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionFeed {
    pub companions: Vec<String>,
    #[serde(default)]
    pub mode: CompanionMode,
    #[serde(default)]
    pub frequency: CompanionFrequency,
    /// Companions waiting for events right now; they answer within seconds.
    #[serde(default)]
    pub listening: Vec<String>,
    /// The built-in assistant is turned on: writing or pressing "Comment now" brings it in.
    #[serde(default)]
    pub assistant_available: bool,
    pub reactions: Vec<CompanionReaction>,
}

fn not_joined(bot_name: &str) -> Error {
    Error::Conflict(format!(
        "'{bot_name}' has not joined this session; join it first (join_watch_session)"
    ))
}

#[derive(Debug, Clone)]
struct TextTrack {
    index: u32,
    language: Option<String>,
    default: bool,
    forced: bool,
}

/// The track the viewer is shown, else the default full track, else the first full track.
fn choose_dialogue_track<'a>(
    tracks: &'a [TextTrack],
    preferred: &[Option<u32>],
) -> Option<&'a TextTrack> {
    preferred
        .iter()
        .flatten()
        .find_map(|idx| tracks.iter().find(|t| t.index == *idx))
        .or_else(|| tracks.iter().find(|t| t.default && !t.forced))
        .or_else(|| tracks.iter().find(|t| !t.forced))
        .or_else(|| tracks.first())
}

fn chapter_at(chapters: &[mbl_probe::Chapter], position_ms: i64) -> Option<String> {
    let position = u64::try_from(position_ms).ok()?;
    chapters
        .iter()
        .find(|c| c.start_ms <= position && position < c.end_ms)
        .and_then(|c| c.title.clone())
        .filter(|t| !t.trim().is_empty())
}

fn cue_at(lines: &[DialogueLine], position_ms: i64) -> Option<String> {
    lines
        .iter()
        .rev()
        .find(|l| l.start_ms <= position_ms && position_ms < l.end_ms)
        .map(|l| l.text.clone())
}

/// Lines that started at or before `position_ms` and ended within the dialogue window.
fn recent_dialogue(lines: &[DialogueLine], position_ms: i64) -> Vec<DialogueLine> {
    let shown = lines.partition_point(|l| l.start_ms <= position_ms);
    let mut out: Vec<DialogueLine> = lines[..shown]
        .iter()
        .rev()
        .filter(|l| l.end_ms >= position_ms - DIALOGUE_WINDOW_MS)
        .take(DIALOGUE_LINES)
        .cloned()
        .collect();
    out.reverse();
    out
}

/// Parse WebVTT into plain-text lines sorted by start time.
fn parse_webvtt(text: &str) -> Vec<DialogueLine> {
    let mut out = Vec::new();
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r')).peekable();
    while let Some(line) = lines.next() {
        let Some((start, end)) = line.split_once("-->") else {
            continue;
        };
        let (Some(start_ms), Some(end_ms)) =
            (parse_vtt_time(start), parse_vtt_time(end.split_whitespace().next().unwrap_or("")))
        else {
            continue;
        };
        let mut parts = Vec::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() {
                break;
            }
            parts.push(clean_cue_text(next));
            lines.next();
        }
        let text = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        if !text.is_empty() && end_ms > start_ms {
            out.push(DialogueLine { start_ms, end_ms, text });
        }
    }
    out.sort_by_key(|l| l.start_ms);
    out
}

/// `HH:MM:SS.mmm` or `MM:SS.mmm`.
fn parse_vtt_time(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    let (clock, millis) = raw.split_once('.')?;
    let millis: i64 = millis.get(..3)?.parse().ok()?;
    let fields: Vec<i64> = clock.split(':').map(|f| f.parse().ok()).collect::<Option<_>>()?;
    let seconds = match fields.as_slice() {
        [h, m, s] => h * 3600 + m * 60 + s,
        [m, s] => m * 60 + s,
        _ => return None,
    };
    Some(seconds * 1000 + millis)
}

fn clean_cue_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_tag = false;
    for c in raw.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&lrm;", "")
        .replace("&rlm;", "")
        .trim()
        .to_owned()
}

impl Default for CompanionService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_names_the_npu_model() {
        assert_eq!(npu_model_name("hailort-npu(HAILO10H,yolov11s.hef)"), "yolov11s.hef");
        assert_eq!(
            npu_model_name("hailort-vlm(Qwen2-VL-2B-Instruct.hef)"),
            "Qwen2-VL-2B-Instruct.hef"
        );
        assert_eq!(npu_model_name("cpu-heuristic"), "cpu-heuristic");
    }

    #[test]
    fn descriptions_end_on_a_whole_sentence() {
        assert_eq!(whole_sentences("A man stands. The mood is som"), "A man stands.");
        assert_eq!(whole_sentences(" Two people talk! "), "Two people talk!");
        assert_eq!(whole_sentences("no full stop"), "no full stop");
    }

    #[test]
    fn chapter_is_found_by_position() {
        let chapters = vec![
            mbl_probe::Chapter { start_ms: 0, end_ms: 1000, title: Some("Intro".into()) },
            mbl_probe::Chapter { start_ms: 1000, end_ms: 5000, title: Some("Heist".into()) },
            mbl_probe::Chapter { start_ms: 5000, end_ms: 6000, title: None },
        ];
        assert_eq!(chapter_at(&chapters, 999).as_deref(), Some("Intro"));
        assert_eq!(chapter_at(&chapters, 1000).as_deref(), Some("Heist"));
        assert_eq!(chapter_at(&chapters, 5500), None);
        assert_eq!(chapter_at(&chapters, 7000), None);
        assert_eq!(chapter_at(&chapters, -1), None);
    }

    const VTT: &str = "WEBVTT\r\n\r\nNOTE made by ffmpeg\r\n\r\n1\r\n00:00:01.000 --> 00:00:02.500\r\n<i>Are you</i> happy?\r\n\r\n00:00:03.000 --> 00:00:04.000 align:start\r\n- Jason &amp; Daniela.\r\n- Yes.\r\n\r\n01:00.000 --> 01:01.000\r\n\r\n02:00.000 --> 02:05.000\r\nLater line\r\n";

    #[test]
    fn webvtt_is_parsed_into_plain_lines() {
        let lines = parse_webvtt(VTT);
        assert_eq!(
            lines,
            vec![
                DialogueLine { start_ms: 1000, end_ms: 2500, text: "Are you happy?".into() },
                DialogueLine {
                    start_ms: 3000,
                    end_ms: 4000,
                    text: "- Jason & Daniela. - Yes.".into()
                },
                DialogueLine { start_ms: 120_000, end_ms: 125_000, text: "Later line".into() },
            ]
        );
        assert_eq!(parse_vtt_time("1:02:03.004"), Some(3_723_004));
        assert_eq!(parse_vtt_time("bad"), None);
    }

    #[test]
    fn dialogue_never_includes_lines_after_the_position() {
        let lines = parse_webvtt(VTT);
        assert_eq!(cue_at(&lines, 1500).as_deref(), Some("Are you happy?"));
        assert_eq!(cue_at(&lines, 2600), None);
        let recent = recent_dialogue(&lines, 3500);
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().all(|l| l.start_ms <= 3500));
        // Lines that ended more than 90 seconds ago drop out.
        assert!(recent_dialogue(&lines, 100_000).is_empty());
        assert_eq!(recent_dialogue(&lines, 121_000).len(), 1);
        // Only the last six lines are kept.
        let many: Vec<DialogueLine> = (0..10)
            .map(|i| DialogueLine {
                start_ms: i * 1000,
                end_ms: i * 1000 + 500,
                text: i.to_string(),
            })
            .collect();
        let tail = recent_dialogue(&many, 9_200);
        assert_eq!(tail.first().map(|l| l.text.as_str()), Some("4"));
        assert_eq!(tail.len(), 6);
    }

    #[test]
    fn dialogue_track_prefers_what_the_viewer_sees() {
        let track = |index, default, forced| TextTrack { index, language: None, default, forced };
        let tracks = vec![track(2, false, true), track(3, false, false), track(4, true, false)];
        assert_eq!(choose_dialogue_track(&tracks, &[Some(3), None]).map(|t| t.index), Some(3));
        assert_eq!(choose_dialogue_track(&tracks, &[None, Some(2)]).map(|t| t.index), Some(2));
        // An unknown or image track falls back to the default full track.
        assert_eq!(choose_dialogue_track(&tracks, &[Some(9), None]).map(|t| t.index), Some(4));
        let no_default = vec![track(2, false, true), track(3, false, false)];
        assert_eq!(choose_dialogue_track(&no_default, &[None]).map(|t| t.index), Some(3));
        assert!(choose_dialogue_track(&[], &[Some(1)]).is_none());
    }
}
