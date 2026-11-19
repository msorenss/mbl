//! MCP AI Watch Companion ("Co-watching with Agent", e.g. Hermes) (M7.16).

use mbl_core::{Actor, Error, ItemId, Result, time::now_ms};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionReaction {
    pub session_id: String,
    pub bot_name: String,
    pub timestamp_ms: i64,
    pub commentary: String,
    pub sentiment: Option<String>, // "funny", "shocked", "lore", "trivia", "sad"
    pub created_at: i64,
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
    pub current_subtitle_cue: Option<String>,
    pub connected_companions: Vec<String>,
}

/// Reactions kept per playback session so the player can show them.
const RECENT_PER_SESSION: usize = 50;
/// Sessions with stored reactions; the oldest is dropped beyond this.
const RECENT_SESSIONS: usize = 256;

#[derive(Debug, Clone)]
pub struct CompanionService {
    sessions: Arc<RwLock<HashMap<String, LiveSessionContext>>>,
    recent: Arc<RwLock<HashMap<String, VecDeque<CompanionReaction>>>>,
    playback: Option<crate::PlaybackService>,
    db: Option<mbl_db::Db>,
    reactions_tx: broadcast::Sender<CompanionReaction>,
}

impl CompanionService {
    pub fn new() -> Self {
        let (reactions_tx, _) = broadcast::channel(128);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            recent: Arc::new(RwLock::new(HashMap::new())),
            reactions_tx,
            playback: None,
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

    /// Live context including the chapter at the current position.
    pub async fn live_context(
        &self,
        actor: &Actor,
        session_id: &str,
    ) -> Result<LiveSessionContext> {
        let mut context = self.get_live_context(actor, session_id)?;
        let part = self
            .playback
            .as_ref()
            .and_then(|p| p.get_session(actor, session_id))
            .map(|s| s.part_id);
        if let (Some(db), Some(part)) = (&self.db, part)
            && let Ok(Some(raw)) = mbl_db::repo::media::part_chapters(db, part).await
            && let Ok(chapters) = serde_json::from_str::<Vec<mbl_probe::Chapter>>(&raw)
        {
            context.current_chapter = chapter_at(&chapters, context.position_ms);
        }
        Ok(context)
    }

    /// Reactions for a session created after `after_ms`, oldest first.
    pub fn reactions_since(
        &self,
        actor: &Actor,
        session_id: &str,
        after_ms: i64,
    ) -> Result<CompanionFeed> {
        let context = self.get_live_context(actor, session_id)?;
        let recent = self
            .recent
            .read()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        let reactions = recent
            .get(session_id)
            .map(|list| list.iter().filter(|r| r.created_at > after_ms).cloned().collect())
            .unwrap_or_default();
        Ok(CompanionFeed { companions: context.connected_companions, reactions })
    }

    pub fn join_session(&self, actor: &Actor, session_id: &str, bot_name: &str) -> Result<()> {
        let context = self.get_live_context(actor, session_id)?;
        if bot_name.trim().is_empty() || bot_name.len() > 100 {
            return Err(Error::Validation("companion name must contain 1–100 characters".into()));
        }
        let mut lock = self
            .sessions
            .write()
            .map_err(|_| Error::Unavailable("companion lock unavailable".into()))?;
        let ctx = lock.entry(session_id.to_owned()).or_insert(context);
        if !ctx.connected_companions.iter().any(|b| b == bot_name) {
            if ctx.connected_companions.len() >= 16 {
                return Err(Error::Conflict("too many companions".into()));
            }
            ctx.connected_companions.push(bot_name.to_owned());
        }
        Ok(())
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
        Ok(LiveSessionContext {
            session_id: session_id.to_owned(),
            item_id: session.item_id,
            title: session.title,
            position_ms: session.position_ms as i64,
            duration_ms: session.duration_ms as i64,
            current_chapter: None,
            current_subtitle_cue: None,
            connected_companions: lock
                .get(session_id)
                .map(|s| s.connected_companions.clone())
                .unwrap_or_default(),
        })
    }

    pub fn send_reaction(
        &self,
        actor: &Actor,
        session_id: &str,
        bot_name: &str,
        timestamp_ms: i64,
        commentary: &str,
        sentiment: Option<&str>,
    ) -> Result<CompanionReaction> {
        let context = self.get_live_context(actor, session_id)?;
        if !context.connected_companions.iter().any(|name| name == bot_name) {
            return Err(Error::NotFound);
        }
        if timestamp_ms < 0
            || timestamp_ms > context.position_ms.saturating_add(10_000)
            || commentary.trim().is_empty()
            || commentary.len() > 4000
        {
            return Err(Error::Validation("invalid reaction position or commentary".into()));
        }
        let reaction = CompanionReaction {
            session_id: session_id.to_owned(),
            bot_name: bot_name.to_owned(),
            timestamp_ms,
            commentary: commentary.to_owned(),
            sentiment: sentiment.map(|s| s.to_owned()),
            created_at: now_ms(),
        };

        if let Ok(mut recent) = self.recent.write() {
            if !recent.contains_key(session_id) && recent.len() >= RECENT_SESSIONS {
                let oldest = recent
                    .iter()
                    .min_by_key(|(_, list)| list.back().map_or(0, |r| r.created_at))
                    .map(|(id, _)| id.clone());
                if let Some(oldest) = oldest {
                    recent.remove(&oldest);
                }
            }
            let list = recent.entry(session_id.to_owned()).or_default();
            if list.len() >= RECENT_PER_SESSION {
                list.pop_front();
            }
            list.push_back(reaction.clone());
        }
        let _ = self.reactions_tx.send(reaction.clone());
        Ok(reaction)
    }

    pub fn subscribe_reactions(&self) -> broadcast::Receiver<CompanionReaction> {
        self.reactions_tx.subscribe()
    }
}

/// Connected companions and their latest reactions for one playback session.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CompanionFeed {
    pub companions: Vec<String>,
    pub reactions: Vec<CompanionReaction>,
}

fn chapter_at(chapters: &[mbl_probe::Chapter], position_ms: i64) -> Option<String> {
    let position = u64::try_from(position_ms).ok()?;
    chapters
        .iter()
        .find(|c| c.start_ms <= position && position < c.end_ms)
        .and_then(|c| c.title.clone())
        .filter(|t| !t.trim().is_empty())
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
}
