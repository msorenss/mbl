//! Remote Library Node service (M8.10, M8.11).
//! Handles pairing codes, node registration, delta catalog sync, and smart handover.

use std::net::IpAddr;
use std::sync::Arc;

use mbl_auth::hash_token;
use mbl_core::{
    Actor, Error, ItemId, NodeCatalogSyncRequest, NodeCatalogSyncResponse, NodeHandoverResponse,
    NodeId, NodePairRequest, NodePairResponse, Result, time::now_ms,
};
use mbl_db::Db;
use mbl_db::repo::m8::{self, RemoteNodeLibraryRow, RemoteNodeRow};

#[derive(Debug, Clone)]
pub struct RemoteNodeService {
    db: Arc<Db>,
}

impl RemoteNodeService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Generate a 6-digit node pairing code valid for 15 minutes.
    pub async fn generate_pairing_code(&self, actor: &Actor) -> Result<String> {
        actor.require_admin()?;
        let mut rnd_bytes = [0u8; 4];
        getrandom::fill(&mut rnd_bytes).map_err(|e| {
            Error::Internal(format!("failed to generate random pairing code: {e}").into())
        })?;
        let random_num = u32::from_ne_bytes(rnd_bytes) % 1_000_000;
        let code = format!("N-{random_num:06}");
        let expires_at = now_ms() + (15 * 60 * 1000);

        m8::create_node_pairing_code(&self.db, &code, actor.user_id, expires_at).await?;
        Ok(code)
    }

    /// Pair a new remote library node using a valid pairing code.
    pub async fn pair_node(&self, req: NodePairRequest) -> Result<NodePairResponse> {
        let maybe_user = m8::consume_node_pairing_code(&self.db, &req.pairing_code).await?;
        if maybe_user.is_none() {
            return Err(Error::Unauthorized);
        }

        let mut token_bytes = [0u8; 24];
        getrandom::fill(&mut token_bytes)
            .map_err(|e| Error::Internal(format!("failed to generate random token: {e}").into()))?;
        let hex_token: String = token_bytes.iter().map(|b| format!("{b:02x}")).collect();
        let auth_token = format!("mbl_nod_{hex_token}");
        let token_hash = hash_token(&auth_token);

        let node_id =
            m8::register_node(&self.db, &req.name, &req.base_url, &token_hash, &req.capabilities)
                .await?;

        Ok(NodePairResponse {
            node_id,
            auth_token,
            server_version: env!("CARGO_PKG_VERSION").into(),
            server_name: "MBL Server".into(),
        })
    }

    /// Authenticate a remote node request via bearer auth header.
    pub async fn authenticate(&self, header_value: &str) -> Result<RemoteNodeRow> {
        let token = header_value.strip_prefix("Bearer ").unwrap_or(header_value).trim();
        let token_hash = hash_token(token);

        let node = m8::get_node_by_token_hash(&self.db, &token_hash).await?;
        node.ok_or(Error::Unauthorized)
    }

    pub async fn list_nodes(&self, actor: &Actor) -> Result<Vec<RemoteNodeRow>> {
        actor.require_admin()?;
        m8::list_nodes(&self.db).await
    }

    pub async fn get_node(&self, actor: &Actor, id: NodeId) -> Result<RemoteNodeRow> {
        actor.require_admin()?;
        let node = m8::get_node(&self.db, id).await?;
        node.ok_or(Error::NotFound)
    }

    pub async fn delete_node(&self, actor: &Actor, id: NodeId) -> Result<bool> {
        actor.require_admin()?;
        m8::delete_node(&self.db, id).await
    }

    /// Sync the delta catalog of libraries and items from a storage node.
    pub async fn sync_catalog(
        &self,
        node_id: NodeId,
        req: NodeCatalogSyncRequest,
    ) -> Result<NodeCatalogSyncResponse> {
        let (synced_libraries, total_items) =
            m8::sync_node_libraries(&self.db, node_id, &req.libraries).await?;
        m8::update_node_heartbeat(&self.db, node_id, "online", now_ms()).await?;

        Ok(NodeCatalogSyncResponse { synced_libraries, total_items })
    }

    pub async fn list_node_libraries(
        &self,
        actor: &Actor,
        node_id: NodeId,
    ) -> Result<Vec<RemoteNodeLibraryRow>> {
        actor.require_admin()?;
        m8::list_node_libraries(&self.db, node_id).await
    }

    pub async fn list_all_node_libraries(
        &self,
        actor: &Actor,
    ) -> Result<Vec<RemoteNodeLibraryRow>> {
        actor.require_admin()?;
        m8::list_all_node_libraries(&self.db).await
    }

    /// Evaluate smart handover: return direct LAN play URL if client is on LAN, or signal relay.
    pub async fn get_handover(
        &self,
        node_id: NodeId,
        item_id: ItemId,
        client_ip: Option<IpAddr>,
    ) -> Result<NodeHandoverResponse> {
        let node = m8::get_node(&self.db, node_id).await?.ok_or(Error::NotFound)?;

        let is_lan = client_ip.is_some_and(|ip| match ip {
            IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local(),
            IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
        });

        if is_lan {
            let base = node.base_url.trim_end_matches('/');
            let direct_play_url = format!("{base}/api/v1/stream/direct/{}", item_id.0);
            Ok(NodeHandoverResponse {
                direct_play_url: Some(direct_play_url),
                requires_relay: false,
                reason: "Client is on local network; direct play handover granted".into(),
            })
        } else {
            Ok(NodeHandoverResponse {
                direct_play_url: None,
                requires_relay: true,
                reason: "Client is remote (WAN); media relay or transcode proxy required".into(),
            })
        }
    }
}
