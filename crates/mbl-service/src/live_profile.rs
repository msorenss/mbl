//! Profile filtering and revocable login-backed live capabilities.
use super::{LiveLease, LiveOwner, LiveTvService};
use mbl_core::livetv::{Channel, EpgGridResponse, EpgProgram, Tuner};
use mbl_core::{Actor, ChannelId, Error, ProgramId, Result, SessionId, UserId, time::now_ms};
use mbl_db::repo::{m6, m9, sessions::SessionsRepo};

#[derive(Debug, Clone)]
pub(super) struct LiveAccess {
    user: UserId,
    login_hash: Vec<u8>,
    source: String,
    pub(super) program: Option<(ProgramId, i64)>,
}

fn limited(actor: &Actor) -> bool {
    !actor.is_admin && actor.content_rating_ceiling.is_some()
}
fn current_allowed(actor: &Actor, channel: &Channel) -> bool {
    (!channel.is_hidden || actor.is_admin)
        && (!limited(actor)
            || channel.current_program.as_ref().is_some_and(|p| {
                p.start_time <= now_ms()
                    && p.end_time > now_ms()
                    && actor.can_view_rating(p.rating.as_deref())
            }))
}
fn channel_for(actor: &Actor, mut channel: Channel) -> Channel {
    channel.can_play = !channel.is_virtual
        && (channel.stream_url.starts_with("http://")
            || channel.stream_url.starts_with("https://"))
        && current_allowed(actor, &channel);
    if !actor.is_admin {
        // Source URLs may include provider passwords; playback uses the owned relay.
        channel.stream_url.clear();
    }
    if channel.is_virtual && !actor.is_admin {
        channel.current_program = None;
        channel.next_program = None;
    }
    channel.current_program =
        channel.current_program.filter(|p| actor.can_view_rating(p.rating.as_deref()));
    channel.next_program =
        channel.next_program.filter(|p| actor.can_view_rating(p.rating.as_deref()));
    channel
}

impl LiveTvService {
    async fn live_actor(&self, owner: SessionId) -> Result<(Actor, Vec<u8>)> {
        let (user, hash) = SessionsRepo::new((*self.db).clone())
            .active_identity(owner, now_ms())
            .await?
            .ok_or(Error::Unauthorized)?;
        let actor = m6::actor_for_user(&self.db, user).await?.ok_or(Error::Unauthorized)?;
        Ok((actor, hash))
    }

    pub(super) async fn live_access(
        &self,
        owner: SessionId,
        channel: &Channel,
    ) -> Result<LiveAccess> {
        let (actor, hash) = self.live_actor(owner).await?;
        if !current_allowed(&actor, channel) {
            return Err(Error::Forbidden);
        }
        Ok(LiveAccess {
            user: actor.user_id,
            login_hash: hash,
            source: channel.stream_url.clone(),
            program: if limited(&actor) {
                channel.current_program.as_ref().map(|p| (p.id, p.end_time))
            } else {
                None
            },
        })
    }

    pub(super) async fn check_live_lease(&self, lease: &LiveLease) -> Result<()> {
        let LiveOwner::Client(owner) = lease.owner else {
            return Ok(());
        };
        let access = lease.access.as_ref().ok_or(Error::Forbidden)?;
        let (actor, hash) = self.live_actor(owner).await?;
        if actor.user_id != access.user || hash != access.login_hash {
            return Err(Error::Unauthorized);
        }
        let channel = self.get_channel(lease.channel_id).await?;
        if channel.is_virtual
            || channel.stream_url != access.source
            || !current_allowed(&actor, &channel)
        {
            return Err(Error::Forbidden);
        }
        if let Some((program, end)) = access.program {
            if end <= now_ms()
                || !channel
                    .current_program
                    .as_ref()
                    .is_some_and(|p| p.id == program && p.end_time == end)
            {
                return Err(Error::Forbidden);
            }
        } else if limited(&actor) {
            // A newly restricted profile must start a fresh, bounded encoder.
            return Err(Error::Forbidden);
        }
        Ok(())
    }

    pub async fn list_tuners_for(&self, actor: &Actor) -> Result<Vec<Tuner>> {
        let mut tuners = self.list_tuners().await?;
        if !actor.is_admin {
            for tuner in &mut tuners {
                tuner.url.clear();
            }
        }
        Ok(tuners)
    }
    pub async fn get_tuner_for(&self, actor: &Actor, id: mbl_core::TunerId) -> Result<Tuner> {
        let mut tuner = self.get_tuner(id).await?;
        if !actor.is_admin {
            tuner.url.clear();
        }
        Ok(tuner)
    }
    pub async fn list_channels_for(&self, actor: &Actor, hidden: bool) -> Result<Vec<Channel>> {
        Ok(self
            .list_channels(hidden && actor.is_admin)
            .await?
            .into_iter()
            .map(|c| channel_for(actor, c))
            .collect())
    }
    pub async fn get_channel_for(&self, actor: &Actor, id: ChannelId) -> Result<Channel> {
        let channel = self.get_channel(id).await?;
        if channel.is_hidden && !actor.is_admin {
            return Err(Error::NotFound);
        }
        Ok(channel_for(actor, channel))
    }
    pub async fn get_epg_grid_for(
        &self,
        actor: &Actor,
        start: i64,
        end: i64,
        hidden: bool,
    ) -> Result<EpgGridResponse> {
        if start < 0 || end <= start {
            return Err(Error::Validation("Invalid TV guide time window".into()));
        }
        let mut grid = self.get_epg_grid(start, end, hidden && actor.is_admin).await?;
        for row in &mut grid.channels {
            // Virtual programme ACLs require catalogue-aware synthesis (area 17).
            if row.channel.is_virtual && !actor.is_admin {
                row.programs.clear();
                row.channel.current_program = None;
                row.channel.next_program = None;
            }
            row.channel = channel_for(actor, row.channel.clone());
            row.programs.retain(|p| actor.can_view_rating(p.rating.as_deref()));
        }
        Ok(grid)
    }
    pub async fn search_guide_for(
        &self,
        actor: &Actor,
        query: &str,
        start: Option<i64>,
        end: Option<i64>,
        limit: u32,
    ) -> Result<Vec<EpgProgram>> {
        if start.is_some_and(|t| t < 0) || end.is_some_and(|t| t < 0) {
            return Err(Error::Validation("Invalid TV guide time".into()));
        }
        let limit = limit.min(200) as usize;
        let start = start.map(|t| if t < 10_000_000_000 { t * 1000 } else { t });
        let end = end.map(|t| if t < 10_000_000_000 { t * 1000 } else { t });
        let mut found = Vec::new();
        let mut after = (i64::MIN, i64::MIN);
        while found.len() < limit {
            let page = m9::search_epg_page(&self.db, query, start, end, after).await?;
            if page.is_empty() {
                break;
            }
            for row in page {
                after = (row.start_time, row.id);
                if !actor.can_view_rating(row.rating.as_deref()) {
                    continue;
                }
                if !actor.is_admin {
                    let Some(channel) = row.channel_id else {
                        continue;
                    };
                    let channel = self.get_channel(ChannelId(channel)).await?;
                    if channel.is_hidden || channel.is_virtual {
                        continue;
                    }
                }
                found.push(super::row_to_epg_program(row));
                if found.len() == limit {
                    break;
                }
            }
        }
        Ok(found)
    }
}
