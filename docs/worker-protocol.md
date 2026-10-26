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
2. **Worker submits pairing request:**
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

When the server decision engine selects a worker for a playback session:

1. **Job Dispatch:**
   - Server transmits `WorkerJobSpec` to worker:
     ```json
     {
       "sessionId": "hls_sess_10293847",
       "partId": 42,
       "sourceUrl": "http://192.168.1.50:8420/api/v1/stream/worker/source/42?token=...",
       "targetFormat": "fmp4",
       "segmentDurationSecs": 6,
       "videoCodec": "h264_nvenc",
       "audioCodec": "aac",
       "videoBitrateKbps": 8000,
       "audioBitrateKbps": 256,
       "width": 1920,
       "height": 1080,
       "startTimeOffsetSecs": 0.0,
       "uploadUrlTemplate": "http://192.168.1.50:8420/api/v1/stream/hls/hls_sess_10293847/segments/{filename}"
     }
     ```
2. **Media Reading:**
   - If NAS path mapping is configured (e.g. `/mnt/nas/movies` -> `Z:\movies`), the worker reads the file directly from disk.
   - Otherwise, the worker issues HTTP range requests to `sourceUrl`.
3. **Segment Upload:**
   - As each segment finishes encoding (e.g. `segment_00000.m4s`), the worker issues:
     `PUT /api/v1/stream/hls/{sessionId}/segments/segment_00000.m4s`
     `Authorization: Bearer mbl_wrk_...`
   - Server stores the segment in the session directory and signals waiting HTTP playback requests.
4. **Control & Throttling:**
   - Server sends `Pause` (SIGSTOP/NtSuspend) when client buffer is >= 10 segments ahead.
   - Server sends `Resume` (SIGCONT) when client buffer drops below 5 segments.
   - Server sends `Seek { segment_number }` or `Cancel` when client seeks far or stops.

---

## 5. Heartbeat, Load & Wake-on-LAN

- **Heartbeat:** Every 5 seconds, worker sends `POST /api/v1/workers/heartbeat` with active session count, CPU usage %, and GPU usage %.
- **Pause-while-gaming:** If Windows worker detects high GPU usage (>80% for non-transcode tasks) or a fullscreen gaming process, it sets status to `paused_gaming`. Server transparently holds new jobs or dispatches them locally.
- **Wake-on-LAN:** If worker is offline or sleeping, server sends a standard Wake-on-LAN magic packet (`6x 0xFF + 16x MAC`) over UDP port 9 to wake the machine before job dispatch.
- **Failover:** If worker fails to heartbeat within 15 seconds during an active stream, the server terminates the remote assignment and seamlessly restarts transcode locally at the current segment boundary.

---

## 6. Remote Library Storage Nodes (`mbl-node`)

- **Pairing & Federation:** Storage nodes register via pairing codes and provide a list of local libraries (`POST /api/v1/nodes/{id}/sync-catalog`).
- **Catalog Aggregation:** The central server surfaces node libraries with an identifiable badge (e.g. `"Storage: NAS-Node-1"`).
- **Smart Handover:**
  - If client IP is on the same local subnet as the storage node, the central server returns an HTTP 302 redirect or direct playback URL directly to the node.
  - If client is remote (WAN) and storage node has HW transcode, node transcodes locally; otherwise central server proxies media.
