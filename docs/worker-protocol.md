# MBL Distributed Transcode Worker & Remote Node Protocol Specification

- **Status:** Ratified (M8.1)
- **Version:** 1.0.0
- **Applies to:** `mbl-server`, `mbl-worker`, `mbl-node`

---

## 1. Overview & Architecture

MBL uses a distributed node topology where a lightweight central server (such as a Raspberry Pi 5 with storage) can offload heavy video transcoding workloads to dedicated GPU transcode workers (e.g. Windows gaming PCs, Linux boxes with NVIDIA NVENC or Intel Arc/QSV) and federate remote library storage nodes (NAS, secondary servers).

```
                 +-----------------------------------------+
                 |       MBL Central Server (Pi 5)         |
                 |  - SQLite DB & Media Catalog            |
                 |  - Playback Decision Engine             |
                 |  - HLS Master/Media Playlists           |
                 +-------------------+---------------------+
                                     |
              +----------------------+----------------------+
              | Mutual TLS / Auth                           | Mutual TLS / Delta Sync
              v                                             v
+-------------------------------+             +-------------------------------+
|    mbl-worker (GPU Worker)    |             |    mbl-node (Federated Node)  |
| - NVIDIA / Intel QSV / VAAPI  |             | - Local disk storage / NAS    |
| - Typed JobSpec execution     |             | - Local scanner & metadata    |
| - Range read source file      |             | - Direct play LAN handover    |
| - HTTP PUT fMP4 segments      |             | - Relay transcode proxy       |
+-------------------------------+             +-------------------------------+
```

---

## 2. Security & No-Remote-Command-Execution

To maintain strict security across LAN and tailnet boundaries:
1. **No arbitrary shell execution:** The central server **never** sends shell command lines or raw FFmpeg argument strings to workers. It sends a strictly typed `WorkerJobSpec`.
2. **Mutual Authentication:** Workers and Nodes authenticate using scoped bearer tokens or mTLS with pinned SHA-256 certificate fingerprints exchanged during pairing.
3. **Signed Source Range URLs:** Workers access source media via short-lived HMAC-signed HTTP URLs (`/api/v1/stream/worker/source/{partId}?token=...`), preventing workers from browsing or traversing server filesystems.
4. **Segment Upload Scoping:** Workers upload rendered `.m4s` HLS segments via `PUT /api/v1/stream/hls/{sessionId}/segments/{filename}` restricted to the session ID assigned to that worker.

---

## 3. Worker Pairing Handshake

A one-time pairing flow registers a new worker:

1. **Admin generates pairing code:**
   - Admin in Web UI or via MCP `pair_worker` calls:
     `POST /api/v1/workers/pairing-code`
   - Server returns a 6-character code (e.g. `W-749201`) valid for 15 minutes.
2. **Worker submits pairing request** (after running its encoder self-test):
   - Worker CLI or service invokes:
     `POST /api/v1/workers/pair`
     ```json
     {
       "pairingCode": "W-749201",
       "name": "Gaming PC (RTX 4080)",
       "hostname": "desktop-gaming",
       "macAddress": "00:1A:2B:3C:4D:5E",
       "capabilities": {
         "gpuVendor": "Nvidia",
         "gpuDeviceName": "NVIDIA GeForce RTX 4080",
         "supportedVideoDecoders": ["h264", "hevc", "av1", "vp9"],
         "supportedVideoEncoders": ["h264_nvenc", "hevc_nvenc", "av1_nvenc"],
         "supportsHardwareTonemap": true,
         "maxConcurrentSessions": 5
       },
       "clientCertFingerprint": "sha256:abcd..."
     }
     ```
3. **Server validates and registers:**
   - Server consumes pairing code, creates row in `workers`, and responds with `WorkerId` and persistent auth token:
     ```json
     {
       "workerId": 1,
       "authToken": "mbl_wrk_7a8b9c...",
       "serverVersion": "0.6.0",
       "serverName": "Living Room Server"
     }
     ```

---

## 4. Transcode Job Execution Flow

Implemented in `mbl_transcode::remote` (server broker), `mbl-api` worker routes and `mbl-worker`.

1. **Polling:** the worker keeps one long-poll open:
   `POST /api/v1/workers/poll` (`Authorization: Bearer mbl_wrk_…`) with `WorkerPollRequest`:
   the encoders its own FFmpeg self-test proved (`{"backend": "nvenc", "codec": "h264"}` …),
   `maxJobs`, `paused` and the IDs of the jobs it is still running. The server answers at once
   when there is work, otherwise with an empty list after about 25 s. Heartbeats
   (`POST /api/v1/workers/heartbeat`, every 5 s) keep the worker counted as alive during the wait.
2. **Assignment:** when an HLS session needs a video encode and a worker is online, not paused,
   below `maxJobs` and has a matching encoder (hardware preferred, software only for a single
   rendition), the server queues `WorkerMessage::Start`:
   ```json
   {
     "type": "start",
     "job": {
       "jobId": "3f2a…",
       "sourcePath": "/api/v1/stream/worker/source/42?token=…",
       "renditions": ["0"],
       "pipeline": { "input": "source", "startMs": 12000, "video": {"op": "encode", …},
                     "output": {"dir": "output", "segmentSecs": 6, "startNumber": 2, …},
                     "backend": "nvenc" }
     }
   }
   ```
   `pipeline` is the server's typed transcode model. The server never sends command lines or
   its own paths; the worker builds FFmpeg arguments itself with `mbl_transcode::build_args` after
   replacing the input with `<server>{sourcePath}` and the output with a private temp directory,
   and refuses jobs whose backend/codec it did not prove, text-subtitle burn-in, or unexpected
   source paths.
3. **Media reading:** FFmpeg reads the signed source URL with HTTP range requests. Tokens are HMAC
   signed with a random per-process key, bound to worker and part, and expire after 12 hours.
4. **Output upload:** as FFmpeg's EVENT playlist grows, the worker uploads `init.mp4`, each new
   `<n>.m4s` and then the playlist itself:
   `PUT /api/v1/workers/jobs/{jobId}/files/{rendition}/{file}`. The server accepts only those file
   names, only for a running job owned by that worker, and moves each file into the session
   directory under the broker lock, so output from a job that was just replaced (seek) is refused
   (404). The HLS session then reads progress exactly as it would from a local FFmpeg.
5. **Control:** `Stop` (seek or session end; a seek starts a new job), `Pause`/`Resume` (the
   client is more than `ahead_segments` behind; SIGSTOP/SIGCONT on Unix, thread suspension on
   Windows).
6. **Outcome:** `POST /api/v1/workers/jobs/{jobId}/report` with `finished` or `failed` and the last
   FFmpeg error lines.

## 5. Failover, Load & Wake-on-LAN

- **Failover:** a job fails when the worker reports failure, drops it from `activeJobIds`, is
  revoked, or is silent for 15 s. The session then encodes locally from the next requested
  segment and never returns to that worker. A session placed on a worker does not take a local
  encoder slot; on failover it runs locally even if that exceeds the local limit.
- **Pause-while-gaming:** with `--pause-while-gaming` (default on) and `nvidia-smi` available, an
  idle worker whose GPU is at least 60 % busy reports `paused` and receives no new jobs. Running
  jobs continue. Other GPUs are not measured.
- **Wake-on-LAN:** an admin can send a magic packet to a paired worker with a MAC address. The
  server does not wake workers automatically before dispatch.
- **Not offloaded:** remux/copy sessions, the Cast generated-timeline path, text subtitle burn-in,
  live TV and DVR.

## 6. Storage Nodes (`mbl-node`)

Implemented in `mbl-node`, `RemoteNodeService` and `Scanner::{reconcile_remote, import_remote}`.

1. **Pairing:** `POST /api/v1/nodes/pair` with `baseUrl` (http/https). The response adds
   `streamSecret`, a random 32-byte key (hex) the node keeps to verify file URLs.
2. **Catalog sync:** `POST /api/v1/nodes/{id}/sync-catalog` (node bearer token) with every shared
   folder and its complete listing:
   `{"libraries": [{"remoteLibraryId": 1, "name": "Movies", "kind": "movie", "itemCount": 2,
   "files": [{"path": "Heat (1995)/Heat (1995).mkv", "size": 123, "mtime": 1700000000000}]}]}`.
   The server creates one MBL library per folder (root `mblnode://<node>/<folder>`), marks files that
   disappeared missing, and answers `needProbe` with at most 200 new or changed files. A folder the
   node stops listing has all its files marked missing.
3. **Probe upload:** the node runs FFprobe on those files and posts
   `POST /api/v1/nodes/{id}/catalog/probes` with up to 100 `{remoteLibraryId, path, size, mtime,
   probe}` entries (`probe` is MBL's `MediaInfo`). The server imports them with the scanner's normal
   write path (parsing, versions, streams, metadata matching). The node syncs again until
   `needProbe` is empty.
4. **File access:** the node serves `GET|HEAD /files/{folder}/{path}?exp=<ms>&sig=<b64url>` where
   `sig = HMAC-SHA256(streamSecret, "n1:{folder}:{exp}:{path}")`. It rejects expired or wrong
   signatures (403), traversal, and anything resolving outside the folder (404). The server relays
   direct play, passes signed URLs to FFmpeg for transcoding, and hands signed URLs to signed-in LAN
   clients via `GET /api/v1/nodes/{id}/stream/handover/{itemId}`.
5. **Revocation:** `DELETE /api/v1/nodes/{id}` deletes the node's libraries; its token stops working.
