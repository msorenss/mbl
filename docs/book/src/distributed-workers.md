# Distributed Nodes & Transcode Workers

MBL is the only personal media server with built-in native node distribution, enabling low-power servers (such as a Raspberry Pi 5) to offload heavy transcode jobs to powerful desktop workstations or GPU compute rigs.

## Component Overview

- **`mbl-worker`:** Dedicated transcode worker agent running on Windows or Linux.
- **`mbl-node`:** Federated edge storage node running on remote NAS or mini-PCs.

## Pairing Flow & Worker Protocol

Workers pair securely with the primary server using 6-digit one-time codes generated in the Web Admin dashboard (`/admin/workers`):

```bash
# On the worker machine:
mbl-worker pair --server http://mbl.lan:8420 --code 481920
```

1. The server issues a cryptographically signed worker identity token with restricted scope.
2. The worker advertises hardware capabilities (`WorkerCapabilities`) via 5-second heartbeats.
3. When transcoding is required, the server crafts an HMAC-signed short-lived source URL (`/api/v1/stream/worker/source/{partId}?token=...`).
4. The worker processes segments in parallel and streams chunks directly back to the server's session directory via `PUT /api/v1/stream/hls/{sessionId}/segments/{filename}`.

## Wake-on-LAN & Gaming-Aware Offload

- **Wake-on-LAN (WoL):** If a worker is configured with a MAC address, the server broadcasts standard magic packets (`6x 0xFF + 16x MAC`) to wake sleep-state desktop GPUs before dispatching batch jobs.
- **Pause-While-Gaming Guard:** On Windows, `mbl-worker` samples GPU 3D utilization. If non-transcode GPU activity spikes (e.g., the user launches a video game), the worker signals `paused_gaming` and the server seamlessly fails back to local transcoding.

## Smart Handover for Federated Nodes (`mbl-node`)

When a user requests media hosted on a federated `mbl-node`, MBL checks client network topology (`GET /api/v1/nodes/{id}/stream/handover/{itemId}`):
- **On the same LAN:** The client is seamlessly handed over to direct-play straight from the edge node, conserving server network I/O.
- **Across WAN:** The server proxies and relays the stream securely.
