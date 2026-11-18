//! Transcode Worker service (M8.1–M8.9).
//! Handles pairing codes, worker registration, Wake-on-LAN magic packets,
//! signed media range tokens, segment uploads, and worker selection.

use std::net::SocketAddr;
use std::sync::Arc;

use mbl_auth::{
    WorkerSourceTokenClaims, hash_token, sign_worker_source_token, verify_worker_source_token,
};
use mbl_core::{
    Actor, Error, PartId, Result, WorkerHeartbeat, WorkerId, WorkerJobReport, WorkerPairRequest,
    WorkerPairResponse, WorkerPollRequest, WorkerPollResponse, time::now_ms,
};
use mbl_transcode::RemoteDispatcher;

/// Largest single file a worker may upload.
pub const MAX_WORKER_UPLOAD_BYTES: usize = mbl_transcode::remote::MAX_UPLOAD_BYTES;
use mbl_db::Db;
use mbl_db::repo::m8::{self, WorkerRow};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Batch offload estimation returned for dry-run optimization requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkerBatchEstimate {
    pub selected_worker_id: Option<WorkerId>,
    pub selected_worker_name: Option<String>,
    pub estimated_duration_secs: u64,
    pub estimated_output_bytes: u64,
    pub can_offload: bool,
}

#[derive(Debug, Clone)]
pub struct WorkerService {
    db: Arc<Db>,
    stream_secret: [u8; 32],
    dispatcher: Option<Arc<RemoteDispatcher>>,
}

impl WorkerService {
    pub fn new(db: Arc<Db>) -> Self {
        // Source tokens only need to outlive the running process, so a fresh random key per
        // start is enough and never leaves the machine.
        let mut stream_secret = [0u8; 32];
        if let Err(error) = getrandom::fill(&mut stream_secret) {
            tracing::error!(%error, "no randomness for worker source tokens; worker media access is disabled");
        }
        Self { db, stream_secret, dispatcher: None }
    }

    pub fn with_stream_secret(mut self, secret: [u8; 32]) -> Self {
        self.stream_secret = secret;
        self
    }

    /// Attach the job broker shared with the HLS session manager.
    pub fn with_dispatcher(mut self, dispatcher: Arc<RemoteDispatcher>) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    pub fn dispatcher(&self) -> Option<&Arc<RemoteDispatcher>> {
        self.dispatcher.as_ref()
    }

    /// A signer for the dispatcher: source URLs live for 12 hours, long enough for one film.
    pub fn source_signer(&self) -> mbl_transcode::SourceSigner {
        let service = self.clone();
        Arc::new(move |worker_id, part_id| {
            service.generate_source_token(worker_id, part_id, 12 * 3600)
        })
    }

    fn require_dispatcher(&self) -> Result<&Arc<RemoteDispatcher>> {
        self.dispatcher.as_ref().ok_or_else(|| {
            Error::Unavailable("transcode offload is not available on this server".into())
        })
    }

    /// Long-poll for work: returns queued messages, or an empty list after `wait`.
    pub async fn poll(
        &self,
        worker: &WorkerRow,
        req: &WorkerPollRequest,
        wait: std::time::Duration,
    ) -> Result<WorkerPollResponse> {
        let dispatcher = self.require_dispatcher()?;
        let state = if req.paused { "paused_gaming" } else { "idle" };
        m8::update_worker_heartbeat(&self.db, worker.id, state, now_ms()).await?;
        let notify = dispatcher.register_poll(worker.id, req);
        let notified = notify.notified();
        let mut messages = dispatcher.take_messages(worker.id);
        if messages.is_empty() {
            let _ = tokio::time::timeout(wait, notified).await;
            messages = dispatcher.take_messages(worker.id);
        }
        Ok(WorkerPollResponse { messages })
    }

    pub fn report_job(
        &self,
        worker: &WorkerRow,
        job_id: &str,
        report: &WorkerJobReport,
    ) -> Result<()> {
        self.require_dispatcher()?.report(worker.id, job_id, report)
    }

    pub async fn store_job_file(
        &self,
        worker: &WorkerRow,
        job_id: &str,
        rendition: &str,
        filename: &str,
        data: &[u8],
    ) -> Result<()> {
        self.require_dispatcher()?.store_upload(worker.id, job_id, rendition, filename, data).await
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Generate a 6-digit pairing code valid for 15 minutes.
    pub async fn generate_pairing_code(&self, actor: &Actor) -> Result<String> {
        actor.require_admin()?;
        let mut rnd_bytes = [0u8; 4];
        getrandom::fill(&mut rnd_bytes).map_err(|e| {
            Error::Internal(format!("failed to generate random pairing code: {e}").into())
        })?;
        let random_num = u32::from_ne_bytes(rnd_bytes) % 1_000_000;
        let code = format!("W-{random_num:06}");
        let expires_at = now_ms() + (15 * 60 * 1000);

        m8::create_worker_pairing_code(&self.db, &code, actor.user_id, expires_at).await?;
        Ok(code)
    }

    /// Pair a new worker using a valid pairing code.
    pub async fn pair_worker(&self, req: WorkerPairRequest) -> Result<WorkerPairResponse> {
        let maybe_user = m8::consume_worker_pairing_code(&self.db, &req.pairing_code).await?;
        if maybe_user.is_none() {
            return Err(Error::Unauthorized);
        }

        let mut token_bytes = [0u8; 24];
        getrandom::fill(&mut token_bytes)
            .map_err(|e| Error::Internal(format!("failed to generate random token: {e}").into()))?;
        let hex_token: String = token_bytes.iter().map(|b| format!("{b:02x}")).collect();
        let auth_token = format!("mbl_wrk_{hex_token}");
        let token_hash = hash_token(&auth_token);

        let max_sessions = req.capabilities.max_concurrent_sessions.max(1);
        let worker_id = m8::register_worker(
            &self.db,
            &req.name,
            req.hostname.as_deref(),
            req.mac_address.as_deref(),
            &token_hash,
            req.client_cert_fingerprint.as_deref(),
            &req.capabilities,
            max_sessions,
            &[],
        )
        .await?;

        Ok(WorkerPairResponse {
            worker_id,
            auth_token,
            server_version: env!("CARGO_PKG_VERSION").into(),
            server_name: "MBL Server".into(),
        })
    }

    /// Authenticate a worker request via bearer auth header.
    pub async fn authenticate(&self, header_value: &str) -> Result<WorkerRow> {
        let token = header_value.strip_prefix("Bearer ").unwrap_or(header_value).trim();
        let token_hash = hash_token(token);

        let worker = m8::get_worker_by_token_hash(&self.db, &token_hash).await?;
        worker.ok_or(Error::Unauthorized)
    }

    pub async fn list_workers(&self, actor: &Actor) -> Result<Vec<WorkerRow>> {
        actor.require_admin()?;
        m8::list_workers(&self.db).await
    }

    pub async fn get_worker(&self, actor: &Actor, id: WorkerId) -> Result<WorkerRow> {
        actor.require_admin()?;
        let worker = m8::get_worker(&self.db, id).await?;
        worker.ok_or(Error::NotFound)
    }

    pub async fn delete_worker(&self, actor: &Actor, id: WorkerId) -> Result<bool> {
        actor.require_admin()?;
        let deleted = m8::delete_worker(&self.db, id).await?;
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.remove_worker(id);
        }
        Ok(deleted)
    }

    pub async fn record_heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<()> {
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.touch(heartbeat.worker_id);
        }
        m8::update_worker_heartbeat(
            &self.db,
            heartbeat.worker_id,
            &heartbeat.status.state,
            now_ms(),
        )
        .await
    }

    /// Send a Wake-on-LAN magic packet (UDP broadcast) to the worker's MAC address.
    pub async fn wake_worker(&self, actor: &Actor, id: WorkerId) -> Result<()> {
        actor.require_admin()?;
        let worker = self.get_worker(actor, id).await?;
        let mac_str = worker.mac_address.ok_or_else(|| {
            Error::Validation("Worker does not have a registered MAC address".into())
        })?;

        let mac_bytes = parse_mac_address(&mac_str)?;
        let mut magic_packet = Vec::with_capacity(102);
        magic_packet.extend_from_slice(&[0xFF; 6]);
        for _ in 0..16 {
            magic_packet.extend_from_slice(&mac_bytes);
        }

        if let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:0").await {
            let _ = socket.set_broadcast(true);
            let target = SocketAddr::from(([255, 255, 255, 255], 9));
            if let Err(e) = socket.send_to(&magic_packet, target).await {
                tracing::warn!(error = %e, "Failed to broadcast WoL packet over UDP");
            }
        }

        Ok(())
    }

    /// Sign a source media token granting the worker access to stream source range chunks.
    pub fn generate_source_token(
        &self,
        worker_id: WorkerId,
        part_id: PartId,
        ttl_secs: u64,
    ) -> String {
        let claims = WorkerSourceTokenClaims {
            worker_id,
            part_id,
            expires_at_ms: now_ms() + (ttl_secs as i64 * 1000),
        };
        sign_worker_source_token(&self.stream_secret, claims)
    }

    /// Verify a worker source token and ensure it matches the requested part.
    pub fn verify_source_token(&self, token: &str, part_id: PartId) -> Result<WorkerId> {
        let claims = verify_worker_source_token(&self.stream_secret, token, part_id, now_ms())?;
        Ok(claims.worker_id)
    }

    /// Select an idle transcode worker for job dispatch.
    pub async fn select_transcode_worker(&self) -> Result<Option<WorkerRow>> {
        let workers = m8::list_workers(&self.db).await?;
        let now = now_ms();
        for w in workers {
            if w.status == "idle" && w.last_heartbeat_at.is_some_and(|hb| now - hb < 30_000) {
                return Ok(Some(w));
            }
        }
        Ok(None)
    }

    /// Estimate batch offload for dry-run requests.
    pub async fn estimate_batch_offload(
        &self,
        duration_secs: u64,
        target_count: usize,
    ) -> Result<WorkerBatchEstimate> {
        let candidate = self.select_transcode_worker().await?;
        let (worker_id, worker_name, can_offload) = match &candidate {
            Some(w) => (Some(w.id), Some(w.name.clone()), true),
            None => (None, None, false),
        };

        // Rough heuristic: GPU transcode ~4x real-time; 1080p ~4 Mbps = 0.5 MB/s
        let estimated_duration_secs = if can_offload {
            (duration_secs * target_count as u64) / 4
        } else {
            duration_secs * target_count as u64
        };
        let estimated_output_bytes = duration_secs * 500_000 * target_count as u64;

        Ok(WorkerBatchEstimate {
            selected_worker_id: worker_id,
            selected_worker_name: worker_name,
            estimated_duration_secs,
            estimated_output_bytes,
            can_offload,
        })
    }
}

/// Helper function to parse MAC address string into 6 raw bytes.
pub fn parse_mac_address(mac_str: &str) -> Result<[u8; 6]> {
    let clean: String = mac_str.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if clean.len() != 12 {
        return Err(Error::Validation(format!(
            "Invalid MAC address '{mac_str}': expected 12 hexadecimal characters"
        )));
    }

    let mut bytes = [0u8; 6];
    for i in 0..6 {
        bytes[i] = u8::from_str_radix(&clean[i * 2..i * 2 + 2], 16).map_err(|_| {
            Error::Validation(format!("Invalid hex byte in MAC address '{mac_str}'"))
        })?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mac_address_formats() {
        let mac1 = parse_mac_address("00:1A:2B:3C:4D:5E").unwrap();
        assert_eq!(mac1, [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);

        let mac2 = parse_mac_address("00-1a-2b-3c-4d-5e").unwrap();
        assert_eq!(mac2, [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);

        let mac3 = parse_mac_address("001a2b3c4d5e").unwrap();
        assert_eq!(mac3, [0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);

        assert!(parse_mac_address("invalid").is_err());
        assert!(parse_mac_address("00:1A:2B").is_err());
    }
}
