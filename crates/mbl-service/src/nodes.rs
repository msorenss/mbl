//! Remote Library Node service (M8.10, M8.11).
//! Handles pairing codes, node registration, delta catalog sync, and smart handover.

use std::sync::Arc;

use mbl_auth::hash_token;
use mbl_core::{
    Actor, Error, LibraryKind, NodeCatalogSyncRequest, NodeCatalogSyncResponse, NodeFileRef,
    NodeHandoverResponse, NodeId, NodePairRequest, NodePairResponse, NodeProbeUpload,
    NodeProbeUploadResponse, Result, node_library_root, parse_node_path, time::now_ms,
};
use mbl_db::Db;
use mbl_db::repo::libraries::LibrariesRepo;
use mbl_db::repo::m8::{self, RemoteNodeLibraryRow, RemoteNodeRow};
use mbl_scanner::{RemoteFile, Scanner};

/// Files the server asks a node to probe per sync round.
const PROBE_BATCH: usize = 200;
/// Probed files accepted per upload.
pub const MAX_PROBE_UPLOAD: usize = 100;
/// Files per node library listing.
pub const MAX_LIBRARY_FILES: usize = 250_000;

#[derive(Clone)]
pub struct RemoteNodeService {
    db: Arc<Db>,
    scanner: Option<Scanner>,
}

impl std::fmt::Debug for RemoteNodeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteNodeService").finish_non_exhaustive()
    }
}

/// Signed URL for one file on a storage node, valid for `ttl_secs`.
pub async fn signed_node_file_url(db: &Db, node_path: &str, ttl_secs: i64) -> Result<String> {
    let (node_id, library, rel) = parse_node_path(node_path).ok_or(Error::NotFound)?;
    let node = m8::get_node(db, node_id).await?.ok_or(Error::NotFound)?;
    let secret = m8::node_stream_secret(db, node_id).await?.ok_or_else(|| {
        Error::Unavailable(format!(
            "storage node '{}' was paired with an older version; pair it again",
            node.name
        ))
    })?;
    let expires = now_ms() + ttl_secs * 1000;
    let sig = mbl_auth::sign_node_file(secret.as_bytes(), library, rel, expires);
    let mut url = reqwest::Url::parse(&node.base_url)
        .map_err(|_| Error::Unavailable("storage node has an invalid address".into()))?;
    url.path_segments_mut()
        .map_err(|()| Error::Unavailable("storage node has an invalid address".into()))?
        .pop_if_empty()
        .push("files")
        .push(&library.to_string())
        .extend(rel.split('/'));
    url.query_pairs_mut().append_pair("exp", &expires.to_string()).append_pair("sig", &sig);
    Ok(url.into())
}

/// Relay a node file to the client (Range, HEAD), so remote and LAN clients use one URL.
pub async fn relay_node_file(
    db: &Db,
    node_path: &str,
    range: Option<&str>,
    head: bool,
) -> Result<axum::response::Response> {
    use axum::http::{HeaderValue, header};
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
    });
    let url = signed_node_file_url(db, node_path, 300).await?;
    let mut request = if head { client.head(&url) } else { client.get(&url) };
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    let upstream = request
        .send()
        .await
        .map_err(|e| Error::Unavailable(format!("storage node is unreachable: {e}")))?;
    let status = upstream.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::NotFound);
    }
    if !(status.is_success() || status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE) {
        return Err(Error::Unavailable(format!("storage node answered {status}")));
    }
    let mut response = axum::response::Response::builder()
        .status(axum::http::StatusCode::from_u16(status.as_u16()).map_err(Error::internal)?);
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::CONTENT_RANGE,
        header::ACCEPT_RANGES,
        header::LAST_MODIFIED,
        header::ETAG,
    ] {
        if let Some(value) = upstream
            .headers()
            .get(name.as_str())
            .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok())
        {
            response = response.header(name, value);
        }
    }
    let body = if head {
        axum::body::Body::empty()
    } else {
        axum::body::Body::from_stream(upstream.bytes_stream())
    };
    response.body(body).map_err(Error::internal)
}

fn valid_rel_path(rel: &str) -> bool {
    rel.len() <= 4096 && parse_node_path(&format!("mblnode://0/0/{rel}")).is_some()
}

impl RemoteNodeService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db, scanner: None }
    }

    /// Catalog import needs the scanner's write path.
    pub fn with_scanner(mut self, scanner: Scanner) -> Self {
        self.scanner = Some(scanner);
        self
    }

    fn scanner(&self) -> Result<&Scanner> {
        self.scanner
            .as_ref()
            .ok_or_else(|| Error::Unavailable("catalog import is not available".into()))
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
        let base_url = req.base_url.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(base_url)
            .map_err(|_| Error::Validation("baseUrl must be an http(s) URL".into()))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
            return Err(Error::Validation("baseUrl must be an http(s) URL".into()));
        }
        let mut secret_bytes = [0u8; 32];
        getrandom::fill(&mut secret_bytes)
            .map_err(|e| Error::Internal(format!("failed to generate node secret: {e}").into()))?;
        let stream_secret: String = secret_bytes.iter().map(|b| format!("{b:02x}")).collect();

        let node_id = m8::register_node(
            &self.db,
            &req.name,
            base_url,
            &token_hash,
            &req.capabilities,
            &stream_secret,
        )
        .await?;

        Ok(NodePairResponse {
            node_id,
            auth_token,
            server_version: env!("CARGO_PKG_VERSION").into(),
            server_name: "MBL Server".into(),
            stream_secret: Some(stream_secret),
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
        let node = m8::get_node(&self.db, node_id).await?.ok_or(Error::NotFound)?;
        let (synced_libraries, total_items) =
            m8::sync_node_libraries(&self.db, node_id, &req.libraries).await?;
        m8::update_node_heartbeat(&self.db, node_id, "online", now_ms()).await?;

        let mut need_probe = Vec::new();
        for lib in &req.libraries {
            let Some(files) = &lib.files else { continue };
            if files.len() > MAX_LIBRARY_FILES {
                return Err(Error::Validation(format!(
                    "library '{}' lists more than {MAX_LIBRARY_FILES} files",
                    lib.name
                )));
            }
            let kind: LibraryKind = serde_json::from_value(serde_json::Value::String(
                lib.kind.clone(),
            ))
            .map_err(|_| Error::Validation(format!("unknown library kind '{}'", lib.kind)))?;
            let root = node_library_root(node_id, lib.remote_library_id);
            let library_id = match m8::linked_library(&self.db, node_id, lib.remote_library_id)
                .await?
            {
                Some(id) => id,
                None => {
                    let name = format!("{} ({})", lib.name.trim(), node.name);
                    let id = LibrariesRepo::new((*self.db).clone())
                        .insert(&name, kind, std::slice::from_ref(&root), now_ms())
                        .await?;
                    m8::link_library(&self.db, node_id, lib.remote_library_id, id).await?;
                    tracing::info!(node_id = node_id.0, library_id = id.0, %name, "created library for storage node folder");
                    id
                }
            };
            let listing: Vec<RemoteFile> = files
                .iter()
                .filter(|f| valid_rel_path(&f.path))
                .map(|f| RemoteFile { rel: f.path.clone(), size: f.size, mtime: f.mtime })
                .collect();
            let budget = PROBE_BATCH.saturating_sub(need_probe.len());
            let outcome =
                self.scanner()?.reconcile_remote(library_id, &root, &listing, budget).await?;
            need_probe.extend(
                outcome
                    .need
                    .into_iter()
                    .map(|path| NodeFileRef { remote_library_id: lib.remote_library_id, path }),
            );
        }

        // A folder the node no longer shares: its files are gone from this server's view.
        if req.libraries.iter().any(|l| l.files.is_some()) {
            for (remote_library_id, library_id) in m8::linked_libraries(&self.db, node_id).await? {
                if !req.libraries.iter().any(|l| l.remote_library_id == remote_library_id) {
                    let root = node_library_root(node_id, remote_library_id);
                    self.scanner()?.reconcile_remote(library_id, &root, &[], 0).await?;
                }
            }
        }

        Ok(NodeCatalogSyncResponse { synced_libraries, total_items, need_probe })
    }

    /// Import probe results for files the server asked about.
    pub async fn upload_probes(
        &self,
        node_id: NodeId,
        upload: NodeProbeUpload,
    ) -> Result<NodeProbeUploadResponse> {
        if upload.files.len() > MAX_PROBE_UPLOAD {
            return Err(Error::Validation(format!("at most {MAX_PROBE_UPLOAD} files per upload")));
        }
        let mut failed = 0u32;
        let mut by_library: std::collections::BTreeMap<
            i64,
            Vec<(RemoteFile, mbl_probe::MediaInfo)>,
        > = std::collections::BTreeMap::new();
        for file in upload.files {
            let info = serde_json::from_value::<mbl_probe::MediaInfo>(file.probe);
            match info {
                Ok(info) if valid_rel_path(&file.path) => {
                    by_library.entry(file.remote_library_id).or_default().push((
                        RemoteFile { rel: file.path, size: file.size, mtime: file.mtime },
                        info,
                    ))
                }
                _ => failed += 1,
            }
        }
        let mut imported = 0u32;
        for (remote_library_id, files) in by_library {
            let Some(library_id) = m8::linked_library(&self.db, node_id, remote_library_id).await?
            else {
                failed += u32::try_from(files.len()).unwrap_or(u32::MAX);
                continue;
            };
            let root = node_library_root(node_id, remote_library_id);
            let stats = self.scanner()?.import_remote(library_id, &root, files).await?;
            imported += u32::try_from(stats.added + stats.changed).unwrap_or(u32::MAX);
            failed += u32::try_from(stats.failed).unwrap_or(u32::MAX);
        }
        m8::update_node_heartbeat(&self.db, node_id, "online", now_ms()).await?;
        Ok(NodeProbeUploadResponse { imported, failed })
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

    /// Smart handover for a node part the caller may play: LAN clients get a signed URL straight
    /// to the node (valid six hours); others are told to use the server's own stream URLs, which
    /// relay the file.
    pub async fn handover(
        &self,
        node_path: &str,
        client_on_lan: bool,
    ) -> Result<NodeHandoverResponse> {
        if client_on_lan {
            let url = signed_node_file_url(&self.db, node_path, 6 * 3600).await?;
            Ok(NodeHandoverResponse {
                direct_play_url: Some(url),
                requires_relay: false,
                reason: "Client is on the local network; play directly from the storage node"
                    .into(),
            })
        } else {
            Ok(NodeHandoverResponse {
                direct_play_url: None,
                requires_relay: true,
                reason: "Client is outside the local network; the server relays the media".into(),
            })
        }
    }
}
