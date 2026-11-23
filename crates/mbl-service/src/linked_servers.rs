//! Linked MBL servers: see and control another server's players, and start titles from its
//! library there. Each link is the other server's address and an API token made on it (scopes
//! `read` + `playback`); every call goes to its MCP endpoint, so the other server's own
//! permissions, network rules and audit log apply. Nothing of the other server is copied here.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use mbl_core::{Actor, Error, Result};
use mbl_db::{Db, repo::settings::SettingsRepo};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use utoipa::ToSchema;

const SETTING: &str = "linked_servers";
const MAX_LINKS: usize = 16;
const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// Calls a tool on another MBL's MCP endpoint and returns its structured result. Lives in
/// `mbl-mcp`, which depends on this crate, so the server plugs it in at start.
#[async_trait::async_trait]
pub trait LinkedServerClient: Send + Sync {
    async fn call(&self, url: &str, token: &str, tool: &str, args: Value) -> Result<Value>;
}

/// A stored link, with its token (never sent to browsers).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredLink {
    id: String,
    name: String,
    url: String,
    token: String,
    #[serde(default)]
    shared: bool,
}

/// A link as administrators see it.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedServer {
    pub id: String,
    pub name: String,
    /// The other server's web address, e.g. `http://192.168.1.20:8420`.
    pub url: String,
    /// Everyone on this server may use its players, not only administrators.
    pub shared: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AddLinkedServerRequest {
    pub name: String,
    pub url: String,
    /// API token made on the other server (AI agents → new token, scopes read + playback).
    pub token: String,
    #[serde(default)]
    pub shared: bool,
}

/// A player connected to a linked server.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedPlayer {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub state: String,
    pub now_playing_item_id: Option<i64>,
    /// Title on the other server, when something plays.
    pub now_playing_title: Option<String>,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub volume: u32,
}

/// A linked server with its players, or why it could not be reached.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedServerView {
    pub id: String,
    pub name: String,
    pub url: String,
    pub players: Vec<LinkedPlayer>,
    pub error: Option<String>,
}

/// What to do on a linked server's player.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedPlayerCommand {
    /// `pause`, `resume`, `stop`, `seek` or `volume`.
    pub action: String,
    #[serde(default)]
    pub position_ms: Option<i64>,
    #[serde(default)]
    pub volume: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedPlayRequest {
    pub item_id: i64,
    #[serde(default)]
    pub start_ms: Option<i64>,
}

/// A title in a linked server's library.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkedItem {
    pub id: i64,
    pub title: String,
    pub kind: String,
    pub year: Option<i64>,
}

#[derive(Clone)]
pub struct LinkedServerService {
    db: Db,
    client: Arc<RwLock<Option<Arc<dyn LinkedServerClient>>>>,
}

impl std::fmt::Debug for LinkedServerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkedServerService").finish_non_exhaustive()
    }
}

impl LinkedServerService {
    pub fn new(db: Db) -> Self {
        Self { db, client: Arc::new(RwLock::new(None)) }
    }

    /// Plugs in the MCP client; shared by every clone of the service.
    pub fn set_client(&self, client: Arc<dyn LinkedServerClient>) {
        if let Ok(mut slot) = self.client.write() {
            *slot = Some(client);
        }
    }

    fn client(&self) -> Result<Arc<dyn LinkedServerClient>> {
        self.client
            .read()
            .ok()
            .and_then(|slot| slot.clone())
            .ok_or_else(|| Error::Unavailable("linked servers are not available".into()))
    }

    async fn stored(&self) -> Result<Vec<StoredLink>> {
        let raw = SettingsRepo::new(self.db.clone()).get(SETTING).await?;
        Ok(raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default())
    }

    async fn save(&self, links: &[StoredLink]) -> Result<()> {
        let raw = serde_json::to_string(links).map_err(Error::internal)?;
        SettingsRepo::new(self.db.clone()).set(SETTING, &raw).await
    }

    /// The links someone may use: all for administrators, shared ones for other accounts,
    /// none for managed profiles.
    async fn usable(&self, actor: &Actor) -> Result<Vec<StoredLink>> {
        if actor.is_managed {
            return Ok(Vec::new());
        }
        let links = self.stored().await?;
        Ok(links.into_iter().filter(|l| actor.is_admin || l.shared).collect())
    }

    async fn link(&self, actor: &Actor, id: &str) -> Result<StoredLink> {
        self.usable(actor).await?.into_iter().find(|l| l.id == id).ok_or(Error::NotFound)
    }

    async fn call(&self, link: &StoredLink, tool: &str, args: Value) -> Result<Value> {
        let client = self.client()?;
        tokio::time::timeout(CALL_TIMEOUT, client.call(&link.url, &link.token, tool, args))
            .await
            .map_err(|_| Error::Unavailable(format!("{} did not answer in time", link.name)))?
    }

    /// Links, for the settings page (administrators).
    pub async fn list(&self, actor: &Actor) -> Result<Vec<LinkedServer>> {
        actor.require_admin()?;
        Ok(self.stored().await?.into_iter().map(public).collect())
    }

    /// Adds a link after checking that the other server answers with this token.
    pub async fn add(&self, actor: &Actor, req: AddLinkedServerRequest) -> Result<LinkedServer> {
        actor.require_admin()?;
        let name = req.name.trim().chars().take(60).collect::<String>();
        if name.is_empty() {
            return Err(Error::Validation("give the server a name".into()));
        }
        let url = normalize_url(&req.url)?;
        let token = req.token.trim().to_owned();
        if token.is_empty() || token.len() > 512 {
            return Err(Error::Validation("paste the token made on the other server".into()));
        }
        let mut links = self.stored().await?;
        if links.len() >= MAX_LINKS {
            return Err(Error::Validation(format!("at most {MAX_LINKS} linked servers")));
        }
        if links.iter().any(|l| l.url == url) {
            return Err(Error::Conflict("that server is already linked".into()));
        }
        let link = StoredLink { id: new_id(), name, url, token, shared: req.shared };
        self.call(&link, "list_players", json!({})).await.map_err(|e| {
            Error::Validation(format!("{} did not accept the link: {}", link.url, plain(&e)))
        })?;
        links.push(link.clone());
        self.save(&links).await?;
        tracing::info!(server = %link.url, "Linked another MBL server");
        Ok(public(link))
    }

    pub async fn remove(&self, actor: &Actor, id: &str) -> Result<()> {
        actor.require_admin()?;
        let mut links = self.stored().await?;
        let before = links.len();
        links.retain(|l| l.id != id);
        if links.len() == before {
            return Err(Error::NotFound);
        }
        self.save(&links).await
    }

    /// The linked servers someone may use, without contacting them (for the server menu).
    pub async fn names(&self, actor: &Actor) -> Result<Vec<LinkedServer>> {
        Ok(self.usable(actor).await?.into_iter().map(public).collect())
    }

    /// Every usable linked server with its players right now.
    pub async fn overview(&self, actor: &Actor) -> Result<Vec<LinkedServerView>> {
        let links = self.usable(actor).await?;
        let views = futures_util::future::join_all(links.into_iter().map(|link| async move {
            match self.players(&link).await {
                Ok(players) => LinkedServerView {
                    id: link.id,
                    name: link.name,
                    url: link.url,
                    players,
                    error: None,
                },
                Err(e) => LinkedServerView {
                    id: link.id,
                    name: link.name,
                    url: link.url,
                    players: Vec::new(),
                    error: Some(plain(&e)),
                },
            }
        }))
        .await;
        Ok(views)
    }

    async fn players(&self, link: &StoredLink) -> Result<Vec<LinkedPlayer>> {
        let value = self.call(link, "list_players", json!({})).await?;
        let mut players: Vec<LinkedPlayer> = value
            .get("players")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(parse_player).collect())
            .unwrap_or_default();
        for player in &mut players {
            if let Some(item) = player.now_playing_item_id
                && let Ok(detail) = self.call(link, "get_item", json!({ "item_id": item })).await
            {
                player.now_playing_title =
                    detail.get("title").and_then(Value::as_str).map(str::to_owned);
            }
        }
        Ok(players)
    }

    /// Pause, resume, stop, seek or change the volume on a linked server's player.
    pub async fn command(
        &self,
        actor: &Actor,
        id: &str,
        player_id: &str,
        cmd: LinkedPlayerCommand,
    ) -> Result<()> {
        let link = self.link(actor, id).await?;
        let mut args = json!({ "player_id": player_id, "action": cmd.action });
        match cmd.action.as_str() {
            "pause" | "resume" | "stop" => {}
            "seek" => {
                let ms = cmd
                    .position_ms
                    .filter(|ms| *ms >= 0)
                    .ok_or_else(|| Error::Validation("seek needs positionMs".into()))?;
                args["position_ms"] = json!(ms);
            }
            "volume" => {
                let v = cmd
                    .volume
                    .filter(|v| *v <= 100)
                    .ok_or_else(|| Error::Validation("volume needs 0–100".into()))?;
                args["volume"] = json!(v);
            }
            _ => return Err(Error::Validation("unknown action".into())),
        }
        self.call(&link, "control_player", args).await.map(|_| ())
    }

    /// What companions say about what a linked server's player plays (its `read_companion_chat`).
    pub async fn player_comments(
        &self,
        actor: &Actor,
        id: &str,
        player_id: &str,
        after_id: Option<u64>,
    ) -> Result<crate::PlayerComments> {
        let link = self.link(actor, id).await?;
        let mut args = json!({ "player_id": player_id });
        if let Some(after) = after_id {
            args["after_id"] = json!(after);
        }
        let value = self.call(&link, "read_companion_chat", args).await?;
        serde_json::from_value(value.get("playerComments").cloned().unwrap_or(Value::Null))
            .map_err(|_| Error::External(format!("{} sent no comments", link.name)))
    }

    /// Search a linked server's library (what its token may see).
    pub async fn search(&self, actor: &Actor, id: &str, query: &str) -> Result<Vec<LinkedItem>> {
        let link = self.link(actor, id).await?;
        let query: String = query.trim().chars().take(100).collect();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let value = self.call(&link, "search_media", json!({ "query": query })).await?;
        let items = value
            .get("groups")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|g| g.get("items").and_then(Value::as_array))
            .flatten()
            .filter_map(|item| {
                Some(LinkedItem {
                    id: item.get("id")?.as_i64()?,
                    title: item.get("title")?.as_str()?.to_owned(),
                    kind: item.get("kind").and_then(Value::as_str).unwrap_or("").to_owned(),
                    year: item.get("year").and_then(Value::as_i64),
                })
            })
            .take(30)
            .collect();
        Ok(items)
    }

    /// Start a title from the linked server's library on one of its players. The player
    /// streams from that server, so its permissions, transcoding and history apply there.
    pub async fn play(
        &self,
        actor: &Actor,
        id: &str,
        player_id: &str,
        req: LinkedPlayRequest,
    ) -> Result<()> {
        let link = self.link(actor, id).await?;
        let mut args = json!({ "player_id": player_id, "item_id": req.item_id });
        if let Some(ms) = req.start_ms.filter(|ms| *ms >= 0) {
            args["start_ms"] = json!(ms);
        }
        self.call(&link, "play_on_player", args).await.map(|_| ())
    }
}

fn public(link: StoredLink) -> LinkedServer {
    LinkedServer { id: link.id, name: link.name, url: link.url, shared: link.shared }
}

fn parse_player(v: &Value) -> Option<LinkedPlayer> {
    // `list_players` sends the player as the REST API does (camelCase); accept snake_case too.
    let field = |camel: &str, snake: &str| v.get(camel).or_else(|| v.get(snake));
    Some(LinkedPlayer {
        id: v.get("id")?.as_str()?.to_owned(),
        name: v.get("name").and_then(Value::as_str).unwrap_or("Player").chars().take(80).collect(),
        device_type: field("deviceType", "device_type")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        state: v.get("state").and_then(Value::as_str).unwrap_or("idle").to_owned(),
        now_playing_item_id: field("nowPlayingItemId", "now_playing_item_id")
            .and_then(Value::as_i64),
        now_playing_title: None,
        position_ms: field("positionMs", "position_ms").and_then(Value::as_i64).unwrap_or(0),
        duration_ms: field("durationMs", "duration_ms").and_then(Value::as_i64).unwrap_or(0),
        volume: v.get("volume").and_then(Value::as_u64).map_or(100, |v| v.min(100) as u32),
    })
}

/// `http(s)://host[:port]` without a path or a trailing slash.
fn normalize_url(raw: &str) -> Result<String> {
    let url = reqwest::Url::parse(raw.trim())
        .map_err(|_| Error::Validation("enter the address like http://192.168.1.20:8420".into()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(Error::Validation("the address must start with http:// or https://".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Validation("leave sign-in details out of the address".into()));
    }
    let mut origin = url.origin().ascii_serialization();
    let path = url.path().trim_end_matches('/').trim_end_matches("/mcp");
    if !path.is_empty() && path != "/" {
        origin.push_str(path);
    }
    Ok(origin)
}

fn new_id() -> String {
    let mut bytes = [0u8; 4];
    let _ = getrandom::fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An error without its `validation:`-style prefix, for people.
fn plain(e: &Error) -> String {
    let text = e.to_string();
    match text.split_once(": ") {
        Some((head, rest)) if !head.contains(' ') => rest.to_owned(),
        _ => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_normalized_to_the_server_root() {
        assert_eq!(normalize_url("http://192.168.1.20:8420/").unwrap(), "http://192.168.1.20:8420");
        assert_eq!(normalize_url("https://mbl.example/mcp").unwrap(), "https://mbl.example");
        assert_eq!(normalize_url(" https://h.example:8443 ").unwrap(), "https://h.example:8443");
        assert!(normalize_url("ftp://h").is_err());
        assert!(normalize_url("http://user:pw@h").is_err());
        assert!(normalize_url("not a url").is_err());
    }

    #[test]
    fn players_are_read_from_list_players() {
        let p = parse_player(&json!({
            "id": "p1", "name": "Living room", "device_type": "chromecast", "state": "playing",
            "now_playing_item_id": 7, "position_ms": 1000, "duration_ms": 9000, "volume": 300
        }))
        .unwrap();
        assert_eq!(p.name, "Living room");
        assert_eq!(p.now_playing_item_id, Some(7));
        assert_eq!(p.volume, 100);
        assert!(parse_player(&json!({ "name": "no id" })).is_none());
        // What list_players actually sends.
        let p = parse_player(&json!({
            "id": "p2", "name": "TV", "deviceType": "web", "state": "playing",
            "nowPlayingItemId": 9, "positionMs": 5, "durationMs": 10, "volume": 40
        }))
        .unwrap();
        assert_eq!((p.now_playing_item_id, p.position_ms, p.duration_ms), (Some(9), 5, 10));
        assert_eq!(p.device_type, "web");
    }
}
