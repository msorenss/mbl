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
2. `mbl-worker run` tests its FFmpeg encoders (NVENC, QSV, VAAPI, AMF, software) and keeps a long-poll open to the server, plus a heartbeat every 5 seconds.
3. When playback needs a video encode and a worker with a matching encoder is free, the server hands the job to the worker instead of encoding locally. The worker reads the source through a signed, expiring URL and uploads the HLS output as it is produced. Playback looks the same to the client.
4. If the worker fails, is revoked or stops responding for 15 seconds, the server continues the stream locally from the next segment.

Run the worker on Windows (PowerShell, in the folder with `mbl-worker.exe` and the bundled `ffmpeg.exe`):

```powershell
.\mbl-worker.exe pair --server https://mbl.example.com:8443 --code W-123456 --name "Gaming PC"
.\mbl-worker.exe run
```

The configuration is stored in `%ProgramData%\MBL\worker.json` (use an elevated prompt for `pair`, or pass `--config`). `run` accepts `--max-jobs`, `--ffmpeg`, `--work-dir` and `--pause-while-gaming false`.

Not offloaded: remux/copy sessions, text subtitle burn-in, Live TV and DVR. The worker must be able to reach the server URL it was paired with.

## Wake-on-LAN & Gaming-Aware Offload

- **Wake-on-LAN (WoL):** If a worker is paired with a MAC address, an admin can send a standard magic packet (`6x 0xFF + 16x MAC`) from the Workers page or MCP. The server does not wake workers automatically.
- **Pause-While-Gaming Guard:** With an NVIDIA GPU (`nvidia-smi`), an idle worker whose GPU is at least 60 % busy reports `paused_gaming` and receives no new jobs; new streams are encoded on the server. Running jobs continue.

## Storage Nodes (`mbl-node`)

A storage node shares media folders on another machine (Windows or Linux) with the MBL server.
Each folder appears in MBL as an ordinary library named `<folder> (<node>)`, with metadata,
watch state and permissions like any other library. Files stay on the node; the server reads them
through the node when someone watches.

1. **Pair:** create a code under *Administration → Distributed Nodes → Pair Storage Node*, then on
   the node:
   ```bash
   mbl-node pair --server http://mbl.lan:8420 --code N-123456 --base-url http://192.168.1.60:8430
   ```
   `--base-url` is the address the server (and LAN clients) use to reach the node. On Windows the
   configuration is stored in `%ProgramData%\MBL\node.json` (run elevated); on Linux pass
   `--config /var/lib/mbl-node/node.json` (the systemd unit uses that file).
2. **Share folders:**
   ```bash
   mbl-node library add --name Movies --kind movie --path /srv/movies
   mbl-node library add --name Series --kind show  --path /srv/tv
   ```
   Kinds are `movie`, `show`, `music`, `photo` and `homevideo`; folders may not overlap.
3. **Run:** `mbl-node run` (or `systemctl enable --now mbl-node`). The node lists its folders,
   probes new and changed files with its own FFprobe when the server asks for them, and syncs every
   5 minutes (`--sync-minutes`). It serves files on the `--base-url` port; open that port in the
   node's firewall.

How playback works:

- **Direct play** and downloads use the server's normal stream URLs; the server relays the bytes
  from the node (with range requests), so WAN and Cast clients need no access to the node.
- **Transcoding** (on the server or a transcode worker) reads the node file through a signed URL.
- **LAN handover:** `GET /api/v1/nodes/{id}/stream/handover/{itemId}` gives a signed-in client on
  the local network a signed URL straight to the node, valid for six hours.
- Every node URL is signed by the server with a key exchanged at pairing (HMAC-SHA256, bound to the
  folder, path and expiry). The node refuses unsigned or altered requests and never serves files
  outside its shared folders.

Limits: seek previews (trickplay) are not generated for node media, sidecar subtitle files next
to node media are not picked up (embedded subtitles work), and a file that is renamed on the node
is treated as removed and added. Revoking a node deletes its libraries.

## Smart Handover

When a user requests media hosted on a federated `mbl-node`, MBL checks client network topology (`GET /api/v1/nodes/{id}/stream/handover/{itemId}`):
- **On the same LAN:** The client is seamlessly handed over to direct-play straight from the edge node, conserving server network I/O.
- **Across WAN:** The server proxies and relays the stream securely.
