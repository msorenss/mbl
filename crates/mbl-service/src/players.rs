//! Player Registry and Companion Remote Control protocol (M7.10, M7.13).

use mbl_core::{Actor, Error, ItemId, Result, SessionId, UserId, time::now_ms};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
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
    #[serde(default)]
    pub last_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "action",
    content = "params",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlayerCommand {
    Play { item_id: ItemId, start_ms: Option<i64> },
    Pause,
    Resume,
    Stop { message: Option<String> },
    Seek { position_ms: i64 },
    SetVolume { volume: u32 },
    SetTracks { audio_stream_index: Option<i64>, subtitle_stream_index: Option<i64> },
}

#[derive(Debug)]
struct RegisteredPlayer {
    info: PlayerInfo,
    owner: Option<(UserId, SessionId)>,
    connection: Option<(u64, mpsc::Sender<PlayerCommand>)>,
}

#[derive(Debug, Clone)]
pub struct PlayerRegistry {
    players: Arc<RwLock<HashMap<String, RegisteredPlayer>>>,
    sequence: Arc<AtomicU64>,
}

/// A transport lease. Dropping an old socket cannot remove a newer connection.
#[derive(Debug)]
pub struct PlayerConnection {
    pub commands: mpsc::Receiver<PlayerCommand>,
    registry: PlayerRegistry,
    id: String,
    generation: u64,
}

impl Drop for PlayerConnection {
    fn drop(&mut self) {
        if let Ok(mut players) = self.registry.players.write()
            && players
                .get(&self.id)
                .is_some_and(|p| p.connection.as_ref().is_some_and(|(g, _)| *g == self.generation))
        {
            players.remove(&self.id);
        }
    }
}

impl PlayerRegistry {
    pub fn new() -> Self {
        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            sequence: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Trusted in-process registration. Such devices are visible only to administrators.
    pub fn register_or_update(&self, mut info: PlayerInfo) {
        info.last_seen_at = now_ms();
        if let Ok(mut players) = self.players.write() {
            players
                .entry(info.id.clone())
                .and_modify(|p| p.info = info.clone())
                .or_insert(RegisteredPlayer { info, owner: None, connection: None });
        }
    }

    pub fn register(&self, actor: &Actor, session: SessionId, mut info: PlayerInfo) -> Result<()> {
        if info.id.is_empty()
            || info.id.len() > 128
            || info.name.trim().is_empty()
            || info.name.len() > 200
            || info.volume > 100
            || info.position_ms < 0
            || info.duration_ms < 0
        {
            return Err(Error::Validation("invalid player identity or state".into()));
        }
        let mut players = self
            .players
            .write()
            .map_err(|_| Error::Unavailable("player registry unavailable".into()))?;
        players.retain(|_, p| now_ms().saturating_sub(p.info.last_seen_at) < 60_000);
        if let Some(player) = players.get_mut(&info.id) {
            if player.owner != Some((actor.user_id, session)) {
                return Err(Error::NotFound);
            }
            info.last_seen_at = now_ms();
            player.info = info;
        } else {
            if players
                .values()
                .filter(|p| p.owner.is_some_and(|(user, _)| user == actor.user_id))
                .count()
                >= 32
            {
                return Err(Error::Conflict("maximum active players reached".into()));
            }
            info.last_seen_at = now_ms();
            players.insert(
                info.id.clone(),
                RegisteredPlayer { info, owner: Some((actor.user_id, session)), connection: None },
            );
        }
        Ok(())
    }

    fn visible(actor: &Actor, player: &RegisteredPlayer) -> bool {
        actor.is_admin || player.owner.is_some_and(|(user, _)| user == actor.user_id)
    }

    pub fn unregister_owned(&self, actor: &Actor, session: SessionId, id: &str) -> Result<()> {
        let mut players = self
            .players
            .write()
            .map_err(|_| Error::Unavailable("player registry unavailable".into()))?;
        if let Some(player) = players.get(id) {
            if !actor.is_admin && player.owner != Some((actor.user_id, session)) {
                return Err(Error::NotFound);
            }
            players.remove(id);
        }
        Ok(())
    }

    pub fn list_players(&self, actor: &Actor) -> Result<Vec<PlayerInfo>> {
        let mut players = self
            .players
            .write()
            .map_err(|_| Error::Unavailable("player registry unavailable".into()))?;
        players.retain(|_, p| now_ms().saturating_sub(p.info.last_seen_at) < 60_000);
        let mut list: Vec<_> =
            players.values().filter(|p| Self::visible(actor, p)).map(|p| p.info.clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(list)
    }

    pub fn get_player(&self, player_id: &str) -> Option<PlayerInfo> {
        self.players.read().ok()?.get(player_id).map(|p| p.info.clone())
    }

    pub fn connect(&self, actor: &Actor, session: SessionId, id: &str) -> Result<PlayerConnection> {
        let mut players = self
            .players
            .write()
            .map_err(|_| Error::Unavailable("player registry unavailable".into()))?;
        let player = players.get_mut(id).ok_or(Error::NotFound)?;
        if player.owner != Some((actor.user_id, session))
            && !(actor.is_admin && player.owner.is_none())
        {
            return Err(Error::NotFound);
        }
        if player.connection.as_ref().is_some_and(|(_, tx)| !tx.is_closed()) {
            return Err(Error::Conflict("player already connected".into()));
        }
        let (tx, commands) = mpsc::channel(32);
        let generation = self.sequence.fetch_add(1, Ordering::Relaxed);
        player.connection = Some((generation, tx));
        Ok(PlayerConnection { commands, registry: self.clone(), id: id.to_owned(), generation })
    }

    pub fn send_command(&self, actor: &Actor, id: &str, command: PlayerCommand) -> Result<()> {
        if actor.user_id.0 == 0 {
            return Err(Error::Unauthorized);
        }
        let players = self
            .players
            .read()
            .map_err(|_| Error::Unavailable("player registry unavailable".into()))?;
        let player = players.get(id).filter(|p| Self::visible(actor, p)).ok_or(Error::NotFound)?;
        if now_ms().saturating_sub(player.info.last_seen_at) >= 60_000 {
            return Err(Error::NotFound);
        }
        if let PlayerCommand::Stop { message: Some(_) } = &command {
            actor.require_admin()?;
        }
        match &command {
            PlayerCommand::Seek { position_ms } if *position_ms < 0 => {
                return Err(Error::Validation("seek position must be non-negative".into()));
            }
            PlayerCommand::SetVolume { volume } if *volume > 100 => {
                return Err(Error::Validation("volume must be 0–100".into()));
            }
            _ => {}
        }
        let (_, tx) = player
            .connection
            .as_ref()
            .ok_or_else(|| Error::Unavailable("player has no command connection".into()))?;
        tx.try_send(command).map_err(|_| {
            Error::Unavailable("player command connection is unavailable or busy".into())
        })
    }
}

impl Default for PlayerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn info() -> PlayerInfo {
        PlayerInfo {
            id: "device".into(),
            name: "TV".into(),
            device_type: "web".into(),
            state: "playing".into(),
            now_playing_item_id: None,
            position_ms: 0,
            duration_ms: 1000,
            volume: 50,
            last_seen_at: 0,
        }
    }
    #[tokio::test]
    async fn commands_require_the_owning_session_and_live_transport() {
        let registry = PlayerRegistry::new();
        let alice = Actor::user(UserId(1));
        let bob = Actor::user(UserId(2));
        registry.register(&alice, SessionId(1), info()).unwrap();
        assert!(registry.list_players(&bob).unwrap().is_empty());
        assert!(matches!(registry.register(&bob, SessionId(2), info()), Err(Error::NotFound)));
        assert!(matches!(registry.connect(&alice, SessionId(2), "device"), Err(Error::NotFound)));
        assert!(matches!(
            registry.send_command(&alice, "device", PlayerCommand::Pause),
            Err(Error::Unavailable(_))
        ));
        let mut connection = registry.connect(&alice, SessionId(1), "device").unwrap();
        assert!(registry.connect(&alice, SessionId(1), "device").is_err());
        assert!(matches!(
            registry.send_command(&bob, "device", PlayerCommand::Pause),
            Err(Error::NotFound)
        ));
        registry.send_command(&alice, "device", PlayerCommand::Seek { position_ms: 123 }).unwrap();
        let command = connection.commands.recv().await.unwrap();
        assert_eq!(
            serde_json::to_value(command).unwrap(),
            serde_json::json!({"action":"seek", "params":{"positionMs":123}})
        );
        assert!(registry.unregister_owned(&alice, SessionId(2), "device").is_err());
        drop(connection);
        assert!(registry.list_players(&alice).unwrap().is_empty());
    }
}
