//! MBL storage node: shares local media folders with a central MBL server.
//!
//! The node lists its folders, probes new or changed files with its own FFprobe when the server
//! asks, and serves files over HTTP to the server (and to LAN clients the server hands over)
//! through URLs the server signs with the key exchanged at pairing. The server mirrors each
//! folder as a regular library; nothing is copied.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use axum::Router;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use mbl_core::{
    LibraryKind, NodeCatalogSyncRequest, NodeCatalogSyncResponse, NodeFileEntry, NodeId,
    NodeLibrary, NodeProbeUpload, NodeProbeUploadResponse, NodeProbedFile,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// A shared folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeLibraryConfig {
    /// Stable number for this folder; it identifies the library on the server.
    pub id: i64,
    pub name: String,
    pub kind: LibraryKind,
    pub path: PathBuf,
}

/// Configuration written by `mbl-node pair` and edited by `mbl-node library`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeConfigFile {
    pub server_url: String,
    pub node_id: NodeId,
    pub auth_token: String,
    pub name: String,
    /// Address the server and LAN clients use to reach this node.
    pub base_url: String,
    /// Key that verifies the server's signed file URLs.
    #[serde(default)]
    pub stream_secret: Option<String>,
    /// Socket the file server binds; default `0.0.0.0:<port of baseUrl>`.
    #[serde(default)]
    pub listen: Option<String>,
    #[serde(default)]
    pub libraries: Vec<NodeLibraryConfig>,
}

impl NodeConfigFile {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| {
            format!("cannot read {}; run `mbl-node pair` first", path.display())
        })?;
        serde_json::from_str(&text).with_context(|| format!("{} is not valid", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
    }

    /// Socket address for the file server.
    pub fn listen_addr(&self) -> Result<SocketAddr> {
        if let Some(listen) = &self.listen {
            return listen.parse().with_context(|| format!("invalid listen address {listen}"));
        }
        let url = reqwest::Url::parse(&self.base_url).context("invalid baseUrl")?;
        let port = url.port_or_known_default().context("baseUrl has no port")?;
        Ok(SocketAddr::from(([0, 0, 0, 0], port)))
    }

    /// Add a folder, returning its id. Paths must exist and may not overlap another folder.
    pub fn add_library(&mut self, name: &str, kind: LibraryKind, path: &Path) -> Result<i64> {
        let path = dunce::canonicalize(path)
            .with_context(|| format!("folder {} does not exist", path.display()))?;
        if !path.is_dir() {
            bail!("{} is not a folder", path.display());
        }
        if let Some(other) =
            self.libraries.iter().find(|l| l.path.starts_with(&path) || path.starts_with(&l.path))
        {
            bail!(
                "{} overlaps library '{}' ({})",
                path.display(),
                other.name,
                other.path.display()
            );
        }
        let id = self.libraries.iter().map(|l| l.id).max().unwrap_or(0) + 1;
        self.libraries.push(NodeLibraryConfig { id, name: name.trim().to_owned(), kind, path });
        Ok(id)
    }
}

/// Default config location: `%ProgramData%\MBL\node.json` on Windows, `node.json` elsewhere.
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    if let Some(dir) = std::env::var_os("ProgramData") {
        return PathBuf::from(dir).join("MBL").join("node.json");
    }
    PathBuf::from("node.json")
}

// ---------------------------------------------------------------------------------------------
// File server
// ---------------------------------------------------------------------------------------------

struct FileServer {
    secret: Vec<u8>,
    libraries: Vec<(i64, PathBuf)>,
}

#[derive(Deserialize)]
struct Signed {
    exp: i64,
    sig: String,
}

/// Router serving `GET|HEAD /files/{library}/{*path}?exp=…&sig=…` and `GET /health`.
pub fn file_router(config: &NodeConfigFile) -> Result<Router> {
    let secret = config
        .stream_secret
        .clone()
        .filter(|s| !s.is_empty())
        .context("this node was paired with an older server version; pair it again")?;
    let libraries = config
        .libraries
        .iter()
        .map(|l| Ok((l.id, dunce::canonicalize(&l.path).unwrap_or_else(|_| l.path.clone()))))
        .collect::<Result<Vec<_>>>()?;
    let state = Arc::new(FileServer { secret: secret.into_bytes(), libraries });
    Ok(Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/files/{library}/{*path}", get(serve_file).head(serve_file))
        .with_state(state))
}

async fn serve_file(
    State(state): State<Arc<FileServer>>,
    method: Method,
    AxumPath((library, rel)): AxumPath<(i64, String)>,
    Query(signed): Query<Signed>,
    headers: HeaderMap,
) -> Response {
    let now = mbl_core::time::now_ms();
    if !mbl_auth::verify_node_file(&state.secret, library, &rel, signed.exp, &signed.sig, now) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(path) = confined_path(&state.libraries, library, &rel) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let container = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok());
    match mbl_stream::serve_direct_file(&path, container.as_deref(), range, method == Method::HEAD)
        .await
    {
        Ok(response) => response,
        Err(mbl_core::Error::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            warn!(%error, "serving file failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Resolve `rel` inside library `library`, refusing anything that escapes the folder.
fn confined_path(libraries: &[(i64, PathBuf)], library: i64, rel: &str) -> Option<PathBuf> {
    let root = &libraries.iter().find(|(id, _)| *id == library)?.1;
    if rel.is_empty()
        || rel.contains('\\')
        || rel.contains('\0')
        || rel.split('/').any(|c| c.is_empty() || c == "." || c == ".." || c.contains(':'))
    {
        return None;
    }
    let mut path = root.clone();
    path.extend(rel.split('/'));
    let canonical = dunce::canonicalize(&path).ok()?;
    (canonical.starts_with(root) && canonical.is_file()).then_some(canonical)
}

// ---------------------------------------------------------------------------------------------
// Catalog sync
// ---------------------------------------------------------------------------------------------

fn kind_name(kind: LibraryKind) -> String {
    serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

/// List a folder's media files with paths relative to it (`/` separated).
pub fn list_library(library: &NodeLibraryConfig) -> Vec<NodeFileEntry> {
    let Some(root) = library.path.to_str() else {
        warn!(path = %library.path.display(), "skipping folder with a non-UTF-8 path");
        return Vec::new();
    };
    mbl_parser::walk::walk_from(root, root, library.kind)
        .iter()
        .map(|f| NodeFileEntry {
            path: f.relative().replace('\\', "/"),
            size: f.size,
            mtime: f.mtime,
        })
        .collect()
}

#[derive(Clone)]
struct ServerApi {
    client: reqwest::Client,
    server: String,
    token: String,
    node_id: NodeId,
}

impl ServerApi {
    async fn sync(&self, request: &NodeCatalogSyncRequest) -> Result<NodeCatalogSyncResponse> {
        let url = format!("{}/api/v1/nodes/{}/sync-catalog", self.server, self.node_id.0);
        let res = self.client.post(url).bearer_auth(&self.token).json(request).send().await?;
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            bail!("the server rejected this node; pair it again");
        }
        Ok(res.error_for_status()?.json().await?)
    }

    async fn upload(&self, files: Vec<NodeProbedFile>) -> Result<NodeProbeUploadResponse> {
        let url = format!("{}/api/v1/nodes/{}/catalog/probes", self.server, self.node_id.0);
        let res = self
            .client
            .post(url)
            .bearer_auth(&self.token)
            .json(&NodeProbeUpload { files })
            .send()
            .await?;
        Ok(res.error_for_status()?.json().await?)
    }
}

/// One complete sync: list every folder, then probe and upload what the server asks for until it
/// asks for nothing more. Returns the number of files uploaded.
pub async fn sync_once(
    config: &NodeConfigFile,
    tools: &mbl_probe::FfTools,
    client: &reqwest::Client,
) -> Result<usize> {
    let api = ServerApi {
        client: client.clone(),
        server: config.server_url.trim_end_matches('/').to_owned(),
        token: config.auth_token.clone(),
        node_id: config.node_id,
    };
    let libraries = config.libraries.clone();
    let listings = tokio::task::spawn_blocking(move || {
        libraries.iter().map(|l| (l.clone(), list_library(l))).collect::<Vec<_>>()
    })
    .await?;
    let request = NodeCatalogSyncRequest {
        libraries: listings
            .iter()
            .map(|(l, files)| NodeLibrary {
                remote_library_id: l.id,
                name: l.name.clone(),
                kind: kind_name(l.kind),
                item_count: i64::try_from(files.len()).unwrap_or(i64::MAX),
                files: Some(files.clone()),
            })
            .collect(),
    };
    let mut uploaded = 0usize;
    // Each round the server returns a bounded batch; probed files then count as known.
    for _ in 0..10_000 {
        let response = api.sync(&request).await?;
        if response.need_probe.is_empty() {
            break;
        }
        let mut batch = Vec::new();
        for want in &response.need_probe {
            let Some((library, files)) =
                listings.iter().find(|(l, _)| l.id == want.remote_library_id)
            else {
                continue;
            };
            let Some(entry) = files.iter().find(|f| f.path == want.path) else { continue };
            let mut path = library.path.clone();
            path.extend(want.path.split('/'));
            match tools.probe(&path, mbl_probe::DEFAULT_PROBE_TIMEOUT).await {
                Ok(info) => batch.push(NodeProbedFile {
                    remote_library_id: library.id,
                    path: entry.path.clone(),
                    size: entry.size,
                    mtime: entry.mtime,
                    probe: serde_json::to_value(info)?,
                }),
                Err(error) => warn!(path = %path.display(), %error, "cannot probe file; skipping"),
            }
        }
        if batch.is_empty() {
            // Nothing in this batch could be probed; avoid asking for the same files forever.
            break;
        }
        for chunk in batch.chunks(50) {
            let result = api.upload(chunk.to_vec()).await?;
            uploaded += result.imported as usize;
            if result.failed > 0 {
                warn!(failed = result.failed, "server could not import some files");
            }
        }
    }
    Ok(uploaded)
}

/// Run the file server and periodic sync until `stop` is cancelled.
pub async fn run(
    config: NodeConfigFile,
    tools: mbl_probe::FfTools,
    sync_every: Duration,
    stop: CancellationToken,
) -> Result<()> {
    if config.libraries.is_empty() {
        bail!("no folders are shared; add one with `mbl-node library add`");
    }
    let router = file_router(&config)?;
    let addr = config.listen_addr()?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot listen on {addr}"))?;
    info!(%addr, base_url = %config.base_url, folders = config.libraries.len(), "MBL storage node serving files");
    let server_stop = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router).with_graceful_shutdown(server_stop.cancelled_owned()).await
    });
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(10)).build()?;
    loop {
        match sync_once(&config, &tools, &client).await {
            Ok(n) => info!(uploaded = n, "catalog synced"),
            Err(error) if error.to_string().contains("pair it again") => {
                stop.cancel();
                let _ = server.await;
                return Err(error);
            }
            Err(error) => {
                warn!(error = %format!("{error:#}"), "catalog sync failed; retrying later")
            }
        }
        tokio::select! {
            () = stop.cancelled() => break,
            () = tokio::time::sleep(sync_every) => {}
        }
    }
    let _ = server.await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confines_paths_to_the_shared_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("Show/Season 1")).unwrap();
        std::fs::write(root.join("Show/Season 1/e1.mkv"), b"x").unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.mkv"), b"x").unwrap();
        let libs = [(1, root.clone())];
        assert_eq!(
            confined_path(&libs, 1, "Show/Season 1/e1.mkv"),
            Some(root.join("Show").join("Season 1").join("e1.mkv"))
        );
        for bad in [
            "../secret.mkv",
            "Show/../../secret.mkv",
            "Show\\e1.mkv",
            "/etc/passwd",
            "Show//e1.mkv",
            "C:/x.mkv",
            "",
        ] {
            assert_eq!(confined_path(&libs, 1, bad), None, "{bad}");
        }
        assert_eq!(confined_path(&libs, 2, "Show/Season 1/e1.mkv"), None);
        assert_eq!(confined_path(&libs, 1, "Show"), None, "directories are not served");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path().join("secret.mkv"), root.join("link.mkv"))
                .unwrap();
            assert_eq!(confined_path(&libs, 1, "link.mkv"), None, "symlinks out of the folder");
        }
    }

    #[test]
    fn library_folders_cannot_overlap() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Movies/Kids")).unwrap();
        let mut config = NodeConfigFile {
            server_url: "http://s".into(),
            node_id: NodeId(1),
            auth_token: "t".into(),
            name: "n".into(),
            base_url: "http://n:8430".into(),
            stream_secret: Some("k".into()),
            listen: None,
            libraries: Vec::new(),
        };
        assert_eq!(
            config.add_library("Movies", LibraryKind::Movie, &dir.path().join("Movies")).unwrap(),
            1
        );
        assert!(
            config
                .add_library("Kids", LibraryKind::Movie, &dir.path().join("Movies/Kids"))
                .is_err()
        );
        assert!(config.add_library("All", LibraryKind::Movie, dir.path()).is_err());
        assert!(
            config.add_library("Missing", LibraryKind::Movie, &dir.path().join("nope")).is_err()
        );
        assert_eq!(config.listen_addr().unwrap(), "0.0.0.0:8430".parse().unwrap());
    }
}
