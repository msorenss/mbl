use super::*;
use crate::Authenticated;
use mbl_core::{
    SessionId, UserId,
    online::{OnlinePlayback, OnlineProgress},
};
use mbl_db::repo::sessions::SessionsRepo;

#[derive(Debug, Clone)]
pub(super) struct Session {
    owner: UserId,
    login: SessionId,
    identity: Vec<u8>,
    source: Arc<OnlineSource>,
    playlist: Option<Arc<hls::Playlist>>,
    expires: i64,
    active: Instant,
}
impl OnlineService {
    async fn login_identity(&self, auth: &Authenticated) -> Result<Vec<u8>> {
        Self::authorize(&auth.actor)?;
        let actor = mbl_db::repo::m6::actor_for_user(&self.db, auth.actor.user_id)
            .await?
            .ok_or(Error::Unauthorized)?;
        Self::authorize(&actor)?;
        self.require_enabled().await?;
        let (owner, hash) = SessionsRepo::new(self.db.clone())
            .active_identity(auth.session_id, mbl_core::time::now_ms())
            .await?
            .ok_or(Error::Unauthorized)?;
        if owner != actor.user_id {
            return Err(Error::Forbidden);
        }
        Ok(hash)
    }
    async fn owned(&self, auth: &Authenticated, id: &str) -> Result<Session> {
        let hash = self.login_identity(auth).await?;
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| {
            s.expires > mbl_core::time::now_ms() && s.active.elapsed() < Duration::from_secs(90)
        });
        let session = sessions.get_mut(id).ok_or(Error::NotFound)?;
        if session.owner != auth.actor.user_id
            || session.login != auth.session_id
            || session.identity != hash
        {
            return Err(Error::NotFound);
        }
        session.active = Instant::now();
        Ok(session.clone())
    }
    pub async fn start_online(&self, auth: &Authenticated, url: &str) -> Result<OnlinePlayback> {
        let hash = self.login_identity(auth).await?;
        let checked = self.inspect(&auth.actor, url).await?;
        let _permit = self
            .inspections
            .try_acquire()
            .map_err(|_| Error::RateLimited { retry_after_secs: 1 })?;
        let prepared = tokio::time::timeout(Duration::from_secs(45), async {
            if checked.availability == Availability::MediaDetected {
                return Ok((None, checked.media_url.clone().ok_or(Error::NotFound)?));
            }
            if checked.availability != Availability::ExternalPlayer {
                return Err(Error::Unavailable("No supported playable source was verified".into()));
            }
            let mut response = self.http.fetch(public_url(url)?, 512 * 1024, false).await?;
            if is_html(&response) {
                let mut metadata = source(url);
                let media = metadata::extract(
                    &mut metadata,
                    &response.url,
                    &String::from_utf8_lossy(&response.body),
                )
                .ok_or(Error::NotFound)?;
                response = self.http.fetch(public_url(&media)?, 512 * 1024, false).await?;
            }
            for _ in 0..3 {
                if response.status != 200 {
                    return Err(Error::Unavailable("HLS playlist is unavailable".into()));
                }
                let body = std::str::from_utf8(&response.body)
                    .map_err(|_| Error::Validation("Invalid HLS text".into()))?;
                if let Some(variant) = hls::variant(&response.url, body)? {
                    response = self.http.fetch(public_url(&variant)?, 512 * 1024, false).await?;
                    continue;
                }
                let playlist = hls::parse(&response.url, body)?;
                if playlist.urls.iter().map(String::len).sum::<usize>() > 1024 * 1024 {
                    return Err(Error::Validation("HLS resource map exceeds limit".into()));
                }
                let mut sample = Vec::new();
                for index in playlist.init.into_iter().chain(std::iter::once(playlist.first)) {
                    let segment = self
                        .http
                        .fetch(public_url(&playlist.urls[index])?, 16 * 1024 * 1024, false)
                        .await?;
                    if segment.status != 200 {
                        return Err(Error::Unavailable("HLS media is unavailable".into()));
                    }
                    sample.extend_from_slice(&segment.body);
                }
                probe::media_sample(&sample, self.tools.as_ref()).await?;
                return Ok((Some(Arc::new(playlist)), String::new()));
            }
            Err(Error::Validation("HLS master nesting exceeds limit".into()))
        })
        .await
        .map_err(|_| Error::Unavailable("Online playback preparation timed out".into()))??;
        if self.login_identity(auth).await? != hash {
            return Err(Error::Unauthorized);
        }
        let mut random = [0u8; 24];
        getrandom::fill(&mut random).map_err(Error::internal)?;
        let id = random.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let expires = mbl_core::time::now_ms() + 4 * 3600 * 1000;
        let position = self.repo.position(auth.actor.user_id, &checked).await?;
        let (playlist, direct) = prepared;
        let is_hls = playlist.is_some();
        let stream_url =
            if is_hls { format!("/api/v1/online/playback/{id}/index.m3u8") } else { direct };
        let mut sessions = self.sessions.lock().await;
        sessions.retain(|_, s| {
            s.expires > mbl_core::time::now_ms() && s.active.elapsed() < Duration::from_secs(90)
        });
        if sessions.len() >= 16
            || sessions.values().filter(|s| s.owner == auth.actor.user_id).count() >= 2
        {
            return Err(Error::RateLimited { retry_after_secs: 10 });
        }
        sessions.insert(
            id.clone(),
            Session {
                owner: auth.actor.user_id,
                login: auth.session_id,
                identity: hash,
                source: Arc::new(checked),
                playlist,
                expires,
                active: Instant::now(),
            },
        );
        Ok(OnlinePlayback { id, stream_url, hls: is_hls, position, expires_at: expires })
    }
    pub async fn online_manifest(&self, auth: &Authenticated, id: &str) -> Result<String> {
        let session = self.owned(auth, id).await?;
        let playlist = session.playlist.ok_or(Error::NotFound)?;
        Ok(playlist.lines.join("\n") + "\n")
    }
    pub async fn online_resource(
        &self,
        auth: &Authenticated,
        id: &str,
        index: usize,
    ) -> Result<Vec<u8>> {
        let session = self.owned(auth, id).await?;
        let url =
            session.playlist.as_ref().and_then(|p| p.urls.get(index)).ok_or(Error::NotFound)?;
        let _permit =
            self.transfers.try_acquire().map_err(|_| Error::RateLimited { retry_after_secs: 1 })?;
        let response = tokio::time::timeout(
            Duration::from_secs(12),
            self.http.fetch(public_url(url)?, 16 * 1024 * 1024, false),
        )
        .await
        .map_err(|_| Error::Unavailable("HLS segment timed out".into()))??;
        if response.status != 200 {
            return Err(Error::Unavailable("HLS segment is unavailable".into()));
        }
        self.owned(auth, id).await?;
        Ok(response.body)
    }
    pub async fn online_progress(
        &self,
        auth: &Authenticated,
        id: &str,
        progress: OnlineProgress,
    ) -> Result<()> {
        let session = self.owned(auth, id).await?;
        if !progress.position.is_finite()
            || !progress.duration.is_finite()
            || !(0.01..=86400.0).contains(&progress.duration)
            || !(0.0..=progress.duration + 1.0).contains(&progress.position)
        {
            return Err(Error::Validation("Invalid online playback position".into()));
        }
        if progress.heartbeat_only {
            return Ok(());
        }
        let duration = session.playlist.map(|p| p.duration).unwrap_or(progress.duration);
        if progress.position > duration + 1.0 {
            return Err(Error::Validation("Position exceeds programme duration".into()));
        }
        let position = if progress.ended && progress.position >= duration - 2.0 {
            0.0
        } else {
            progress.position.min(duration)
        };
        self.repo.progress(session.owner, &session.source, position, duration).await
    }
    pub async fn stop_online(&self, auth: &Authenticated, id: &str) -> Result<()> {
        self.owned(auth, id).await?;
        self.sessions.lock().await.remove(id);
        Ok(())
    }
}

impl OnlineService {
    pub async fn online_history(
        &self,
        actor: &Actor,
    ) -> Result<Vec<mbl_core::online::OnlineHistoryEntry>> {
        Self::authorize(actor)?;
        let actor = mbl_db::repo::m6::actor_for_user(&self.db, actor.user_id)
            .await?
            .ok_or(Error::Unauthorized)?;
        Self::authorize(&actor)?;
        self.require_enabled().await?;
        let mut rows = self.repo.history(actor.user_id).await?;
        for row in &mut rows {
            row.source = fresh(row.source.clone());
        }
        Ok(rows)
    }
}
