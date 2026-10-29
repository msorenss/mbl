//! Profile-authorized scheduling and background series/retention maintenance.
use super::{LiveTvService, row_to_recording, row_to_recording_rule};
use mbl_core::livetv::{
    CreateRecordingRuleRequest, Recording, RecordingPlan, RecordingRule, ScheduleRecordingRequest,
};
use mbl_core::{
    Actor, ChannelId, Error, ItemId, LibraryId, ProgramId, RecordingId, RecordingRuleId, Result,
    UserId, time::now_ms,
};
use mbl_db::repo::{items, m6, m9};

fn epoch_ms(value: i64) -> Result<i64> {
    if value <= 0 {
        return Err(Error::Validation("Recording times must be positive".into()));
    }
    Ok(if value < 10_000_000_000 { value * 1000 } else { value })
}

fn padding(value: Option<u32>) -> Result<u32> {
    let value = value.unwrap_or(0);
    if value > 86_400 {
        return Err(Error::Validation("Recording padding must not exceed 24 hours".into()));
    }
    Ok(value)
}

fn recording_window(start: i64, end: i64, before: u32, after: u32) -> Result<(i64, i64)> {
    let start = start
        .checked_sub(i64::from(before) * 1000)
        .filter(|n| *n > 0)
        .ok_or_else(|| Error::Validation("Invalid recording start or padding".into()))?;
    let end = end
        .checked_add(i64::from(after) * 1000)
        .ok_or_else(|| Error::Validation("Recording time overflow".into()))?;
    if end <= start {
        return Err(Error::Validation("Recording end must follow its start".into()));
    }
    Ok((start, end))
}

fn rating_access_level(rating: Option<&str>) -> u8 {
    // Missing ratings use the existing PG-or-higher policy, not the level-2
    // fallback used for an unrecognised non-empty rating label.
    match rating.filter(|rating| !rating.trim().is_empty()) {
        Some(rating) => mbl_core::actor::content_rating_level(rating),
        None => 1,
    }
}

impl LiveTvService {
    async fn recording_library(&self, actor: &Actor) -> Result<LibraryId> {
        self.dvr
            .as_ref()
            .ok_or_else(|| Error::Unavailable("DVR storage is not configured".into()))?
            .library(actor)
            .await
    }

    /// A preview performs the same normalization and authorization as scheduling,
    /// without creating storage directories, libraries, reservations or recordings.
    pub async fn preview_recording(
        &self,
        actor: &Actor,
        req: ScheduleRecordingRequest,
    ) -> Result<RecordingPlan> {
        Ok(self.prepare_recording(actor, req).await?.0)
    }

    async fn prepare_recording(
        &self,
        caller: &Actor,
        mut req: ScheduleRecordingRequest,
    ) -> Result<(RecordingPlan, Actor)> {
        let mut owner = caller.clone();
        let mut rule = None;
        if let Some(id) = req.retry_of {
            if !self.get_recording_for(caller, id).await?.can_delete {
                return Err(Error::Forbidden);
            }
            let old = m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?;
            if !matches!(old.status.as_str(), "failed" | "partial") {
                return Err(Error::Conflict(
                    "Only failed or partial recordings can be retried".into(),
                ));
            }
            let owner_id = old
                .owner_id
                .map(UserId)
                .ok_or_else(|| Error::Conflict("Recording owner no longer exists".into()))?;
            owner = m6::actor_for_user(&self.db, owner_id).await?.ok_or(Error::NotFound)?;
            req.channel_id = ChannelId(
                old.channel_id
                    .ok_or_else(|| Error::Conflict("Recording source was removed".into()))?,
            );
            req.program_id = old.program_id.map(ProgramId);
            if req.program_id.is_none() && owner.content_rating_ceiling.is_some() {
                req.program_id =
                    m9::get_epg_program_at(&self.db, req.channel_id.0, old.program_start_time)
                        .await?
                        .map(|p| ProgramId(p.id));
            }
            req.title = old.title;
            req.start_time = old.program_start_time;
            req.end_time = old.program_end_time;
            let before =
                u32::try_from(old.program_start_time.saturating_sub(old.start_time).max(0) / 1000)
                    .map_err(Error::internal)?;
            let after =
                u32::try_from(old.end_time.saturating_sub(old.program_end_time).max(0) / 1000)
                    .map_err(Error::internal)?;
            req.start_padding_secs = Some(before);
            req.end_padding_secs = Some(after);
            if let Some(id) = old.rule_id {
                let inherited = m9::list_recording_rules(&self.db, Some(owner.user_id))
                    .await?
                    .into_iter()
                    .find(|r| r.id == id)
                    .ok_or_else(|| {
                        Error::Conflict("Recording rule changed; preview the retry again".into())
                    })?;
                req.start_padding_secs =
                    Some(u32::try_from(inherited.start_padding_secs).map_err(Error::internal)?);
                req.end_padding_secs =
                    Some(u32::try_from(inherited.end_padding_secs).map_err(Error::internal)?);
                rule = Some(inherited);
            }
        }
        let channel = self.get_channel(req.channel_id).await?;
        if channel.is_virtual
            || !(channel.stream_url.starts_with("http://")
                || channel.stream_url.starts_with("https://"))
        {
            return Err(Error::Validation(
                "Recording requires an HTTP or HTTPS tuner channel".into(),
            ));
        }
        let before = padding(req.start_padding_secs)?;
        let after = padding(req.end_padding_secs)?;
        let (title, start, end) = match req.program_id {
            Some(id) => {
                let program = m9::get_epg_program(&self.db, id).await?.ok_or(Error::NotFound)?;
                if program.channel_id != Some(req.channel_id.0)
                    || !owner.can_view_rating(program.rating.as_deref())
                {
                    return Err(Error::NotFound);
                }
                (program.title, program.start_time, program.end_time)
            }
            None => {
                if owner.content_rating_ceiling.is_some() {
                    return Err(Error::Forbidden);
                }
                (req.title.trim().to_owned(), epoch_ms(req.start_time)?, epoch_ms(req.end_time)?)
            }
        };
        if title.is_empty() || title.len() > 512 || end <= start || end <= now_ms() {
            return Err(Error::Validation(
                "Recording requires a title and an unexpired programme window".into(),
            ));
        }
        let library = self
            .dvr
            .as_ref()
            .ok_or_else(|| Error::Unavailable("DVR storage is not configured".into()))?
            .library_preview(&owner)
            .await?;
        let (before, after) =
            if owner.content_rating_ceiling.is_some() { (0, 0) } else { (before, after) };
        let (capture_start, capture_end) = recording_window(start, end, before, after)?;
        let existing =
            m9::existing_owned_recording(&self.db, owner.user_id, req.channel_id, start).await?;
        if let Some(id) = existing {
            self.get_recording_for(&owner, id).await?;
        }
        let mut conflict = None;
        if existing.is_none()
            && let Some(tuner) = channel.tuner_id
        {
            let count = m9::count_overlapping_recordings(
                &self.db,
                req.channel_id,
                capture_start,
                capture_end,
            )
            .await?;
            if count >= i64::from(self.get_tuner(tuner).await?.tuner_count) {
                conflict = Some("All tuners are already reserved during this recording".into());
            }
        }
        let plan = RecordingPlan {
            owner_id: owner.user_id,
            library_id: library,
            rule_id: rule.as_ref().map(|r| RecordingRuleId(r.id)),
            retry_of: req.retry_of,
            channel_id: req.channel_id,
            program_id: req.program_id,
            title,
            start_time: capture_start,
            end_time: capture_end,
            program_start_time: start,
            program_end_time: end,
            start_padding_secs: before,
            end_padding_secs: after,
            keep_max: rule.as_ref().and_then(|r| r.keep_max).and_then(|n| u32::try_from(n).ok()),
            auto_delete_watched: rule.as_ref().is_some_and(|r| r.auto_delete_watched != 0),
            existing_recording_id: existing,
            conflict,
            storage: self.recording_storage_status().await?,
        };
        Ok((plan, owner))
    }

    pub async fn schedule_recording(
        &self,
        actor: &Actor,
        req: ScheduleRecordingRequest,
    ) -> Result<Recording> {
        let (plan, owner) = self.prepare_recording(actor, req).await?;
        if let Some(conflict) = &plan.conflict {
            return Err(Error::Conflict(conflict.clone()));
        }
        let library = self.recording_library(&owner).await?;
        let id = m9::schedule_owned_recording(
            &self.db,
            &m9::NewRecording {
                retry_of: plan.retry_of,
                owner_id: Some(owner.user_id),
                library_id: Some(library),
                rule_id: plan.rule_id,
                channel_id: plan.channel_id,
                program_id: plan.program_id,
                title: &plan.title,
                start_time: plan.start_time,
                end_time: plan.end_time,
                program_start_time: plan.program_start_time,
                program_end_time: plan.program_end_time,
            },
            false,
        )
        .await?;
        self.get_recording_for(actor, id).await
    }

    pub async fn create_series_rule(
        &self,
        actor: &Actor,
        req: CreateRecordingRuleRequest,
    ) -> Result<RecordingRule> {
        let title = req.title.trim();
        if title.is_empty()
            || title.len() > 512
            || req.keep_max == Some(0)
            || req.keep_max.is_some_and(|n| n > 10_000)
        {
            return Err(Error::Validation(
                "A series rule needs a title and a retention count between 1 and 10000".into(),
            ));
        }
        if let Some(channel) = req.channel_id {
            let channel = self.get_channel(channel).await?;
            if channel.is_virtual {
                return Err(Error::Validation("Virtual channels cannot be recorded".into()));
            }
        }
        let before = padding(req.start_padding_secs)?;
        let after = padding(req.end_padding_secs)?;
        self.recording_library(actor).await?;
        let id = m9::create_recording_rule(
            &self.db,
            actor.user_id,
            title,
            req.channel_id,
            req.series_only,
            req.season_number,
            req.new_only,
            before,
            after,
            req.keep_max,
            req.auto_delete_watched.unwrap_or(false),
        )
        .await?;
        let row = m9::list_recording_rules(&self.db, Some(actor.user_id))
            .await?
            .into_iter()
            .find(|r| r.id == id.0)
            .ok_or(Error::NotFound)?;
        if let Err(error) = self.match_recording_rule(actor, &row).await {
            tracing::warn!(rule_id=row.id,%error,"Series rule created; initial matching deferred");
        }
        Ok(row_to_recording_rule(row))
    }

    async fn match_recording_rule(&self, actor: &Actor, rule: &m9::RecordingRuleRow) -> Result<()> {
        let library = self.recording_library(actor).await?;
        let now = now_ms();
        let mut cursor = (now.saturating_sub(1), 0);
        loop {
            let programs = m9::matching_rule_programs(&self.db, rule, now, cursor).await?;
            let done = programs.len() < 200;
            for program in programs {
                cursor = (program.start_time, program.id);
                if program.start_time <= 0 || program.end_time <= program.start_time {
                    continue;
                }
                let Some(channel) = program.channel_id else { continue };
                if !actor.can_view_rating(program.rating.as_deref()) {
                    continue;
                }
                let channel_info = self.get_channel(ChannelId(channel)).await?;
                if channel_info.is_virtual
                    || !(channel_info.stream_url.starts_with("http://")
                        || channel_info.stream_url.starts_with("https://"))
                {
                    continue;
                }
                let (before, after) = if actor.content_rating_ceiling.is_some() {
                    (0, 0)
                } else {
                    (
                        padding(Some(
                            u32::try_from(rule.start_padding_secs).map_err(Error::internal)?,
                        ))?,
                        padding(Some(
                            u32::try_from(rule.end_padding_secs).map_err(Error::internal)?,
                        ))?,
                    )
                };
                let (capture_start, capture_end) = match recording_window(
                    program.start_time,
                    program.end_time,
                    before,
                    after,
                ) {
                    Ok(window) => window,
                    Err(error) => {
                        tracing::warn!(program_id=program.id,%error,"Invalid guide recording window skipped");
                        continue;
                    }
                };
                m9::schedule_owned_recording(
                    &self.db,
                    &m9::NewRecording {
                        retry_of: None,
                        owner_id: Some(actor.user_id),
                        library_id: Some(library),
                        rule_id: Some(RecordingRuleId(rule.id)),
                        channel_id: ChannelId(channel),
                        program_id: Some(ProgramId(program.id)),
                        title: &program.title,
                        start_time: capture_start,
                        end_time: capture_end,
                        program_start_time: program.start_time,
                        program_end_time: program.end_time,
                    },
                    true,
                )
                .await?;
            }
            if done {
                break;
            }
            tokio::task::yield_now().await;
        }
        Ok(())
    }

    /// Called periodically by the recorder, independently of due-recording execution.
    pub(crate) async fn refresh_recording_rules(&self) -> Result<()> {
        for rule in m9::list_recording_rules(&self.db, None).await? {
            let Some(actor) = m6::actor_for_user(&self.db, UserId(rule.user_id)).await? else {
                continue;
            };
            if let Err(error) = self.match_recording_rule(&actor, &rule).await {
                tracing::warn!(rule_id=rule.id,%error,"DVR rule matching failed");
            }
        }
        Ok(())
    }

    pub(crate) async fn apply_recording_retention(&self, playing: &[ItemId]) -> Result<()> {
        if let Some(dvr) = &self.dvr {
            dvr.maintain_retention(playing).await?;
        }
        m9::prune_deleted_recordings(&self.db, now_ms()).await?;
        Ok(())
    }

    /// Recheck current permissions before claiming a job and during capture.
    pub async fn authorize_recording_job(&self, row: &m9::RecordingRow) -> Result<()> {
        let channel = row.channel_id.ok_or(Error::NotFound)?;
        if let Some(owner) = row.owner_id {
            let actor =
                m6::actor_for_user(&self.db, UserId(owner)).await?.ok_or(Error::Forbidden)?;
            let library = match row.library_id {
                Some(id) => LibraryId(id),
                None => self.recording_library(&actor).await?,
            };
            if !actor.can_access_library(library)
                || !actor.can_view_rating(row.content_rating.as_deref())
            {
                return Err(Error::Forbidden);
            }
            let programme = match row.program_id {
                Some(id) => m9::get_epg_program(&self.db, ProgramId(id)).await?,
                None if actor.content_rating_ceiling.is_some() => {
                    m9::get_epg_program_at(&self.db, channel, row.program_start_time).await?
                }
                None => None,
            };
            if actor.content_rating_ceiling.is_some()
                && (programme.is_none()
                    || row.start_time != row.program_start_time
                    || row.end_time != row.program_end_time)
            {
                return Err(Error::Forbidden);
            }
            if let Some(program) = programme
                && (program.channel_id != Some(channel)
                    || program.start_time != row.program_start_time
                    || program.end_time != row.program_end_time
                    || !program.title.eq_ignore_ascii_case(&row.title)
                    || rating_access_level(program.rating.as_deref())
                        > rating_access_level(row.content_rating.as_deref())
                    || !actor.can_view_rating(program.rating.as_deref()))
            {
                return Err(Error::Validation(
                    "Guide programme changed; reschedule the recording".into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn list_recording_rules(&self, actor: &Actor) -> Result<Vec<RecordingRule>> {
        let rows = m9::list_recording_rules(
            &self.db,
            if actor.is_admin { None } else { Some(actor.user_id) },
        )
        .await?;
        Ok(rows.into_iter().map(row_to_recording_rule).collect())
    }
    pub async fn get_recording_rule(
        &self,
        actor: &Actor,
        id: RecordingRuleId,
    ) -> Result<RecordingRule> {
        self.list_recording_rules(actor)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)
    }
    pub async fn delete_recording_rule(&self, actor: &Actor, id: RecordingRuleId) -> Result<bool> {
        self.get_recording_rule(actor, id).await?;
        m9::delete_recording_rule(&self.db, id).await
    }

    pub async fn list_recordings(&self, status: Option<&str>) -> Result<Vec<Recording>> {
        Ok(m9::list_recordings(&self.db, status).await?.into_iter().map(row_to_recording).collect())
    }
    pub async fn list_recordings_for(
        &self,
        actor: &Actor,
        status: Option<&str>,
    ) -> Result<Vec<Recording>> {
        let rows = m9::list_recordings(&self.db, status).await?;
        let ids: Vec<_> = rows.iter().filter_map(|r| r.item_id.map(ItemId)).collect();
        let visible = items::visible_ids(&self.db, actor, &ids).await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let allowed = match row.item_id {
                    Some(id) => visible.contains(&ItemId(id)),
                    None => {
                        actor.is_admin
                            || (row.owner_id == Some(actor.user_id.0)
                                && row
                                    .library_id
                                    .is_some_and(|id| actor.can_access_library(LibraryId(id)))
                                && actor.can_view_rating(row.content_rating.as_deref()))
                    }
                };
                if !allowed {
                    return None;
                }
                let can_delete = actor.is_admin || row.owner_id == Some(actor.user_id.0);
                let mut recording = row_to_recording(row);
                recording.can_delete = can_delete;
                Some(recording)
            })
            .collect())
    }
    pub async fn get_recording_for(&self, actor: &Actor, id: RecordingId) -> Result<Recording> {
        self.list_recordings_for(actor, None)
            .await?
            .into_iter()
            .find(|r| r.id == id)
            .ok_or(Error::NotFound)
    }
    pub async fn get_recording(&self, id: RecordingId) -> Result<Recording> {
        Ok(row_to_recording(m9::get_recording(&self.db, id).await?.ok_or(Error::NotFound)?))
    }
    pub async fn cancel_recording(&self, actor: &Actor, id: RecordingId) -> Result<bool> {
        if !self.get_recording_for(actor, id).await?.can_delete {
            return Err(Error::Forbidden);
        }
        if let Some(dvr) = &self.dvr {
            return dvr.delete(id).await;
        }
        m9::delete_recording(&self.db, id).await
    }
}
