//! Live TV and channel management service.
//!
//! Handles tuner discovery (HDHomeRun), M3U playlist importing, streaming XMLTV EPG
//! parsing, live stream resolution with deinterlacing, recording scheduling with series
//! rules and tuner conflict detection, and virtual 24/7 linear channels.

use crate::StreamService;
use crate::live_relay::LiveRelay;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::RwLock;

use mbl_core::ids::{
    ChannelId, ProgramId, RecordingId, RecordingRuleId, TunerId, UserId, VirtualChannelId,
};
use mbl_core::livetv::{
    Channel, CreateRecordingRuleRequest, CreateTunerRequest, EpgGridChannel, EpgGridResponse,
    EpgProgram, LiveStreamInfo, Recording, RecordingRule, RecordingStatus,
    ScheduleRecordingRequest, Tuner, TunerKind, TunerStatus, UpdateChannelRequest,
};
use mbl_core::plugins::{
    CreateVirtualChannelRequest, VirtualChannel, VirtualChannelItem, VirtualChannelMode,
    VirtualPlayhead,
};
use mbl_core::{Error, Result, SessionId};
use mbl_db::Db;
use mbl_db::repo::m9::{self, EpgProgramInsert};
use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LiveOwner {
    Client(SessionId),
    Recording(RecordingId),
}

#[derive(Debug, Clone)]
struct LiveLease {
    owner: LiveOwner,
    relay: Option<Arc<LiveRelay>>,
    channel_id: ChannelId,
    tuner_id: Option<TunerId>,
    expires_at: i64,
}

/// Live TV management and playback coordinator.
#[derive(Debug, Clone)]
pub struct LiveTvService {
    db: Arc<Db>,
    dvr: Option<crate::dvr::Dvr>,
    stream: StreamService,
    closed: Arc<AtomicBool>,
    client: reqwest::Client,
    /// In-memory counter for active streams per tuner to enforce tuner concurrency limits.
    active_tuner_streams: Arc<RwLock<HashMap<String, LiveLease>>>,
}

impl LiveTvService {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            dvr: None,
            stream: StreamService::new((*db).clone()),
            closed: Arc::default(),
            db,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            active_tuner_streams: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_stream(mut self, stream: StreamService) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_recordings(
        mut self,
        root: std::path::PathBuf,
        tools: Option<mbl_probe::FfTools>,
    ) -> Self {
        self.dvr = Some(crate::dvr::Dvr::new((*self.db).clone(), root, tools));
        self
    }

    pub async fn publish_recording(&self, id: RecordingId) -> Result<mbl_core::ItemId> {
        self.dvr
            .as_ref()
            .ok_or_else(|| Error::Unavailable("DVR storage is not configured".into()))?
            .publish(id)
            .await
    }

    pub async fn reconcile_recordings(&self) {
        if let Some(dvr) = &self.dvr {
            dvr.reconcile().await;
        }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    // -----------------------------------------------------------------------
    // Tuners
    // -----------------------------------------------------------------------

    pub async fn list_tuners(&self) -> Result<Vec<Tuner>> {
        let rows = m9::list_tuners(&self.db).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let status = match r.status.as_str() {
                    "online" => TunerStatus::Online,
                    "scanning" => TunerStatus::Scanning,
                    _ => TunerStatus::Offline,
                };
                Tuner {
                    id: TunerId(r.id),
                    name: r.name,
                    kind: match r.kind.as_str() {
                        "m3u" => TunerKind::M3u,
                        _ => TunerKind::Hdhomerun,
                    },
                    url: r.url,
                    status,
                    tuner_count: r.tuner_count as u32,
                    created_at: r.created_at,
                    updated_at: r.updated_at,
                }
            })
            .collect())
    }

    pub async fn add_tuner(&self, req: CreateTunerRequest) -> Result<Tuner> {
        let kind_str = match req.kind {
            TunerKind::Hdhomerun => "hdhomerun",
            TunerKind::M3u => "m3u",
        };

        let tuner_count = req.tuner_count.unwrap_or(2);
        let id = m9::create_tuner(&self.db, &req.name, kind_str, &req.url, tuner_count).await?;

        // If HDHomeRun, optionally fetch lineup immediately
        if matches!(req.kind, TunerKind::Hdhomerun) {
            let _ = self.sync_hdhomerun_lineup(id, &req.url).await;
        }

        self.get_tuner(id).await
    }

    pub async fn get_tuner(&self, id: TunerId) -> Result<Tuner> {
        let row = m9::get_tuner(&self.db, id).await?.ok_or(Error::NotFound)?;

        Ok(Tuner {
            id: TunerId(row.id),
            name: row.name,
            kind: match row.kind.as_str() {
                "m3u" => TunerKind::M3u,
                _ => TunerKind::Hdhomerun,
            },
            url: row.url,
            status: match row.status.as_str() {
                "online" => TunerStatus::Online,
                "scanning" => TunerStatus::Scanning,
                _ => TunerStatus::Offline,
            },
            tuner_count: row.tuner_count as u32,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    pub async fn delete_tuner(&self, id: TunerId) -> Result<bool> {
        let deleted = m9::delete_tuner(&self.db, id).await?;
        if deleted {
            let ids: Vec<_> = self
                .active_tuner_streams
                .read()
                .await
                .iter()
                .filter(|(_, l)| l.tuner_id == Some(id))
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                self.release_internal(&id).await;
            }
        }
        Ok(deleted)
    }

    /// Discovers HDHomeRun tuners on the local network via UDP broadcast, or via direct inspect.
    pub async fn discover_hdhomerun(&self) -> Result<Vec<CreateTunerRequest>> {
        let mut discovered = Vec::new();

        // HDHomeRun UDP broadcast discovery packet
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await;
        if let Ok(s) = socket {
            let _ = s.set_broadcast(true);
            let packet = [0x00, 0x02, 0x00, 0x01, 0x01, 0x02];
            let _ = s.send_to(&packet, "255.255.255.255:65001").await;

            // Wait briefly for responses
            let mut buf = [0u8; 1024];
            let timeout = tokio::time::sleep(std::time::Duration::from_millis(500));
            tokio::pin!(timeout);

            loop {
                tokio::select! {
                    res = s.recv_from(&mut buf) => {
                        if let Ok((len, addr)) = res
                            && len >= 8
                        {
                            let ip = addr.ip().to_string();
                            let base_url = format!("http://{ip}");
                            if let Ok(info) = self.inspect_hdhomerun(&base_url).await {
                                discovered.push(info);
                            }
                        }
                    }
                    _ = &mut timeout => {
                        break;
                    }
                }
            }
        }

        Ok(discovered)
    }

    pub async fn inspect_hdhomerun(&self, base_url: &str) -> Result<CreateTunerRequest> {
        let trimmed = base_url.trim_end_matches('/');
        let url = format!("{trimmed}/discover.json");
        let res = self.client.get(&url).send().await.map_err(Error::internal)?;

        let val: serde_json::Value = res.json().await.map_err(Error::internal)?;

        let name = val["FriendlyName"].as_str().unwrap_or("HDHomeRun Tuner").to_string();
        let tuner_count = val["TunerCount"].as_u64().map(|c| c as u32);

        Ok(CreateTunerRequest {
            name,
            kind: TunerKind::Hdhomerun,
            url: trimmed.to_string(),
            tuner_count,
        })
    }

    pub async fn sync_hdhomerun_lineup(&self, tuner_id: TunerId, base_url: &str) -> Result<u32> {
        let trimmed = base_url.trim_end_matches('/');
        let url = format!("{trimmed}/lineup.json");
        let res = self.client.get(&url).send().await.map_err(Error::internal)?;

        let channels: Vec<serde_json::Value> = res.json().await.map_err(Error::internal)?;

        let mut count = 0;
        for c in channels {
            let guide_number = c["GuideNumber"].as_str().unwrap_or("0");
            let guide_name = c["GuideName"].as_str().unwrap_or("Channel");
            let stream_url = match c["URL"].as_str() {
                Some(u) => u.to_string(),
                None => format!("{trimmed}/auto/v{guide_number}"),
            };

            m9::upsert_channel(
                &self.db,
                Some(tuner_id),
                guide_number,
                guide_name,
                None,
                None,
                &stream_url,
                Some(guide_number),
                false,
            )
            .await?;
            count += 1;
        }

        m9::auto_map_channels_to_epg(&self.db).await?;
        Ok(count)
    }

    // -----------------------------------------------------------------------
    // M3U Playlist Parsing & Import
    // -----------------------------------------------------------------------

    pub async fn import_m3u(&self, tuner_name: &str, content: &str) -> Result<TunerId> {
        if content.len() > 50 * 1024 * 1024 {
            return Err(Error::Validation(
                "M3U playlist exceeds maximum size limit (50 MB)".into(),
            ));
        }

        let tuner_id = m9::create_tuner(
            &self.db,
            tuner_name,
            "m3u",
            "file://playlist.m3u",
            4, // Default to 4 concurrent streams for M3U playlists
        )
        .await?;

        let parsed = parse_m3u_entries(content);
        for item in parsed {
            m9::upsert_channel(
                &self.db,
                Some(tuner_id),
                &item.channel_number,
                &item.name,
                item.callsign.as_deref(),
                item.logo_url.as_deref(),
                &item.stream_url,
                item.epg_channel_id.as_deref(),
                false,
            )
            .await?;
        }

        m9::auto_map_channels_to_epg(&self.db).await?;
        Ok(tuner_id)
    }

    // -----------------------------------------------------------------------
    // Streaming XMLTV Parser & Import
    // -----------------------------------------------------------------------

    pub async fn import_xmltv(&self, xml_bytes: &[u8]) -> Result<u32> {
        if xml_bytes.len() > 50 * 1024 * 1024 {
            return Err(Error::Validation(
                "XMLTV guide data exceeds maximum size limit (50 MB)".into(),
            ));
        }

        let programs = parse_xmltv_streaming(xml_bytes)?;
        let count = m9::batch_upsert_epg_programs(&self.db, &programs).await?;
        m9::auto_map_channels_to_epg(&self.db).await?;
        Ok(count)
    }

    // -----------------------------------------------------------------------
    // Channels
    // -----------------------------------------------------------------------

    pub async fn list_channels(&self, include_hidden: bool) -> Result<Vec<Channel>> {
        let rows = m9::list_channels(&self.db, include_hidden).await?;
        let now = mbl_core::time::now_ms();
        let mut channels = Vec::new();

        for r in rows {
            let (curr, next) =
                m9::get_current_and_next_program(&self.db, ChannelId(r.id), now).await?;
            channels.push(row_to_channel(
                r,
                curr.map(row_to_epg_program),
                next.map(row_to_epg_program),
            ));
        }

        Ok(channels)
    }

    pub async fn get_channel(&self, id: ChannelId) -> Result<Channel> {
        let row = m9::get_channel(&self.db, id).await?.ok_or(Error::NotFound)?;
        let now = mbl_core::time::now_ms();
        let (curr, next) = m9::get_current_and_next_program(&self.db, id, now).await?;
        Ok(row_to_channel(row, curr.map(row_to_epg_program), next.map(row_to_epg_program)))
    }

    pub async fn update_channel(
        &self,
        id: ChannelId,
        req: UpdateChannelRequest,
    ) -> Result<Channel> {
        m9::update_channel(
            &self.db,
            id,
            req.name.as_deref(),
            req.channel_number.as_deref(),
            req.epg_channel_id.as_ref().map(|s| Some(s.as_str())),
            req.is_favorite,
            req.is_hidden,
        )
        .await?;

        self.get_channel(id).await
    }

    pub async fn delete_channel(&self, id: ChannelId) -> Result<bool> {
        let deleted = m9::delete_channel(&self.db, id).await?;
        if deleted {
            let ids: Vec<_> = self
                .active_tuner_streams
                .read()
                .await
                .iter()
                .filter(|(_, l)| l.channel_id == id)
                .map(|(id, _)| id.clone())
                .collect();
            for id in ids {
                self.release_internal(&id).await;
            }
        }
        Ok(deleted)
    }

    // -----------------------------------------------------------------------
    // EPG Grid
    // -----------------------------------------------------------------------

    pub async fn get_epg_grid(
        &self,
        start_time: i64,
        end_time: i64,
        include_hidden: bool,
    ) -> Result<EpgGridResponse> {
        let channels = self.list_channels(include_hidden).await?;
        let now = mbl_core::time::now_ms();

        let start_time_ms =
            if start_time < 10_000_000_000 { start_time * 1000 } else { start_time };
        let end_time_ms = if end_time < 10_000_000_000 { end_time * 1000 } else { end_time };

        let mut grid_channels = Vec::new();

        for mut channel in channels {
            let prog_rows =
                m9::get_programs_for_channel(&self.db, channel.id, start_time_ms, end_time_ms)
                    .await?;
            let mut programs: Vec<EpgProgram> =
                prog_rows.into_iter().map(row_to_epg_program).collect();

            // If it's a virtual channel and has no EPG programs, generate synthetic programs on the fly
            if channel.is_virtual && programs.is_empty() {
                programs =
                    self.generate_synthetic_epg(channel.id, start_time_ms, end_time_ms).await;
            }

            if channel.current_program.is_none() {
                channel.current_program =
                    programs.iter().find(|p| p.start_time <= now && p.end_time > now).cloned();
            }

            if channel.next_program.is_none() {
                channel.next_program = programs.iter().find(|p| p.start_time >= now).cloned();
            }

            grid_channels.push(EpgGridChannel { channel, programs });
        }

        Ok(EpgGridResponse { start_time, end_time, channels: grid_channels })
    }

    pub async fn search_guide(
        &self,
        query: &str,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: u32,
    ) -> Result<Vec<EpgProgram>> {
        let start = start_time.map(|t| if t < 10_000_000_000 { t * 1000 } else { t });
        let end = end_time.map(|t| if t < 10_000_000_000 { t * 1000 } else { t });

        let rows = m9::search_epg(&self.db, query, start, end, limit).await?;
        Ok(rows.into_iter().map(row_to_epg_program).collect())
    }

    // -----------------------------------------------------------------------
    // Live Playback Engine
    // -----------------------------------------------------------------------

    pub async fn resolve_live_stream(
        &self,
        owner: SessionId,
        channel_id: ChannelId,
        renew: Option<&str>,
    ) -> Result<LiveStreamInfo> {
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Unavailable("live TV is shutting down".into()));
        }
        let channel = self.get_channel(channel_id).await?;
        if channel.is_virtual
            || !(channel.stream_url.starts_with("http://")
                || channel.stream_url.starts_with("https://"))
        {
            return Err(Error::Validation(
                "live relay requires an HTTP or HTTPS tuner source".into(),
            ));
        }
        let id = if let Some(id) = renew {
            let relay = {
                let leases = self.active_tuner_streams.read().await;
                let lease = leases.get(id).ok_or(Error::NotFound)?;
                if lease.owner != LiveOwner::Client(owner)
                    || lease.channel_id != channel_id
                    || lease.expires_at <= mbl_core::time::now_ms()
                {
                    return Err(Error::NotFound);
                }
                lease.relay.clone().ok_or(Error::NotFound)?
            };
            if relay.failed().await {
                self.release_internal(id).await;
                return Err(Error::Unavailable("live encoder failed".into()));
            }
            let mut leases = self.active_tuner_streams.write().await;
            let lease = leases.get_mut(id).ok_or(Error::NotFound)?;
            if lease.expires_at <= mbl_core::time::now_ms() {
                return Err(Error::NotFound);
            }
            lease.expires_at = mbl_core::time::now_ms() + 45_000;
            id.to_owned()
        } else {
            let id = self
                .reserve(LiveOwner::Client(owner), &channel, mbl_core::time::now_ms() + 45_000)
                .await?;
            let mut candidate =
                LiveCandidate { service: self.clone(), id: Some(id.clone()), relay: None };
            let started: Result<()> = async {
                let manager = self
                    .stream
                    .hls_manager()
                    .ok_or_else(|| Error::Unavailable("live encoder is not configured".into()))?;
                let capacity = manager.reserve_video_capacity().await?;
                let relay = {
                    // Publish the child while holding the same lock used by shutdown's snapshot.
                    // A cancelled/shutting-down request must never spawn an unowned late child.
                    let mut leases = self.active_tuner_streams.write().await;
                    if self.closed.load(Ordering::Acquire) {
                        return Err(Error::Unavailable("live TV is shutting down".into()));
                    }
                    let lease = leases.get_mut(&id).ok_or(Error::NotFound)?;
                    let relay = LiveRelay::spawn(
                        self.stream.ffmpeg_bin(),
                        manager.config().transcode_dir.join("live").as_std_path(),
                        &channel.stream_url,
                        capacity,
                    )?;
                    candidate.relay = Some(relay.clone());
                    lease.relay = Some(relay.clone());
                    relay
                };
                relay.prepare().await?;
                let mut leases = self.active_tuner_streams.write().await;
                if self.closed.load(Ordering::Acquire) {
                    return Err(Error::Unavailable("live TV is shutting down".into()));
                }
                let lease = leases.get_mut(&id).ok_or(Error::NotFound)?;
                if lease.expires_at <= mbl_core::time::now_ms() {
                    return Err(Error::NotFound);
                }
                lease.expires_at = mbl_core::time::now_ms() + 45_000;
                Ok(())
            }
            .await;
            if let Err(error) = started {
                candidate.abort().await;
                return Err(error);
            }
            candidate.id = None;
            candidate.relay = None;
            id
        };
        Ok(LiveStreamInfo {
            expires_at: mbl_core::time::now_ms() + 45_000,
            stream_url: format!("/api/v1/stream/live/{id}/index.m3u8"),
            session_id: id,
            channel_id,
            channel_name: channel.name,
            is_deinterlacing_required: false,
            target_format: "hls".into(),
        })
    }

    async fn reserve(
        &self,
        owner: LiveOwner,
        channel: &Channel,
        expires_at: i64,
    ) -> Result<String> {
        self.maintain().await;
        let capacity = if let Some(id) = channel.tuner_id {
            Some(self.get_tuner(id).await?.tuner_count as usize)
        } else {
            None
        };
        let mut leases = self.active_tuner_streams.write().await;
        // Serialize against deletion after an earlier channel/tuner lookup.
        self.get_channel(channel.id).await?;
        if self.closed.load(Ordering::Acquire) {
            return Err(Error::Unavailable("live TV is shutting down".into()));
        }
        if let Some(capacity) = capacity
            && leases.values().filter(|l| l.tuner_id == channel.tuner_id).count() >= capacity
        {
            return Err(Error::Conflict("all tuners are in use by live TV or DVR".into()));
        }
        if matches!(owner, LiveOwner::Client(_))
            && leases.values().filter(|l| l.owner == owner).count() >= 8
        {
            return Err(Error::Conflict("too many live streams".into()));
        }
        let id = mbl_auth::new_token()?.value;
        leases.insert(
            id.clone(),
            LiveLease {
                owner,
                relay: None,
                channel_id: channel.id,
                tuner_id: channel.tuner_id,
                expires_at,
            },
        );
        Ok(id)
    }

    pub async fn release_live_stream(
        &self,
        owner: SessionId,
        channel_id: ChannelId,
        id: &str,
    ) -> Result<()> {
        {
            let leases = self.active_tuner_streams.read().await;
            if let Some(lease) = leases.get(id)
                && (lease.owner != LiveOwner::Client(owner) || lease.channel_id != channel_id)
            {
                return Err(Error::NotFound);
            }
        }
        self.release_internal(id).await;
        Ok(())
    }

    async fn release_internal(&self, id: &str) {
        let relay = {
            let mut leases = self.active_tuner_streams.write().await;
            if let Some(lease) = leases.get_mut(id) {
                lease.expires_at = 0;
                lease.relay.clone()
            } else {
                return;
            }
        };
        if let Some(relay) = relay {
            relay.shutdown().await;
        }
        self.active_tuner_streams.write().await.remove(id);
    }

    pub async fn live_file(&self, id: &str, filename: &str) -> Result<std::path::PathBuf> {
        let relay = {
            let leases = self.active_tuner_streams.read().await;
            let lease = leases.get(id).ok_or(Error::NotFound)?;
            if lease.expires_at <= mbl_core::time::now_ms() {
                return Err(Error::NotFound);
            }
            lease.relay.clone().ok_or(Error::NotFound)?
        };
        relay.file(filename).await
    }

    pub async fn reserve_recording(
        &self,
        id: RecordingId,
        channel_id: ChannelId,
        end_time: i64,
    ) -> Result<RecordingLease> {
        let channel = self.get_channel(channel_id).await?;
        let key = self
            .reserve(LiveOwner::Recording(id), &channel, end_time.saturating_add(30_000))
            .await?;
        Ok(RecordingLease { service: self.clone(), key })
    }

    pub async fn maintain(&self) {
        let leases: Vec<_> = self
            .active_tuner_streams
            .read()
            .await
            .iter()
            .map(|(id, l)| (id.clone(), l.clone()))
            .collect();
        for (id, lease) in leases {
            let failed = if let Some(relay) = &lease.relay { relay.failed().await } else { false };
            let expired = {
                let mut leases = self.active_tuner_streams.write().await;
                if let Some(current) = leases.get_mut(&id) {
                    if failed || current.expires_at <= mbl_core::time::now_ms() {
                        current.expires_at = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if expired {
                self.release_internal(&id).await;
            }
        }
        if let Some(manager) = self.stream.hls_manager() {
            let mut live: Vec<_> = self
                .active_tuner_streams
                .read()
                .await
                .iter()
                .filter(|(_, l)| l.relay.is_some())
                .map(|(id, l)| (l.expires_at, id.clone()))
                .collect();
            live.sort();
            for (_, id) in live {
                if manager.temp_usage_bytes() <= manager.config().temp_quota_bytes {
                    break;
                }
                tracing::warn!(session_id = %id, "stopping live HLS to enforce transcode quota");
                self.release_internal(&id).await;
            }
        }
    }

    pub async fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        let ids: Vec<_> = self.active_tuner_streams.read().await.keys().cloned().collect();
        for id in ids {
            self.release_internal(&id).await;
        }
    }

    // -----------------------------------------------------------------------
    // Recording Scheduler & Series Rules
    // -----------------------------------------------------------------------

    pub async fn schedule_recording(&self, req: ScheduleRecordingRequest) -> Result<Recording> {
        let channel = self.get_channel(req.channel_id).await?;

        let epoch_ms = |time: i64| -> Result<i64> {
            if time <= 0 {
                return Err(Error::Validation("recording times must be positive".into()));
            }
            Ok(if time < 10_000_000_000 { time * 1000 } else { time })
        };
        let start = epoch_ms(req.start_time)?;
        let end = epoch_ms(req.end_time)?;
        if end <= start || req.title.trim().is_empty() {
            return Err(Error::Validation(
                "recording requires a title and an end after its start".into(),
            ));
        }
        let start_pad_ms = i64::from(req.start_padding_secs.unwrap_or(0)) * 1000;
        let end_pad_ms = i64::from(req.end_padding_secs.unwrap_or(0)) * 1000;
        let start_time = start
            .checked_sub(start_pad_ms)
            .ok_or_else(|| Error::Validation("recording time overflow".into()))?;
        let end_time = end
            .checked_add(end_pad_ms)
            .ok_or_else(|| Error::Validation("recording time overflow".into()))?;

        // Tuner conflict check
        if let Some(tuner_id) = channel.tuner_id {
            let tuner = self.get_tuner(tuner_id).await?;
            let overlapping =
                m9::count_overlapping_recordings(&self.db, channel.id, start_time, end_time)
                    .await?;

            if overlapping >= tuner.tuner_count as i64 {
                return Err(Error::Conflict(format!(
                    "Tuner conflict: all {} tuners on '{}' are scheduled during this time window",
                    tuner.tuner_count, tuner.name
                )));
            }
        }

        let id = m9::schedule_recording(
            &self.db,
            None,
            req.channel_id,
            req.program_id,
            &req.title,
            start_time,
            end_time,
        )
        .await?;

        self.get_recording(id).await
    }

    pub async fn create_series_rule(
        &self,
        user_id: UserId,
        req: CreateRecordingRuleRequest,
    ) -> Result<RecordingRule> {
        let start_pad = req.start_padding_secs.unwrap_or(0);
        let end_pad = req.end_padding_secs.unwrap_or(0);
        let auto_delete = req.auto_delete_watched.unwrap_or(false);

        let rule_id = m9::create_recording_rule(
            &self.db,
            user_id,
            &req.title,
            req.channel_id,
            req.series_only,
            req.season_number,
            req.new_only,
            start_pad,
            end_pad,
            req.keep_max,
            auto_delete,
        )
        .await?;

        // Schedule all upcoming matching programs in the guide
        let now = mbl_core::time::now_ms();
        let matches =
            m9::search_epg(&self.db, &req.title, Some(now), Some(now + 14 * 86_400_000), 100)
                .await?;

        for prog in matches {
            let Some(cid) = prog.channel_id else {
                continue;
            };

            if req.channel_id.is_none() || req.channel_id == Some(ChannelId(cid)) {
                if prog.title != req.title
                    || (req.series_only && prog.episode_number.is_none())
                    || req
                        .season_number
                        .is_some_and(|season| prog.season_number != Some(i64::from(season)))
                    || (req.new_only && prog.is_new == 0)
                {
                    continue;
                }

                let start_pad_ms = (start_pad as i64) * 1000;
                let end_pad_ms = (end_pad as i64) * 1000;

                let _ = m9::schedule_recording(
                    &self.db,
                    Some(rule_id),
                    ChannelId(cid),
                    Some(ProgramId(prog.id)),
                    &prog.title,
                    prog.start_time - start_pad_ms,
                    prog.end_time + end_pad_ms,
                )
                .await;
            }
        }

        self.get_recording_rule(rule_id).await
    }

    pub async fn list_recording_rules(
        &self,
        user_id: Option<UserId>,
    ) -> Result<Vec<RecordingRule>> {
        let rows = m9::list_recording_rules(&self.db, user_id).await?;
        Ok(rows.into_iter().map(row_to_recording_rule).collect())
    }

    pub async fn get_recording_rule(&self, id: RecordingRuleId) -> Result<RecordingRule> {
        let rows = m9::list_recording_rules(&self.db, None).await?;
        rows.into_iter().find(|r| r.id == id.0).map(row_to_recording_rule).ok_or(Error::NotFound)
    }

    pub async fn delete_recording_rule(&self, id: RecordingRuleId) -> Result<bool> {
        m9::delete_recording_rule(&self.db, id).await
    }

    pub async fn list_recordings(&self, status: Option<&str>) -> Result<Vec<Recording>> {
        let rows = m9::list_recordings(&self.db, status).await?;
        Ok(rows.into_iter().map(row_to_recording).collect())
    }

    /// Share recordings through the same library and rating filter as ordinary browsing.
    pub async fn list_recordings_for(
        &self,
        actor: &mbl_core::Actor,
        status: Option<&str>,
    ) -> Result<Vec<Recording>> {
        let rows = self.list_recordings(status).await?;
        let ids: Vec<_> = rows.iter().filter_map(|row| row.item_id).collect();
        let visible = mbl_db::repo::items::visible_ids(&self.db, actor, &ids).await?;
        Ok(rows
            .into_iter()
            .filter(|row| match row.item_id {
                Some(item) => visible.contains(&item),
                // Unpublished recordings have no library/rating decision yet. Restricted
                // profiles must not gain an alternate route to their metadata.
                None => {
                    actor.is_admin
                        || (matches!(actor.library_access, mbl_core::LibraryAccess::All)
                            && actor.content_rating_ceiling.is_none())
                }
            })
            .collect())
    }

    pub async fn get_recording_for(
        &self,
        actor: &mbl_core::Actor,
        id: RecordingId,
    ) -> Result<Recording> {
        self.list_recordings_for(actor, None)
            .await?
            .into_iter()
            .find(|row| row.id == id)
            .ok_or(Error::NotFound)
    }

    pub async fn get_recording(&self, id: RecordingId) -> Result<Recording> {
        let row = m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?;
        Ok(row_to_recording(row))
    }

    pub async fn cancel_recording(&self, id: RecordingId) -> Result<bool> {
        if let Some(dvr) = &self.dvr {
            return dvr.delete(id).await;
        }
        m9::delete_recording(&self.db, id).await
    }

    // -----------------------------------------------------------------------
    // Virtual 24/7 Channels
    // -----------------------------------------------------------------------

    pub async fn create_virtual_channel(
        &self,
        req: CreateVirtualChannelRequest,
    ) -> Result<VirtualChannel> {
        let mode_str = req.mode.as_str();

        // Create the backing Channel entity so it appears in guide and playback APIs
        let stream_url = format!("/api/v1/livetv/channels/virtual/{}/stream", req.channel_number);
        let channel_id = m9::upsert_channel(
            &self.db,
            None,
            &req.channel_number,
            &req.name,
            None,
            None,
            &stream_url,
            Some(&req.channel_number),
            true,
        )
        .await?;

        let mut items = Vec::new();
        for (i, item_id) in req.item_ids.iter().enumerate() {
            let item_info = m9::get_item_media_info(&self.db, *item_id).await.unwrap_or(None);

            if let Some((title, duration_ms, path)) = item_info {
                items.push(VirtualChannelItem {
                    item_id: *item_id,
                    title,
                    duration_ms: duration_ms.unwrap_or(1800 * 1000) as u64,
                    media_path: path.unwrap_or_else(|| format!("/media/item_{}.mp4", item_id.0)),
                });
            } else {
                items.push(VirtualChannelItem {
                    item_id: *item_id,
                    title: format!("Virtual Feature #{}", i + 1),
                    duration_ms: 1800 * 1000, // 30 mins
                    media_path: format!("/media/item_{}.mp4", item_id.0),
                });
            }
        }

        let id = m9::create_virtual_channel(
            &self.db,
            channel_id,
            &req.name,
            mode_str,
            &items,
            req.insert_commercials,
        )
        .await?;

        self.get_virtual_channel(id).await
    }

    pub async fn list_virtual_channels(&self) -> Result<Vec<VirtualChannel>> {
        let rows = m9::list_virtual_channels(&self.db).await?;
        Ok(rows.into_iter().map(row_to_virtual_channel).collect())
    }

    pub async fn get_virtual_channel(&self, id: VirtualChannelId) -> Result<VirtualChannel> {
        let row = m9::get_virtual_channel(&self.db, id).await?.ok_or(Error::NotFound)?;
        Ok(row_to_virtual_channel(row))
    }

    pub async fn delete_virtual_channel(&self, id: VirtualChannelId) -> Result<bool> {
        m9::delete_virtual_channel(&self.db, id).await
    }

    /// Calculates the active playhead for a virtual channel at `now_ms`.
    /// Zero CPU is consumed when not being actively played!
    pub async fn get_virtual_playhead(
        &self,
        id: VirtualChannelId,
        now_ms: i64,
    ) -> Result<VirtualPlayhead> {
        let vc = self.get_virtual_channel(id).await?;
        if vc.items.is_empty() {
            return Err(Error::NotFound);
        }

        let total_duration_ms: u64 = vc.items.iter().map(|i| i.duration_ms).sum();
        if total_duration_ms == 0 {
            return Err(Error::Validation("Virtual channel duration is 0".into()));
        }

        let loop_offset_ms = (now_ms.max(0) as u64) % total_duration_ms;

        let mut acc = 0u64;
        for (idx, item) in vc.items.iter().enumerate() {
            if loop_offset_ms < acc + item.duration_ms {
                let offset_secs = ((loop_offset_ms - acc) as f64) / 1000.0;
                let duration_secs = (item.duration_ms as f64) / 1000.0;
                let next_title = vc.items.get(idx + 1).map(|n| n.title.clone());

                return Ok(VirtualPlayhead {
                    channel_id: vc.channel_id,
                    channel_name: vc.name.clone(),
                    current_item_id: item.item_id,
                    current_title: item.title.clone(),
                    media_path: item.media_path.clone(),
                    offset_secs,
                    duration_secs,
                    next_title,
                    calculated_at: now_ms,
                });
            }
            acc += item.duration_ms;
        }

        // Fallback to first item
        let first = &vc.items[0];
        Ok(VirtualPlayhead {
            channel_id: vc.channel_id,
            channel_name: vc.name.clone(),
            current_item_id: first.item_id,
            current_title: first.title.clone(),
            media_path: first.media_path.clone(),
            offset_secs: 0.0,
            duration_secs: (first.duration_ms as f64) / 1000.0,
            next_title: vc.items.get(1).map(|n| n.title.clone()),
            calculated_at: now_ms,
        })
    }

    /// Generates synthetic EPG schedule entries for virtual linear channels.
    async fn generate_synthetic_epg(
        &self,
        channel_id: ChannelId,
        start_time: i64,
        end_time: i64,
    ) -> Vec<EpgProgram> {
        let vcs = match self.list_virtual_channels().await {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let vc = match vcs.into_iter().find(|v| v.channel_id == channel_id) {
            Some(v) => v,
            None => return Vec::new(),
        };

        if vc.items.is_empty() {
            return Vec::new();
        }

        let total_duration_ms: u64 = vc.items.iter().map(|i| i.duration_ms).sum();
        if total_duration_ms == 0 {
            return Vec::new();
        }

        let total_ms = total_duration_ms as i64;
        let mut programs = Vec::new();

        let mut loop_base = (start_time / total_ms) * total_ms;

        while loop_base < end_time {
            let mut acc = loop_base;
            for (idx, item) in vc.items.iter().enumerate() {
                let dur_ms = item.duration_ms as i64;
                let item_start = acc;
                let item_end = acc + dur_ms;

                if item_end > start_time && item_start < end_time {
                    programs.push(EpgProgram {
                        id: ProgramId((idx as i64) + 1_000_000),
                        channel_id: Some(channel_id),
                        epg_channel_id: format!("vc-{}", vc.id.0),
                        title: item.title.clone(),
                        description: Some(format!("Virtual Channel stream: {}", item.title)),
                        season_number: None,
                        episode_number: None,
                        start_time: item_start,
                        end_time: item_end,
                        icon_url: None,
                        rating: None,
                        genres: vec!["Linear".into()],
                        is_new: false,
                    });
                }
                acc += dur_ms;
            }
            loop_base += total_ms;
        }

        programs
    }
}

// ---------------------------------------------------------------------------
// Helpers & Parsers
// ---------------------------------------------------------------------------

/// Held by the DVR child for its full lifetime; cancellation also releases the tuner.
#[derive(Debug)]
pub struct RecordingLease {
    service: LiveTvService,
    key: String,
}
impl RecordingLease {
    pub async fn release(mut self) {
        self.service.release_internal(&self.key).await;
        self.key.clear();
    }
}
impl Drop for RecordingLease {
    fn drop(&mut self) {
        if !self.key.is_empty()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let service = self.service.clone();
            let key = self.key.clone();
            runtime.spawn(async move {
                service.release_internal(&key).await;
            });
        }
    }
}
struct LiveCandidate {
    service: LiveTvService,
    id: Option<String>,
    relay: Option<Arc<LiveRelay>>,
}
impl LiveCandidate {
    async fn abort(&mut self) {
        if let Some(id) = &self.id {
            self.service.release_internal(id).await;
        }
        if let Some(relay) = &self.relay {
            relay.shutdown().await;
        }
        self.id = None;
        self.relay = None;
    }
}
impl Drop for LiveCandidate {
    fn drop(&mut self) {
        if let Some(id) = self.id.take()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let service = self.service.clone();
            let relay = self.relay.take();
            runtime.spawn(async move {
                service.release_internal(&id).await;
                if let Some(relay) = relay {
                    relay.shutdown().await;
                }
            });
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedM3uItem {
    pub channel_number: String,
    pub name: String,
    pub callsign: Option<String>,
    pub logo_url: Option<String>,
    pub stream_url: String,
    pub epg_channel_id: Option<String>,
}

pub fn parse_m3u_entries(content: &str) -> Vec<ParsedM3uItem> {
    let mut items = Vec::new();
    let mut current_name = String::new();
    let mut current_logo: Option<String> = None;
    let mut current_epg: Option<String> = None;
    let mut current_num: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(after) = trimmed.strip_prefix("#EXTINF:") {
            current_logo = None;
            current_epg = None;
            current_num = None;

            if let Some((attrs, name_part)) = after.split_once(',') {
                current_name = name_part.trim().to_string();

                if let Some(id) = extract_attr(attrs, "tvg-id") {
                    current_epg = Some(id);
                }
                if let Some(logo) = extract_attr(attrs, "tvg-logo") {
                    current_logo = Some(logo);
                }
                if let Some(chno) = extract_attr(attrs, "tvg-chno") {
                    current_num = Some(chno);
                }
            } else {
                current_name = after.trim().to_string();
            }
        } else if !trimmed.starts_with('#') {
            let stream_url = trimmed.to_string();
            let channel_number = current_num.unwrap_or_else(|| (items.len() + 1).to_string());
            let name = if current_name.is_empty() {
                format!("Channel {channel_number}")
            } else {
                current_name.clone()
            };

            items.push(ParsedM3uItem {
                channel_number,
                name,
                callsign: None,
                logo_url: current_logo,
                stream_url,
                epg_channel_id: current_epg,
            });

            current_name.clear();
            current_logo = None;
            current_epg = None;
            current_num = None;
        }
    }

    items
}

fn extract_attr(haystack: &str, attr: &str) -> Option<String> {
    let key = format!("{attr}=\"");
    if let Some(pos) = haystack.find(&key) {
        let rest = &haystack[pos + key.len()..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Streaming XMLTV parser using `quick-xml`.
/// Reads `<channel>` and `<programme>` elements without full in-memory DOM allocations.
pub fn parse_xmltv_streaming(xml_bytes: &[u8]) -> Result<Vec<EpgProgramInsert>> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut programs = Vec::new();

    let mut in_programme = false;
    let mut current_channel = String::new();
    let mut current_start = 0i64;
    let mut current_stop = 0i64;
    let mut current_title = String::new();
    let mut current_desc = String::new();
    let mut current_genres = Vec::new();
    let mut current_icon: Option<String> = None;
    let mut current_rating: Option<String> = None;
    let mut is_new = false;
    let mut current_season: Option<u32> = None;
    let mut current_episode: Option<u32> = None;

    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                current_tag = name.to_string();

                if name == "programme" {
                    in_programme = true;
                    current_channel.clear();
                    current_start = 0;
                    current_stop = 0;
                    current_title.clear();
                    current_desc.clear();
                    current_genres.clear();
                    current_icon = None;
                    current_rating = None;
                    is_new = false;
                    current_season = None;
                    current_episode = None;

                    for attr in e.attributes().flatten() {
                        let k = attr.key.as_ref();
                        let v = attr.value.as_ref();
                        if k == "channel" {
                            current_channel = v.to_string();
                        } else if k == "start" {
                            current_start = parse_xmltv_date(v).unwrap_or(0);
                        } else if k == "stop" {
                            current_stop = parse_xmltv_date(v).unwrap_or(0);
                        }
                    }
                } else if in_programme && name == "icon" {
                    for attr in e.attributes().flatten() {
                        let k = attr.key.as_ref();
                        if k == "src" {
                            current_icon = Some(attr.value.to_string());
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if in_programme {
                    if name == "new" {
                        is_new = true;
                    } else if name == "icon" {
                        for attr in e.attributes().flatten() {
                            let k = attr.key.as_ref();
                            if k == "src" {
                                current_icon = Some(attr.value.to_string());
                            }
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_programme {
                    let text = e.as_ref();
                    match current_tag.as_str() {
                        "title" => current_title = text.to_string(),
                        "desc" => current_desc = text.to_string(),
                        "category" => current_genres.push(text.to_string()),
                        "value" => current_rating = Some(text.to_string()),
                        "episode-num" => {
                            if let Some((s, ep)) = parse_xmltv_ns(text) {
                                current_season = Some(s);
                                current_episode = Some(ep);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let qname = e.name();
                let name = qname.as_ref();
                if name == "programme" && in_programme {
                    in_programme = false;
                    if !current_channel.is_empty()
                        && !current_title.is_empty()
                        && current_stop > current_start
                    {
                        programs.push(EpgProgramInsert {
                            channel_id: None,
                            epg_channel_id: current_channel.clone(),
                            title: current_title.clone(),
                            description: Some(current_desc.clone()),
                            season_number: current_season,
                            episode_number: current_episode,
                            start_time: current_start,
                            end_time: current_stop,
                            icon_url: current_icon.clone(),
                            rating: current_rating.clone(),
                            genres: current_genres.clone(),
                            is_new,
                        });
                    }
                }
                current_tag.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Validation(format!("XMLTV parse error: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    Ok(programs)
}

/// Parses XMLTV timestamps, e.g. "20261026190000 +0200" or "20261026190000" into epoch milliseconds.
pub fn parse_xmltv_date(s: &str) -> Option<i64> {
    let clean = s.trim();
    if clean.len() < 14 || !clean.as_bytes()[0..14].iter().all(u8::is_ascii_digit) {
        return None;
    }

    let year: i32 = clean[0..4].parse().ok()?;
    let month: u32 = clean[4..6].parse().ok()?;
    let day: u32 = clean[6..8].parse().ok()?;
    let hour: u32 = clean[8..10].parse().ok()?;
    let minute: u32 = clean[10..12].parse().ok()?;
    let second: u32 = clean[12..14].parse().ok()?;

    let mut offset_mins = 0i32;
    if clean.len() > 14 {
        let rest = clean[14..].trim_start();
        if rest.len() >= 5 {
            let bytes = rest.as_bytes();
            let sign = bytes[0];
            if (sign == b'+' || sign == b'-') && bytes[1..5].iter().all(u8::is_ascii_digit) {
                let tz_hour: i32 = rest[1..3].parse().unwrap_or(0);
                let tz_min: i32 = rest[3..5].parse().unwrap_or(0);
                let total = tz_hour * 60 + tz_min;
                offset_mins = if sign == b'-' { -total } else { total };
            }
        }
    }

    let date =
        time::Date::from_calendar_date(year, time::Month::try_from(month as u8).ok()?, day as u8)
            .ok()?;
    let time = time::Time::from_hms(hour as u8, minute as u8, second as u8).ok()?;
    let dt = time::PrimitiveDateTime::new(date, time);

    let offset = time::UtcOffset::from_whole_seconds(offset_mins * 60).ok()?;
    let odt = dt.assume_offset(offset);

    Some(odt.unix_timestamp() * 1000)
}

/// Parses xmltv_ns episode format, e.g. "1 . 4 . " means Season 2, Episode 5 (0-indexed).
fn parse_xmltv_ns(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() >= 2 {
        let season = parts[0].trim().parse::<u32>().ok().map(|s| s + 1);
        let episode = parts[1].trim().parse::<u32>().ok().map(|e| e + 1);
        if let (Some(s), Some(e)) = (season, episode) {
            return Some((s, e));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn row_to_channel(
    r: m9::ChannelRow,
    current_program: Option<EpgProgram>,
    next_program: Option<EpgProgram>,
) -> Channel {
    Channel {
        id: ChannelId(r.id),
        tuner_id: r.tuner_id.map(TunerId),
        channel_number: r.channel_number,
        name: r.name,
        callsign: r.callsign,
        logo_url: r.logo_url,
        stream_url: r.stream_url,
        epg_channel_id: r.epg_channel_id,
        is_favorite: r.is_favorite != 0,
        is_hidden: r.is_hidden != 0,
        is_virtual: r.is_virtual != 0,
        current_program,
        next_program,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn row_to_epg_program(r: m9::EpgProgramRow) -> EpgProgram {
    let genres: Vec<String> = serde_json::from_str(&r.genres).unwrap_or_default();
    EpgProgram {
        id: ProgramId(r.id),
        channel_id: r.channel_id.map(ChannelId),
        epg_channel_id: r.epg_channel_id,
        title: r.title,
        description: r.description,
        season_number: r.season_number.map(|s| s as u32),
        episode_number: r.episode_number.map(|e| e as u32),
        start_time: r.start_time,
        end_time: r.end_time,
        icon_url: r.icon_url,
        rating: r.rating,
        genres,
        is_new: r.is_new != 0,
    }
}

fn row_to_recording(r: m9::RecordingRow) -> Recording {
    let status = match r.status.as_str() {
        "recording" => RecordingStatus::Recording,
        "completed" => RecordingStatus::Completed,
        "failed" => RecordingStatus::Failed,
        "cancelled" => RecordingStatus::Cancelled,
        _ => RecordingStatus::Scheduled,
    };

    Recording {
        id: RecordingId(r.id),
        item_id: r.item_id.map(mbl_core::ItemId),
        rule_id: r.rule_id.map(RecordingRuleId),
        channel_id: ChannelId(r.channel_id),
        channel_name: r.channel_name,
        program_id: r.program_id.map(ProgramId),
        title: r.title,
        start_time: r.start_time,
        end_time: r.end_time,
        status,
        output_path: None,
        size_bytes: r.size_bytes.map(|s| s as u64),
        duration_ms: r.duration_ms.map(|d| d as u64),
        error_message: r.error_message,
        created_at: r.created_at,
    }
}

fn row_to_recording_rule(r: m9::RecordingRuleRow) -> RecordingRule {
    RecordingRule {
        id: RecordingRuleId(r.id),
        user_id: UserId(r.user_id),
        title: r.title,
        channel_id: r.channel_id.map(ChannelId),
        series_only: r.series_only != 0,
        season_number: r.season_number.map(|s| s as u32),
        new_only: r.new_only != 0,
        start_padding_secs: r.start_padding_secs as u32,
        end_padding_secs: r.end_padding_secs as u32,
        keep_max: r.keep_max.map(|k| k as u32),
        auto_delete_watched: r.auto_delete_watched != 0,
        created_at: r.created_at,
    }
}

fn row_to_virtual_channel(r: m9::VirtualChannelRow) -> VirtualChannel {
    let mode = VirtualChannelMode::parse(&r.mode).unwrap_or(VirtualChannelMode::LinearLoop);
    let items: Vec<VirtualChannelItem> = serde_json::from_str(&r.items).unwrap_or_default();
    let total_duration_ms = items.iter().map(|i| i.duration_ms).sum();

    VirtualChannel {
        id: VirtualChannelId(r.id),
        channel_id: ChannelId(r.channel_id),
        name: r.name,
        mode,
        items,
        insert_commercials: r.insert_commercials != 0,
        total_duration_ms,
        created_at: r.created_at,
    }
}
