//! MCP AI Watch Companion ("Co-watching with Agent", e.g. Hermes) (M7.16).

use mbl_core::{Actor, Error, ItemId, Result, time::now_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Debug, Clone)]
pub struct CompanionService {
    sessions: Arc<RwLock<HashMap<String, LiveSessionContext>>>,
    reactions_tx: broadcast::Sender<CompanionReaction>,
}

impl CompanionService {
    pub fn new() -> Self {
        let (reactions_tx, _) = broadcast::channel(128);
        Self { sessions: Arc::new(RwLock::new(HashMap::new())), reactions_tx }
    }

    pub fn update_session_context(&self, ctx: LiveSessionContext) {
        if let Ok(mut lock) = self.sessions.write() {
            lock.insert(ctx.session_id.clone(), ctx);
        }
    }

    pub fn join_session(&self, _actor: &Actor, session_id: &str, bot_name: &str) -> Result<()> {
        if let Ok(mut lock) = self.sessions.write() {
            if let Some(ctx) = lock.get_mut(session_id) {
                if !ctx.connected_companions.iter().any(|b| b == bot_name) {
                    ctx.connected_companions.push(bot_name.to_owned());
                }
            } else {
                // Initialize placeholder context if session is warming up
                lock.insert(
                    session_id.to_owned(),
                    LiveSessionContext {
                        session_id: session_id.to_owned(),
                        item_id: ItemId(0),
                        title: "Media Stream".into(),
                        position_ms: 0,
                        duration_ms: 0,
                        current_chapter: None,
                        current_subtitle_cue: None,
                        connected_companions: vec![bot_name.to_owned()],
                    },
                );
            }
        }
        Ok(())
    }

    pub fn get_live_context(&self, _actor: &Actor, session_id: &str) -> Result<LiveSessionContext> {
        let lock = self.sessions.read().map_err(|_| Error::External("lock poisoned".into()))?;
        lock.get(session_id).cloned().ok_or_else(|| Error::NotFound)
    }

    pub fn send_reaction(
        &self,
        _actor: &Actor,
        session_id: &str,
        bot_name: &str,
        timestamp_ms: i64,
        commentary: &str,
        sentiment: Option<&str>,
    ) -> Result<CompanionReaction> {
        let reaction = CompanionReaction {
            session_id: session_id.to_owned(),
            bot_name: bot_name.to_owned(),
            timestamp_ms,
            commentary: commentary.to_owned(),
            sentiment: sentiment.map(|s| s.to_owned()),
            created_at: now_ms(),
        };

        let _ = self.reactions_tx.send(reaction.clone());
        Ok(reaction)
    }

    pub fn subscribe_reactions(&self) -> broadcast::Receiver<CompanionReaction> {
        self.reactions_tx.subscribe()
    }
}

impl Default for CompanionService {
    fn default() -> Self {
        Self::new()
    }
}
