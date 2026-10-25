//! HLS transcode/remux session manager: upfront playlist persistence, segment wait vs. seek
//! restart (`-ss`, `-start_number`), SIGSTOP/SIGCONT ahead-throttling, 60 s inactivity cleanup,
//! disk quota enforcement, and concurrency caps (M2.5).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use camino::Utf8PathBuf;
use mbl_core::{Error, PartId, Result, UserId};
use mbl_platform::PlatformProfile;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use crate::args::build_args;
use crate::pipeline::{Backend, FfmpegCaps, Pipeline, VideoOp};

/// Default number of segments after which an ahead FFmpeg process is paused.
pub const DEFAULT_AHEAD_SEGMENTS: u32 = 10;
/// Default inactivity timeout before a session is killed and its temp directory removed.
pub const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(60);
/// Default maximum wait for a single segment or `init.mp4` to be produced.
pub const DEFAULT_SEGMENT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Maximum concurrent remux (`-c:v copy`) sessions.
pub const DEFAULT_MAX_REMUX_SESSIONS: usize = 8;

/// Configuration for [`SessionManager`].
#[derive(Debug, Clone)]
pub struct SessionManagerConfig {
    pub ffmpeg_bin: PathBuf,
    pub transcode_dir: Utf8PathBuf,
    pub max_video_sessions: usize,
    pub max_remux_sessions: usize,
    pub ahead_segments: u32,
    pub inactivity_timeout: Duration,
    pub segment_wait_timeout: Duration,
    pub temp_quota_bytes: u64,
    pub caps: FfmpegCaps,
}

impl SessionManagerConfig {
    /// Default concurrent video transcode limit when `transcode.max_sessions == 0`.
    pub const fn default_max_video_sessions(profile: PlatformProfile) -> usize {
        match profile {
            PlatformProfile::Pi4 | PlatformProfile::Pi5 | PlatformProfile::GenericArm => 1,
            PlatformProfile::Generic => 2,
            PlatformProfile::X86Intel | PlatformProfile::X86Nvidia | PlatformProfile::X86Amd => 4,
        }
    }

    pub fn new(ffmpeg_bin: PathBuf, transcode_dir: Utf8PathBuf, profile: PlatformProfile) -> Self {
        Self {
            ffmpeg_bin,
            transcode_dir,
            max_video_sessions: Self::default_max_video_sessions(profile),
            max_remux_sessions: DEFAULT_MAX_REMUX_SESSIONS,
            ahead_segments: DEFAULT_AHEAD_SEGMENTS,
            inactivity_timeout: DEFAULT_INACTIVITY_TIMEOUT,
            segment_wait_timeout: DEFAULT_SEGMENT_WAIT_TIMEOUT,
            temp_quota_bytes: 4096 * 1024 * 1024,
            caps: FfmpegCaps::default(),
        }
    }
}

/// Parameters to create a new HLS session.
#[derive(Debug, Clone)]
pub struct CreateSessionReq {
    pub user_id: UserId,
    pub part_id: PartId,
    pub duration_ms: u64,
    pub pipeline: Pipeline,
    pub master_playlist: String,
    /// `(rendition_name, index_m3u8_content)` written up front before FFmpeg starts.
    pub media_playlists: Vec<(String, String)>,
    /// When `true`, segments 0 and 1 are 3 s each when `segment_secs >= 6`; when `false`, all
    /// segments use uniform `segment_secs` duration.
    pub fast_start_segments: bool,
}

struct SessionState {
    child: Option<Child>,
    last_exit_failed: bool,
    last_spawn_at: Instant,
    active_backend: Backend,
    fallback_backends: Vec<Backend>,
    paused: bool,
    current_start_segment: u32,
    ffmpeg_head_segment: u32,
    last_requested_segment: u32,
    last_access: Instant,
    restart_count: u32,
}

/// An active HLS transcode or remux session.
pub struct HlsSession {
    pub id: String,
    pub user_id: UserId,
    pub part_id: PartId,
    pub dir: Utf8PathBuf,
    pub duration_ms: u64,
    pub is_video_transcode: bool,
    ffmpeg_bin: PathBuf,
    base_pipeline: Pipeline,
    caps: FfmpegCaps,
    ahead_segments: u32,
    segment_wait_timeout: Duration,
    fast_start_segments: bool,
    bad_backends: Arc<RwLock<HashMap<(Backend, String), Instant>>>,
    bad_backend_ttl: Duration,
    state: Mutex<SessionState>,
}

impl std::fmt::Debug for HlsSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HlsSession")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("part_id", &self.part_id)
            .field("dir", &self.dir)
            .field("is_video_transcode", &self.is_video_transcode)
            .finish_non_exhaustive()
    }
}

impl HlsSession {
    /// Touch the session's activity timestamp and return the upfront `master.m3u8` content.
    pub async fn request_master_playlist(&self) -> Result<String> {
        {
            let mut st = self.state.lock().await;
            st.last_access = Instant::now();
        }
        let path = self.dir.join("master.m3u8");
        tokio::fs::read_to_string(path.as_std_path()).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::Internal(Box::new(e))
            }
        })
    }

    /// Touch the session's activity timestamp and return `<rendition>/index.m3u8`.
    pub async fn request_media_playlist(&self, rendition: &str) -> Result<String> {
        validate_rendition_name(rendition)?;
        {
            let mut st = self.state.lock().await;
            st.last_access = Instant::now();
        }
        let path = self.dir.join(rendition).join("index.m3u8");
        tokio::fs::read_to_string(path.as_std_path()).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::NotFound
            } else {
                Error::Internal(Box::new(e))
            }
        })
    }

    /// Resolve `<rendition>/init.mp4`, waiting for FFmpeg to write it if needed.
    pub async fn request_init(&self, rendition: &str) -> Result<PathBuf> {
        validate_rendition_name(rendition)?;
        let rendition_dir = self.dir.join(rendition);
        if !rendition_dir.exists() {
            return Err(Error::NotFound);
        }
        let init_path = rendition_dir.join("init.mp4").into_std_path_buf();

        {
            let mut st = self.state.lock().await;
            st.last_access = Instant::now();
            let running = poll_child_state(&mut st);
            if is_init_complete(&rendition_dir, running) {
                return Ok(init_path);
            }
            if running {
                if st.paused {
                    resume_child(&st.child);
                    st.paused = false;
                }
            } else {
                let start_seg = st.last_requested_segment;
                if st.last_exit_failed
                    && st.last_spawn_at.elapsed() <= Duration::from_secs(5)
                    && !st.fallback_backends.is_empty()
                {
                    self.advance_fallback_locked(&mut st);
                }
                self.spawn_at_segment_locked(&mut st, start_seg).await?;
            }
        }

        let deadline = Instant::now() + self.segment_wait_timeout;
        loop {
            {
                let mut st = self.state.lock().await;
                let running = poll_child_state(&mut st);
                if is_init_complete(&rendition_dir, running) {
                    return Ok(init_path);
                }
                if !running {
                    if st.last_exit_failed
                        && st.last_spawn_at.elapsed() <= Duration::from_secs(5)
                        && !st.fallback_backends.is_empty()
                    {
                        let start_seg = st.last_requested_segment;
                        self.advance_fallback_locked(&mut st);
                        self.spawn_at_segment_locked(&mut st, start_seg).await?;
                    } else {
                        return Err(Error::Unavailable(
                            "FFmpeg exited before producing init.mp4".into(),
                        ));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::Unavailable("timed out waiting for HLS init.mp4".into()));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Resolve `<rendition>/<segment_num>.m4s` according to M2.5 and M4.9 rules:
    /// 1. If the segment exists and is complete, serve it immediately.
    /// 2. If it is within 3 segments of the FFmpeg head, wait (up to timeout).
    /// 3. Otherwise restart FFmpeg at `segment_num` (`-ss`, `-start_number n`).
    /// 4. If FFmpeg exits non-zero within 5 s, mark the backend bad and retry with the next fallback backend.
    pub async fn request_segment(&self, rendition: &str, segment_num: u32) -> Result<PathBuf> {
        validate_rendition_name(rendition)?;
        let rendition_dir = self.dir.join(rendition);
        if !rendition_dir.exists() {
            return Err(Error::NotFound);
        }
        let seg_path = rendition_dir.join(format!("{segment_num}.m4s")).into_std_path_buf();

        {
            let mut st = self.state.lock().await;
            st.last_access = Instant::now();
            st.last_requested_segment = segment_num;

            let running = poll_child_state(&mut st);
            refresh_head_segment(&rendition_dir, &mut st, running);

            // Resume FFmpeg if it was paused and the client has caught up within `ahead_segments`.
            if st.paused
                && st.ffmpeg_head_segment.saturating_sub(segment_num) <= self.ahead_segments
            {
                resume_child(&st.child);
                st.paused = false;
            }

            if is_segment_complete(&rendition_dir, segment_num, running) {
                return Ok(seg_path);
            }

            let within_head_window = running
                && segment_num >= st.current_start_segment
                && segment_num <= st.ffmpeg_head_segment.saturating_add(3);

            if within_head_window {
                if st.paused {
                    resume_child(&st.child);
                    st.paused = false;
                }
            } else {
                if !running
                    && st.last_exit_failed
                    && st.last_spawn_at.elapsed() <= Duration::from_secs(5)
                    && !st.fallback_backends.is_empty()
                {
                    self.advance_fallback_locked(&mut st);
                } else {
                    st.restart_count = st.restart_count.saturating_add(1);
                }
                self.spawn_at_segment_locked(&mut st, segment_num).await?;
            }
        }

        let deadline = Instant::now() + self.segment_wait_timeout;
        loop {
            {
                let mut st = self.state.lock().await;
                let running = poll_child_state(&mut st);
                refresh_head_segment(&rendition_dir, &mut st, running);
                if is_segment_complete(&rendition_dir, segment_num, running) {
                    return Ok(seg_path);
                }
                if !running {
                    if st.last_exit_failed
                        && st.last_spawn_at.elapsed() <= Duration::from_secs(5)
                        && !st.fallback_backends.is_empty()
                    {
                        self.advance_fallback_locked(&mut st);
                        self.spawn_at_segment_locked(&mut st, segment_num).await?;
                    } else {
                        return Err(Error::Unavailable(format!(
                            "FFmpeg exited before producing segment {segment_num}.m4s"
                        )));
                    }
                }
            }
            if Instant::now() >= deadline {
                return Err(Error::Unavailable(format!(
                    "timed out waiting for HLS segment {segment_num}.m4s"
                )));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Evaluate ahead-segment distance and pause/resume FFmpeg accordingly.
    pub async fn tick_throttle(&self) {
        let mut st = self.state.lock().await;
        let running = poll_child_state(&mut st);
        if !running {
            st.paused = false;
            return;
        }
        let rendition_dir = self.base_pipeline.output.rendition_dir(0);
        refresh_head_segment(&rendition_dir, &mut st, true);

        let ahead = st.ffmpeg_head_segment.saturating_sub(st.last_requested_segment);
        if ahead > self.ahead_segments && !st.paused {
            pause_child(&st.child);
            st.paused = true;
            tracing::debug!(
                session_id = %self.id,
                head = st.ffmpeg_head_segment,
                last_req = st.last_requested_segment,
                ahead,
                "paused FFmpeg process (ahead of client)"
            );
        } else if ahead <= self.ahead_segments && st.paused {
            resume_child(&st.child);
            st.paused = false;
            tracing::debug!(
                session_id = %self.id,
                head = st.ffmpeg_head_segment,
                last_req = st.last_requested_segment,
                ahead,
                "resumed FFmpeg process (client caught up)"
            );
        }
    }

    /// Active hardware or software backend currently driving this session.
    pub async fn active_backend(&self) -> Backend {
        self.state.lock().await.active_backend
    }

    /// Whether the FFmpeg child process is currently paused via throttling.
    pub async fn is_paused(&self) -> bool {
        self.state.lock().await.paused
    }

    /// Number of seek/recovery restarts triggered on this session after initial start.
    pub async fn restart_count(&self) -> u32 {
        self.state.lock().await.restart_count
    }

    /// OS PID of the currently running FFmpeg child process, if any.
    pub async fn ffmpeg_pid(&self) -> Option<u32> {
        let mut st = self.state.lock().await;
        if poll_child_state(&mut st) { st.child.as_ref().and_then(Child::id) } else { None }
    }

    /// Last request timestamp on this session.
    pub async fn last_access(&self) -> Instant {
        self.state.lock().await.last_access
    }

    /// Last requested segment index on this session.
    pub async fn last_requested_segment(&self) -> u32 {
        self.state.lock().await.last_requested_segment
    }

    /// Kill FFmpeg and remove the session's temporary directory.
    pub async fn shutdown(&self) {
        let mut st = self.state.lock().await;
        kill_existing_child(&mut st).await;
        for attempt in 0..5 {
            match tokio::fs::remove_dir_all(self.dir.as_std_path()).await {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error)
                    if attempt < 4
                        && matches!(
                            error.kind(),
                            std::io::ErrorKind::PermissionDenied
                                | std::io::ErrorKind::DirectoryNotEmpty
                        ) =>
                {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(error) => {
                    tracing::warn!(
                        session_id = %self.id,
                        path = %self.dir,
                        %error,
                        "failed to remove HLS session directory"
                    );
                    return;
                }
            }
        }
    }

    fn advance_fallback_locked(&self, st: &mut SessionState) {
        let failed = st.active_backend;
        let codec_name = match &self.base_pipeline.video {
            VideoOp::Encode { codec, .. } => codec.as_str().to_owned(),
            _ => "h264".to_owned(),
        };
        if let Ok(mut map) = self.bad_backends.write() {
            map.insert((failed, codec_name.clone()), Instant::now() + self.bad_backend_ttl);
        }
        if !st.fallback_backends.is_empty() {
            let next = st.fallback_backends.remove(0);
            tracing::warn!(
                session_id = %self.id,
                failed_backend = ?failed,
                next_backend = ?next,
                codec = %codec_name,
                "FFmpeg failed within 5s on hardware backend; falling back transparently"
            );
            st.active_backend = next;
            st.last_exit_failed = false;
        }
    }

    async fn spawn_at_segment_locked(&self, st: &mut SessionState, segment_num: u32) -> Result<()> {
        kill_existing_child(st).await;

        let rendition_count = self.base_pipeline.output.renditions.len().max(1);
        for idx in 0..rendition_count {
            let rendition_dir = self.base_pipeline.output.rendition_dir(idx);
            std::fs::create_dir_all(rendition_dir.as_std_path())
                .map_err(|e| Error::Internal(Box::new(e)))?;

            // Remove stale FFmpeg internal playlist and any half-written target segment before spawning.
            let _ = std::fs::remove_file(rendition_dir.join("ffmpeg.m3u8").as_std_path());
            let _ = std::fs::remove_file(
                rendition_dir.join(format!("{segment_num}.m4s")).as_std_path(),
            );
        }

        let mut p = self.base_pipeline.clone();
        p.backend = st.active_backend;
        // If we fell back to a non-GPU backend on a multi-rendition pipeline, collapse to primary rendition 0
        if p.output.renditions.len() > 1 && !p.backend.is_gpu() {
            p.output.renditions.truncate(1);
        }
        p.start_ms = self.segment_start_ms(segment_num);
        p.output.start_number = segment_num;

        let args = build_args(&p, &self.caps)?;
        let child = Command::new(&self.ffmpeg_bin)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Unavailable(format!("failed to spawn ffmpeg: {e}")))?;

        st.current_start_segment = segment_num;
        st.ffmpeg_head_segment = segment_num;
        st.paused = false;
        st.last_exit_failed = false;
        st.last_spawn_at = Instant::now();
        st.child = Some(child);
        Ok(())
    }

    fn segment_start_ms(&self, segment_num: u32) -> u64 {
        let seg_ms = u64::from(self.base_pipeline.output.segment_secs.max(1)) * 1000;
        if self.fast_start_segments && self.base_pipeline.output.segment_secs >= 6 {
            match segment_num {
                0 => 0,
                1 => 3_000,
                n => 6_000 + u64::from(n - 2) * seg_ms,
            }
        } else {
            u64::from(segment_num) * seg_ms
        }
    }
}

/// Manages active HLS sessions, concurrency caps, inactivity cleanup, disk quotas, and bad-backend TTLs.
#[derive(Clone, Debug)]
pub struct SessionManager {
    config: Arc<SessionManagerConfig>,
    sessions: Arc<RwLock<HashMap<String, Arc<HlsSession>>>>,
    bad_backends: Arc<RwLock<HashMap<(Backend, String), Instant>>>,
    bad_backend_ttl: Duration,
}

impl SessionManager {
    pub fn new(config: SessionManagerConfig) -> Result<Self> {
        std::fs::create_dir_all(config.transcode_dir.as_std_path())
            .map_err(|e| Error::Internal(Box::new(e)))?;
        Ok(Self {
            config: Arc::new(config),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            bad_backends: Arc::new(RwLock::new(HashMap::new())),
            bad_backend_ttl: Duration::from_secs(600),
        })
    }

    pub fn with_bad_backend_ttl(mut self, ttl: Duration) -> Self {
        self.bad_backend_ttl = ttl;
        self
    }

    pub fn config(&self) -> &SessionManagerConfig {
        &self.config
    }

    /// Check whether `(backend, codec)` is currently marked bad in the TTL tracker (M4.9).
    pub fn is_backend_bad(&self, backend: Backend, codec: &str) -> bool {
        let now = Instant::now();
        let Ok(mut map) = self.bad_backends.write() else {
            return false;
        };
        map.retain(|_, exp| *exp > now);
        map.contains_key(&(backend, codec.to_ascii_lowercase()))
    }

    /// Number of active sessions (both video transcode and remux).
    pub fn active_count(&self) -> usize {
        self.sessions.read().map(|m| m.len()).unwrap_or(0)
    }

    /// Number of active video-encoding sessions.
    pub fn active_video_transcode_count(&self) -> usize {
        self.sessions
            .read()
            .map(|m| m.values().filter(|s| s.is_video_transcode).count())
            .unwrap_or(0)
    }

    /// Whether at least one video transcode slot is free.
    pub fn has_video_transcode_capacity(&self) -> bool {
        self.active_video_transcode_count() < self.config.max_video_sessions
    }

    /// Look up an active session by ID.
    pub fn get_session(&self, id: &str) -> Option<Arc<HlsSession>> {
        self.sessions.read().ok()?.get(id).cloned()
    }

    /// Create a new HLS session, write master and media playlists up front, and start FFmpeg.
    pub async fn create_session(&self, req: CreateSessionReq) -> Result<Arc<HlsSession>> {
        let default_fallbacks = if req.pipeline.backend != Backend::Software {
            vec![Backend::Software]
        } else {
            Vec::new()
        };
        self.create_session_with_fallbacks(req, default_fallbacks).await
    }

    /// Create a new HLS session with an explicit ordered list of fallback [`Backend`]s (M4.9).
    pub async fn create_session_with_fallbacks(
        &self,
        req: CreateSessionReq,
        mut fallback_backends: Vec<Backend>,
    ) -> Result<Arc<HlsSession>> {
        // Reap any expired sessions first so their slots and disk space are reclaimed.
        self.run_maintenance_once().await;

        let is_video_transcode = matches!(req.pipeline.video, VideoOp::Encode { .. });
        {
            let map = self.sessions.read().map_err(|_| {
                Error::Internal(std::io::Error::other("session map lock poisoned").into())
            })?;
            if is_video_transcode {
                let active_video = map.values().filter(|s| s.is_video_transcode).count();
                if active_video >= self.config.max_video_sessions {
                    return Err(Error::Unavailable(format!(
                        "maximum concurrent video transcode sessions ({}) reached",
                        self.config.max_video_sessions
                    )));
                }
            } else {
                let active_remux = map.values().filter(|s| !s.is_video_transcode).count();
                if active_remux >= self.config.max_remux_sessions {
                    return Err(Error::Unavailable(format!(
                        "maximum concurrent remux sessions ({}) reached",
                        self.config.max_remux_sessions
                    )));
                }
            }
        }

        let session_id = new_session_id()?;
        let session_dir = self.config.transcode_dir.join(&session_id);
        std::fs::create_dir_all(session_dir.as_std_path())
            .map_err(|e| Error::Internal(Box::new(e)))?;

        // Write master.m3u8 and <rendition>/index.m3u8 up front before FFmpeg spawns.
        std::fs::write(session_dir.join("master.m3u8").as_std_path(), &req.master_playlist)
            .map_err(|e| Error::Internal(Box::new(e)))?;

        for (rendition, media_playlist) in &req.media_playlists {
            validate_rendition_name(rendition)?;
            let r_dir = session_dir.join(rendition);
            std::fs::create_dir_all(r_dir.as_std_path())
                .map_err(|e| Error::Internal(Box::new(e)))?;
            std::fs::write(r_dir.join("index.m3u8").as_std_path(), media_playlist)
                .map_err(|e| Error::Internal(Box::new(e)))?;
        }

        let mut base_pipeline = req.pipeline;
        base_pipeline.output.dir = session_dir.clone();
        let initial_segment = base_pipeline.output.start_number;

        // Skip any primary or fallback backends already marked bad in the TTL tracker
        let codec_name = match &base_pipeline.video {
            VideoOp::Encode { codec, .. } => codec.as_str().to_owned(),
            _ => "h264".to_owned(),
        };
        fallback_backends
            .retain(|b| *b == Backend::Software || !self.is_backend_bad(*b, &codec_name));
        let mut active_backend = base_pipeline.backend;
        if active_backend != Backend::Software
            && self.is_backend_bad(active_backend, &codec_name)
            && !fallback_backends.is_empty()
        {
            active_backend = fallback_backends.remove(0);
        }

        let session = Arc::new(HlsSession {
            id: session_id.clone(),
            user_id: req.user_id,
            part_id: req.part_id,
            dir: session_dir,
            duration_ms: req.duration_ms,
            is_video_transcode,
            ffmpeg_bin: self.config.ffmpeg_bin.clone(),
            base_pipeline,
            caps: self.config.caps.clone(),
            ahead_segments: self.config.ahead_segments,
            segment_wait_timeout: self.config.segment_wait_timeout,
            fast_start_segments: req.fast_start_segments,
            bad_backends: self.bad_backends.clone(),
            bad_backend_ttl: self.bad_backend_ttl,
            state: Mutex::new(SessionState {
                child: None,
                last_exit_failed: false,
                last_spawn_at: Instant::now(),
                active_backend,
                fallback_backends,
                paused: false,
                current_start_segment: initial_segment,
                ffmpeg_head_segment: initial_segment,
                last_requested_segment: initial_segment,
                last_access: Instant::now(),
                restart_count: 0,
            }),
        });

        // Spawn FFmpeg immediately for the initial segment.
        {
            let mut st = session.state.lock().await;
            if let Err(e) = session.spawn_at_segment_locked(&mut st, initial_segment).await {
                let _ = std::fs::remove_dir_all(session.dir.as_std_path());
                return Err(e);
            }
        }

        if let Ok(mut map) = self.sessions.write() {
            map.insert(session_id, session.clone());
        }

        Ok(session)
    }

    /// Stop and clean up a specific session by ID.
    pub async fn stop_session(&self, id: &str) -> bool {
        let removed = self.sessions.write().ok().and_then(|mut m| m.remove(id));
        if let Some(session) = removed {
            session.shutdown().await;
            true
        } else {
            false
        }
    }

    /// Stop all active sessions and delete their temporary directories.
    pub async fn shutdown_all(&self) {
        let all: Vec<Arc<HlsSession>> = self
            .sessions
            .write()
            .map(|mut m| m.drain().map(|(_, s)| s).collect())
            .unwrap_or_default();
        for session in all {
            session.shutdown().await;
        }
    }

    /// Run one maintenance pass:
    /// 1. Reap sessions that have exceeded `inactivity_timeout` (default 60 s).
    /// 2. Tick ahead-segment throttling (`SIGSTOP` / `SIGCONT`) on active sessions.
    /// 3. Enforce global disk quota on `transcode_dir`.
    pub async fn run_maintenance_once(&self) {
        let snapshot: Vec<Arc<HlsSession>> =
            self.sessions.read().map(|m| m.values().cloned().collect()).unwrap_or_default();

        let now = Instant::now();
        let mut expired_ids = Vec::new();
        for session in &snapshot {
            let last = session.last_access().await;
            if now.saturating_duration_since(last) >= self.config.inactivity_timeout {
                expired_ids.push(session.id.clone());
            } else {
                session.tick_throttle().await;
            }
        }

        for id in expired_ids {
            tracing::info!(session_id = %id, "cleaning up inactive HLS session");
            self.stop_session(&id).await;
        }

        self.enforce_disk_quota().await;
    }

    /// Enforce `config.temp_quota_bytes` across the transcode directory by first pruning old
    /// already-played `.m4s` segments and then evicting the oldest inactive sessions if needed.
    pub async fn enforce_disk_quota(&self) {
        let total = dir_size_bytes(self.config.transcode_dir.as_std_path());
        if total <= self.config.temp_quota_bytes {
            return;
        }

        let mut sessions: Vec<(Instant, u32, Arc<HlsSession>)> = Vec::new();
        let snapshot: Vec<Arc<HlsSession>> =
            self.sessions.read().map(|m| m.values().cloned().collect()).unwrap_or_default();

        for s in snapshot {
            sessions.push((s.last_access().await, s.last_requested_segment().await, s));
        }
        // Oldest-accessed session first.
        sessions.sort_by_key(|(last, _, _)| *last);

        // Pass 1: delete segments strictly older than `last_requested_segment.saturating_sub(1)`.
        for (_, last_req, session) in &sessions {
            if last_req > &1 {
                let r_dir = session.base_pipeline.output.rendition_dir(0);
                for seg in 0..(last_req - 1) {
                    let _ = std::fs::remove_file(r_dir.join(format!("{seg}.m4s")).as_std_path());
                }
            }
        }

        if dir_size_bytes(self.config.transcode_dir.as_std_path()) <= self.config.temp_quota_bytes {
            return;
        }

        // Pass 2: evict least-recently-used sessions until under quota (or all but newest if >1).
        for (_, _, session) in &sessions {
            if dir_size_bytes(self.config.transcode_dir.as_std_path())
                <= self.config.temp_quota_bytes
            {
                break;
            }
            tracing::warn!(
                session_id = %session.id,
                quota_bytes = self.config.temp_quota_bytes,
                "evicting HLS session to enforce transcode disk quota"
            );
            self.stop_session(&session.id).await;
        }
    }

    /// Spawn a periodic background maintenance loop that runs every `interval` until cancelled.
    pub fn spawn_maintenance_loop(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let mgr = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                mgr.run_maintenance_once().await;
            }
        })
    }
}

fn validate_rendition_name(rendition: &str) -> Result<()> {
    if rendition.is_empty()
        || rendition.contains('/')
        || rendition.contains('\\')
        || rendition.contains("..")
        || rendition.contains('\0')
    {
        return Err(Error::Validation("invalid HLS rendition name".into()));
    }
    Ok(())
}

fn poll_child_state(st: &mut SessionState) -> bool {
    let Some(child) = st.child.as_mut() else {
        return false;
    };
    match child.try_wait() {
        Ok(Some(status)) => {
            st.last_exit_failed = !status.success();
            st.child = None;
            false
        }
        Ok(None) => true,
        Err(_) => {
            st.last_exit_failed = true;
            st.child = None;
            false
        }
    }
}

async fn kill_existing_child(st: &mut SessionState) {
    if let Some(mut child) = st.child.take() {
        if st.paused
            && let Some(pid) = child.id()
        {
            send_unix_signal(pid, "-CONT");
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    st.paused = false;
}

fn pause_child(child_slot: &Option<Child>) {
    if let Some(pid) = child_slot.as_ref().and_then(Child::id) {
        send_unix_signal(pid, "-STOP");
    }
}

fn resume_child(child_slot: &Option<Child>) {
    if let Some(pid) = child_slot.as_ref().and_then(Child::id) {
        send_unix_signal(pid, "-CONT");
    }
}

#[cfg(unix)]
fn send_unix_signal(pid: u32, sig_flag: &str) {
    let _ = std::process::Command::new("kill")
        .args([sig_flag, &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(unix))]
fn send_unix_signal(_pid: u32, _sig_flag: &str) {}

fn is_init_complete(rendition_dir: &Utf8PathBuf, child_running: bool) -> bool {
    let init_path = rendition_dir.join("init.mp4");
    let Ok(meta) = std::fs::metadata(init_path.as_std_path()) else {
        return false;
    };
    if meta.len() == 0 {
        return false;
    }
    if !child_running {
        return true;
    }
    // FFmpeg closes init.mp4 before writing #EXT-X-MAP:URI="init.mp4" to ffmpeg.m3u8 and before
    // opening the first media segment (.m4s).
    if let Ok(m3u8) = std::fs::read_to_string(rendition_dir.join("ffmpeg.m3u8").as_std_path())
        && m3u8.contains("init.mp4")
    {
        return true;
    }
    if let Ok(entries) = std::fs::read_dir(rendition_dir.as_std_path()) {
        for entry in entries.flatten() {
            if entry.file_name().to_str().is_some_and(|n| n.ends_with(".m4s")) {
                return true;
            }
        }
    }
    false
}

fn is_segment_complete(rendition_dir: &Utf8PathBuf, segment_num: u32, child_running: bool) -> bool {
    let seg_path = rendition_dir.join(format!("{segment_num}.m4s"));
    let Ok(meta) = std::fs::metadata(seg_path.as_std_path()) else {
        return false;
    };
    if meta.len() == 0 {
        return false;
    }
    if !child_running {
        return true;
    }
    // If FFmpeg has already created the next segment file, `segment_num.m4s` is closed.
    let next_seg = rendition_dir.join(format!("{}.m4s", segment_num.saturating_add(1)));
    if next_seg.exists() {
        return true;
    }
    // FFmpeg appends `{segment_num}.m4s` to `ffmpeg.m3u8` only after closing the segment file.
    if let Ok(m3u8) = std::fs::read_to_string(rendition_dir.join("ffmpeg.m3u8").as_std_path()) {
        let needle = format!("{segment_num}.m4s");
        if m3u8.lines().any(|line| line.trim() == needle) {
            return true;
        }
    }
    false
}

fn refresh_head_segment(rendition_dir: &Utf8PathBuf, st: &mut SessionState, child_running: bool) {
    let Ok(entries) = std::fs::read_dir(rendition_dir.as_std_path()) else {
        return;
    };
    let mut max_completed = st.current_start_segment;
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".m4s") else {
            continue;
        };
        let Ok(num) = stem.parse::<u32>() else {
            continue;
        };
        if num >= st.current_start_segment && is_segment_complete(rendition_dir, num, child_running)
        {
            max_completed = max_completed.max(num);
        }
    }
    st.ffmpeg_head_segment = max_completed;
}

fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if ft.is_dir() {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        } else if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn new_session_id() -> Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| {
        Error::Internal(std::io::Error::other(format!("getrandom failed: {e}")).into())
    })?;
    let mut hex = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}
