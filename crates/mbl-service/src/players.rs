//! Player Registry and Companion Remote Control protocol (M7.10, M7.13).

use mbl_core::{Actor, Error, ItemId, Result, time::now_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    pub id: String,
    pub name: String,
    pub device_type: String, // "web", "tv", "chromecast", "dlna"
    pub state: String,       // "idle", "playing", "paused"
    pub now_playing_item_id: Option<ItemId>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub volume: u32, // 0..100
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "action", content = "params", rename_all = "camelCase")]
pub enum PlayerCommand {
    Play { item_id: ItemId, start_ms: Option<i64> },
    Pause,
    Resume,
    Stop { message: Option<String> },
    Seek { position_ms: i64 },
    SetVolume { volume: u32 },
    SetTracks { audio_stream_index: Option<i64>, subtitle_stream_index: Option<i64> },
}

#[derive(Debug, Clone)]
pub struct PlayerRegistry {
    players: Arc<RwLock<HashMap<String, PlayerInfo>>>,
    command_tx: broadcast::Sender<(String, PlayerCommand)>,
}

impl PlayerRegistry {
    pub fn new() -> Self {
        let (command_tx, _) = broadcast::channel(128);
        Self { players: Arc::new(RwLock::new(HashMap::new())), command_tx }
    }

    pub fn register_or_update(&self, mut player: PlayerInfo) {
        player.last_seen_at = now_ms();
        if let Ok(mut lock) = self.players.write() {
            lock.insert(player.id.clone(), player);
        }
    }

    pub fn unregister(&self, player_id: &str) {
        if let Ok(mut lock) = self.players.write() {
            lock.remove(player_id);
        }
    }

    pub fn list_players(&self, _actor: &Actor) -> Result<Vec<PlayerInfo>> {
        let now = now_ms();
        if let Ok(mut lock) = self.players.write() {
            // Prune players older than 60 seconds
            lock.retain(|_, p| (now - p.last_seen_at) < 60_000);
            let mut list: Vec<PlayerInfo> = lock.values().cloned().collect();
            list.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(list)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn get_player(&self, player_id: &str) -> Option<PlayerInfo> {
        self.players.read().ok()?.get(player_id).cloned()
    }

    pub fn send_command(
        &self,
        actor: &Actor,
        player_id: &str,
        command: PlayerCommand,
    ) -> Result<()> {
        // Any authenticated user can control players on their LAN/tailnet
        if actor.user_id.0 == 0 {
            return Err(Error::Unauthorized);
        }

        let exists = self.players.read().map(|p| p.contains_key(player_id)).unwrap_or(false);

        if !exists {
            return Err(Error::NotFound);
        }

        // If stop with message, check admin permission if target player belongs to someone else
        if let PlayerCommand::Stop { message: Some(_) } = &command {
            actor.require_admin()?;
        }

        let _ = self.command_tx.send((player_id.to_owned(), command));
        Ok(())
    }

    pub fn subscribe_commands(&self) -> broadcast::Receiver<(String, PlayerCommand)> {
        self.command_tx.subscribe()
    }
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
