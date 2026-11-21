//! Movie Night Matchmaker ("Swipe to Match") service (M7.19).

use mbl_core::events::{Event, EventBus};
use mbl_core::time::now_ms;
use mbl_core::{Actor, ArtworkId, Error, ItemId, LibraryId, Result, UserId};
use mbl_db::Db;
use mbl_db::repo::m7;
pub use mbl_db::repo::m7::MatchmakerSessionRow;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateMatchmakerSessionRequest {
    pub filter_config: Option<String>,
    /// Planned start (Unix ms). Voting closes then and the most liked title wins. Up to 14 days
    /// ahead.
    #[serde(default)]
    pub starts_at: Option<i64>,
    /// Other users to invite; they become members right away and are notified.
    #[serde(default)]
    pub invite_user_ids: Vec<UserId>,
}

/// How far ahead a movie night can be planned.
const MAX_PLAN_AHEAD_MS: i64 = 14 * 86_400_000;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerVoteRequest {
    pub user_name: String,
    pub item_id: ItemId,
    pub vote: i32, // 1 for right (like), 0 for left (pass)
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerMatchResult {
    pub matched: bool,
    pub matched_item_id: Option<ItemId>,
}

/// The link code guests use to join a movie night.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerGuestLink {
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct JoinMatchmakerAsGuest {
    /// Shown to the others; 1–40 characters.
    pub name: String,
}

/// Handed out once when a guest joins. The token identifies the guest from then on.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerGuestJoined {
    pub token: String,
    pub name: String,
}

/// A title as a guest sees it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerGuestTitle {
    pub id: ItemId,
    pub title: String,
    pub year: Option<i64>,
    pub poster_id: Option<ArtworkId>,
}

/// What a guest needs to vote: the titles (in the host's order before shuffling) and the result.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerGuestView {
    /// Seed for the shared shuffle, the same for everyone in the movie night.
    pub seed: String,
    pub name: String,
    pub member_count: i64,
    /// `active`, `matched` or `closed` (planned time passed and no one liked anything).
    pub status: String,
    /// Planned start, when voting closes.
    pub starts_at: Option<i64>,
    pub matched: Option<MatchmakerGuestTitle>,
    pub titles: Vec<MatchmakerGuestTitle>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatchmakerGuestVote {
    pub item_id: ItemId,
    /// 1 to like, 0 to pass.
    pub vote: i32,
}

/// The library a session swipes through.
fn session_library(session: &MatchmakerSessionRow) -> Option<LibraryId> {
    serde_json::from_str::<serde_json::Value>(&session.filter_config).ok()?["libraryId"]
        .as_i64()
        .map(LibraryId)
}

/// A guest, their movie night and the host whose access decides what they may see.
struct GuestContext {
    guest: m7::MatchmakerGuest,
    session: MatchmakerSessionRow,
    host: Actor,
}

#[derive(Debug, Clone)]
pub struct MatchmakerService {
    db: Arc<Db>,
    events: EventBus,
}

impl MatchmakerService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db, events: EventBus::default() }
    }

    /// Publish movie night notifications on the server's event bus.
    pub fn with_events(mut self, events: EventBus) -> Self {
        self.events = events;
        self
    }

    pub async fn create_session(
        &self,
        actor: &Actor,
        req: CreateMatchmakerSessionRequest,
    ) -> Result<MatchmakerSessionRow> {
        let user_id = actor.user_id;
        let session_id = format!("mm_{}", mbl_auth::new_token()?.value);
        let filter_config = req.filter_config.unwrap_or_else(|| "{}".into());
        if filter_config.len() > 4096
            || serde_json::from_str::<serde_json::Value>(&filter_config).is_err()
        {
            return Err(Error::Validation("filterConfig must be JSON up to 4096 bytes".into()));
        }

        let now = now_ms();
        if let Some(at) = req.starts_at
            && (at < now + 60_000 || at > now + MAX_PLAN_AHEAD_MS)
        {
            return Err(Error::Validation(
                "Choose a time at least a minute from now and at most 14 days ahead".into(),
            ));
        }
        let mut invited: Vec<UserId> =
            req.invite_user_ids.into_iter().filter(|id| *id != user_id).collect();
        invited.sort_by_key(|id| id.0);
        invited.dedup();
        if invited.len() > 15 {
            return Err(Error::Validation("Invite at most 15 people".into()));
        }
        let users = mbl_db::repo::users::UsersRepo::new((*self.db).clone());
        for id in &invited {
            if users.get(*id).await?.is_none() {
                return Err(Error::Validation(format!("There is no user {}", id.0)));
            }
        }

        m7::create_matchmaker_session(
            &self.db,
            &session_id,
            user_id,
            &filter_config,
            req.starts_at,
            &invited,
        )
        .await?;
        if let Some(starts_at) = req.starts_at {
            let host = users.get(user_id).await?.map(|u| u.display_name).unwrap_or_default();
            self.events.publish(Event::MovieNightScheduled {
                session_id: session_id.clone(),
                starts_at,
                host_id: user_id,
                host,
                user_ids: m7::matchmaker_member_ids(&self.db, &session_id).await?,
            });
        }
        m7::get_matchmaker_session(&self.db, &session_id).await?.ok_or(Error::NotFound)
    }

    /// Close voting on every planned movie night whose time has come (run periodically).
    pub async fn decide_due(&self) -> Result<usize> {
        let due = m7::due_matchmaker_sessions(&self.db, now_ms()).await?;
        let mut decided = 0;
        for id in due {
            if self.decide(&id).await? {
                decided += 1;
            }
        }
        Ok(decided)
    }

    /// Close voting on one session if its time has come, and tell its members.
    async fn decide(&self, id: &str) -> Result<bool> {
        let Some(winner) = m7::decide_matchmaker_session(&self.db, id, now_ms()).await? else {
            return Ok(false);
        };
        let title = match winner {
            Some(item) => mbl_db::repo::items::title_of(&self.db, item).await?,
            None => None,
        };
        tracing::info!(picked = winner.is_some(), "movie night voting closed");
        self.events.publish(Event::MovieNightDecided {
            session_id: id.to_owned(),
            item_id: winner,
            title,
            user_ids: m7::matchmaker_member_ids(&self.db, id).await?,
        });
        Ok(true)
    }

    pub async fn get_session(
        &self,
        actor: &Actor,
        id: &str,
    ) -> Result<Option<MatchmakerSessionRow>> {
        if !m7::is_matchmaker_member(&self.db, id, actor.user_id).await? {
            return Err(Error::NotFound);
        }
        self.decide(id).await?;
        let session = m7::get_matchmaker_session(&self.db, id).await?;
        if let Some(item) = session.as_ref().and_then(|s| s.matched_item_id) {
            crate::ItemService::new((*self.db).clone()).get(actor, item).await?;
        }
        Ok(session)
    }

    /// Let people without an account join (host or admin). Returns the existing code when guests
    /// are already allowed.
    pub async fn guest_link(&self, actor: &Actor, id: &str) -> Result<MatchmakerGuestLink> {
        let session = self.hosted(actor, id).await?;
        if session.status != "active" {
            return Err(Error::Conflict("this movie night is over".into()));
        }
        if let Some(code) = m7::matchmaker_guest_code(&self.db, id).await? {
            return Ok(MatchmakerGuestLink { code });
        }
        let code = mbl_auth::new_token()?.value;
        m7::set_matchmaker_guest_code(&self.db, id, Some(&code)).await?;
        Ok(MatchmakerGuestLink { code })
    }

    /// Stop allowing guests; removes the guests and their votes.
    pub async fn stop_guests(&self, actor: &Actor, id: &str) -> Result<()> {
        self.hosted(actor, id).await?;
        m7::set_matchmaker_guest_code(&self.db, id, None).await
    }

    /// A session the actor created (or any session for an admin).
    async fn hosted(&self, actor: &Actor, id: &str) -> Result<MatchmakerSessionRow> {
        let session = m7::get_matchmaker_session(&self.db, id).await?.ok_or(Error::NotFound)?;
        if session.created_by != actor.user_id && !actor.is_admin {
            return Err(if m7::is_matchmaker_member(&self.db, id, actor.user_id).await? {
                Error::Forbidden
            } else {
                Error::NotFound
            });
        }
        Ok(session)
    }

    /// Join through a guest link. The returned token is shown once.
    pub async fn join_as_guest(
        &self,
        code: &str,
        req: JoinMatchmakerAsGuest,
    ) -> Result<MatchmakerGuestJoined> {
        let name = req.name.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.is_empty() || name.chars().count() > 40 {
            return Err(Error::Validation("Enter a name of up to 40 characters".into()));
        }
        if code.is_empty() || code.len() > 100 {
            return Err(Error::NotFound);
        }
        let token = mbl_auth::new_token()?;
        let guest = m7::join_matchmaker_guest(&self.db, code, &name, &token.hash).await?;
        Ok(MatchmakerGuestJoined { token: token.value, name: guest.name })
    }

    /// The result of a session that no longer takes votes.
    fn finished(session: &MatchmakerSessionRow) -> Result<Option<MatchmakerMatchResult>> {
        match session.status.as_str() {
            "matched" => Ok(Some(MatchmakerMatchResult {
                matched: true,
                matched_item_id: session.matched_item_id,
            })),
            "closed" => Err(Error::Conflict("voting has closed".into())),
            _ => Ok(None),
        }
    }

    async fn guest(&self, token: &str) -> Result<GuestContext> {
        if token.is_empty() || token.len() > 100 {
            return Err(Error::Unauthorized);
        }
        let guest = m7::matchmaker_guest_by_token(&self.db, &mbl_auth::hash_token(token))
            .await?
            .ok_or(Error::Unauthorized)?;
        let session = m7::get_matchmaker_session(&self.db, &guest.session_id)
            .await?
            .ok_or(Error::NotFound)?;
        // Guests see what the host may see; a host who is gone ends the guests' access.
        let host = mbl_db::repo::m6::actor_for_user(&self.db, session.created_by)
            .await?
            .ok_or(Error::NotFound)?;
        Ok(GuestContext { guest, session, host })
    }

    fn title(item: mbl_db::repo::items::ItemSummary) -> MatchmakerGuestTitle {
        MatchmakerGuestTitle {
            id: item.id,
            title: item.title,
            year: item.year,
            poster_id: item.poster_id,
        }
    }

    /// The titles to vote on and the result so far.
    pub async fn guest_view(&self, token: &str) -> Result<MatchmakerGuestView> {
        let GuestContext { guest, session, host } = self.guest(token).await?;
        let items = crate::ItemService::new((*self.db).clone());
        let titles = match session_library(&session) {
            Some(library) => {
                let query = mbl_db::repo::items::ListQuery {
                    kind: None,
                    sort: Default::default(),
                    order: None,
                    genre: None,
                    year: None,
                    unwatched: false,
                    cursor: None,
                    limit: 200,
                };
                items
                    .list(&host, library, &query)
                    .await?
                    .items
                    .into_iter()
                    .map(Self::title)
                    .collect()
            }
            None => Vec::new(),
        };
        let matched = match session.matched_item_id {
            Some(id) => items.get(&host, id).await.ok().map(|item| MatchmakerGuestTitle {
                id: item.id,
                title: item.title,
                year: item.year,
                poster_id: None,
            }),
            None => None,
        };
        Ok(MatchmakerGuestView {
            seed: session.id,
            name: guest.name,
            member_count: session.member_count,
            status: session.status,
            starts_at: session.starts_at,
            matched,
            titles,
        })
    }

    pub async fn guest_vote(
        &self,
        token: &str,
        req: MatchmakerGuestVote,
    ) -> Result<MatchmakerMatchResult> {
        let GuestContext { guest, session, host } = self.guest(token).await?;
        if req.vote != 0 && req.vote != 1 {
            return Err(Error::Validation("vote must be 0 or 1".into()));
        }
        if let Some(result) = Self::finished(&session)? {
            return Ok(result);
        }
        let item = crate::ItemService::new((*self.db).clone()).get(&host, req.item_id).await?;
        if Some(item.library_id) != session_library(&session) {
            return Err(Error::NotFound);
        }
        let voter = format!("guest:{}", guest.id);
        m7::record_matchmaker_vote(&self.db, &session.id, &voter, req.item_id, req.vote).await?;
        let required = m7::matchmaker_member_count(&self.db, &session.id).await?.max(2);
        if let Some(matched) = m7::check_matchmaker_matches(&self.db, &session.id, required).await?
        {
            m7::complete_matchmaker_session(&self.db, &session.id, matched).await?;
            return Ok(MatchmakerMatchResult { matched: true, matched_item_id: Some(matched) });
        }
        Ok(MatchmakerMatchResult { matched: false, matched_item_id: None })
    }

    /// The host's access, for showing a poster to a guest.
    pub async fn guest_viewer(&self, token: &str) -> Result<Actor> {
        Ok(self.guest(token).await?.host)
    }

    pub async fn join_session(&self, actor: &Actor, id: &str) -> Result<()> {
        m7::join_matchmaker(&self.db, id, actor.user_id).await
    }

    pub async fn record_vote(
        &self,
        actor: &Actor,
        session_id: &str,
        req: MatchmakerVoteRequest,
        _required_participants: usize,
    ) -> Result<MatchmakerMatchResult> {
        let session = self.get_session(actor, session_id).await?.ok_or(Error::NotFound)?;
        if req.vote != 0 && req.vote != 1 {
            return Err(Error::Validation("vote must be 0 or 1".into()));
        }
        crate::ItemService::new((*self.db).clone()).get(actor, req.item_id).await?;
        let required_participants = m7::matchmaker_member_count(&self.db, session_id).await?.max(2);

        if let Some(result) = Self::finished(&session)? {
            return Ok(result);
        }

        m7::record_matchmaker_vote(
            &self.db,
            session_id,
            &format!("user:{}", actor.user_id.0),
            req.item_id,
            req.vote,
        )
        .await?;

        // Check if all participants voted positive for this item
        if let Some(matched_item) =
            m7::check_matchmaker_matches(&self.db, session_id, required_participants).await?
        {
            m7::complete_matchmaker_session(&self.db, session_id, matched_item).await?;
            Ok(MatchmakerMatchResult { matched: true, matched_item_id: Some(matched_item) })
        } else {
            Ok(MatchmakerMatchResult { matched: false, matched_item_id: None })
        }
    }
}
