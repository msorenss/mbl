# MBL Media Server — Threat Model & Security Architecture

This document defines the formal threat model, trust boundaries, asset classifications, and STRIDE risk mitigations for MBL (Milestone M10.1, aligned with PLAN §14).

---

## 1. System Overview & Trust Zones

MBL operates in a hybrid home/remote deployment model, interacting with web browsers, 10-foot TV clients, distributed hardware transcode workers, federated storage nodes, AI agents over MCP, and cloud metadata providers.

```mermaid
flowchart TB
    subgraph Untrusted_WAN [Untrusted Internet / WAN]
        RemoteClients[Remote Web / Mobile Clients]
        Attacker[Malicious Actors / Internet Probers]
    end

    subgraph Trusted_Network [Protected Network / Home LAN & Tailnet]
        LocalClients[Local TV / Browser Clients]
        WorkerNodes[Transcode Workers (GPU PCs)]
        StorageNodes[Federated Storage Nodes (NAS)]
        HAPlatform[Home Assistant / MQTT Broker]
    end

    subgraph MBL_Host [MBL Server Host Environment]
        ReverseProxy[Reverse Proxy / TLS Terminator]
        MBLCore[MBL Server Process (Tokio)]
        SQLite[(Encrypted / WAL SQLite DB)]
        MediaStorage[Local Media Disks / Mounts]
        HailoNPU[Hailo-8 / Hailo-10H NPU (/dev/hailo0)]
    end

    subgraph AI_Ecosystem [AI Agents & MCP Ecosystem]
        ClaudeAgent[Claude / Hermes / Local LLMs]
        MCPServer[MBL MCP Server (/mcp, Streamable HTTP)]
    end

    RemoteClients -->|HTTPS / Tailnet| ReverseProxy
    Attacker -.->|Port Scanning / SSRF / Probing| ReverseProxy
    LocalClients -->|LAN HTTP/WS| MBLCore
    ReverseProxy --> MBLCore
    WorkerNodes <-->|mTLS / HMAC Signed URLs| MBLCore
    StorageNodes <-->|Delta Sync / Smart Handover| MBLCore
    HAPlatform <-->|MQTT (TLS / User Auth)| MBLCore
    ClaudeAgent <-->|OAuth 2.1 PKCE / Scoped Token| MCPServer
    MCPServer --> MBLCore
    MBLCore --> SQLite
    MBLCore --> MediaStorage
    MBLCore --> HailoNPU
```

### Trust Zone Boundaries:
1. **Zone 0 (Host / Process):** MBL server process execution environment, filesystem data directory (`/var/lib/mbl` or `%APPDATA%\MBL`), and SQLite database files.
2. **Zone 1 (Trusted Local Network / Tailnet):** Authenticated LAN clients, Tailscale CGNAT IP addresses (`100.64.0.0/10`), registered transcode workers, and storage nodes.
3. **Zone 2 (Untrusted Internet / WAN):** External incoming connections through port-forwarding, reverse proxies, or public networks.
4. **Zone 3 (AI / MCP Boundary):** AI assistants and autonomous coding agents connecting via Model Context Protocol (Streamable HTTP or stdio).

---

## 2. Asset Classification & Sensitivity

| Asset | Sensitivity | Impact if Compromised | Protection Mechanism |
|---|---|---|---|
| **User Password Hashes** | High | Unauthorized account access | Argon2id with random salts (m_cost=65536, t_cost=3, p=4) |
| **Parental PINs** | High | Bypass of kids content rating restrictions | Argon2id hashing, rate limiting, and session verification |
| **TOTP Secrets** | High | 2FA bypass for WAN admin access | RFC 6238 Base32 secrets stored encrypted in database |
| **API & Worker Tokens** | High | Unauthorized API / MCP execution | SHA-256 token hashing; plaintext shown once upon minting |
| **Media Files on Disk** | Medium | Data theft or unauthorized file deletion | Strict canonical root path-traversal validation, sandboxing |
| **Database Records (WAL)** | High | Corruption, loss of metadata/watch state | Atomic transactions, foreign key cascades, automatic backups |
| **MCP Tool Capabilities** | High | Unauthorized administrative server mutations | Scoped permissions (`read`, `library`, `admin`), `dry_run` & `confirm` flags |

---

## 3. STRIDE Threat Analysis & Mitigations

### 3.1 Spoofing (Identity & Origin)
* **Tailscale Identity Spoofing:**
  - *Threat:* Malicious WAN client sets `Tailscale-User-Login` or similar headers to bypass authentication.
  - *Mitigation:* Strict loopback check. Header-based Tailscale authentication is rejected unless the request originates strictly from the local Unix socket / Windows named pipe and validates against `mbl-remote` LocalAPI.
* **X-Forwarded-For IP Spoofing:**
  - *Threat:* Remote attacker spoofing `X-Forwarded-For: 192.168.1.5` to trigger `admin_lan_only` bypass.
  - *Mitigation:* The rightmost untrusted IP is used unless the immediate peer IP matches configured `trusted_proxies`.
* **Worker & Node Impersonation:**
  - *Threat:* Rogue device on LAN joining as a GPU transcode worker.
  - *Mitigation:* One-time 6-digit numeric pairing codes with short expiration. Workers receive a cryptographically random HMAC secret and worker token.

### 3.2 Tampering
* **Media Stream URL Tampering:**
  - *Threat:* Manipulating part IDs or parameters to stream arbitrary host files.
  - *Mitigation:* Signed HMAC-SHA256 source tokens (`WorkerSourceTokenClaims`) bound to specific `part_id` and timestamp with max 60-second validity.
* **Database Injection (SQLi):**
  - *Threat:* Injecting malicious SQL in search queries or metadata fields.
  - *Mitigation:* 100% parameterized queries via `sqlx`. Zero dynamic string concatenation for SQL statements. FTS5 search queries use structured MATCH parameters.

### 3.3 Repudiation
* **Admin & MCP Audit Logging:**
  - *Threat:* Malicious or runaway AI agent performing deletions without traceability.
  - *Mitigation:* Comprehensive `mcp_audit` log recording timestamp, actor, token ID, client name, tool name, sanitized arguments, execution outcome, and duration. 90-day retention with automatic pruning.
* **Webhook Signatures:**
  - *Threat:* Forged webhook requests triggering unintended smart home actions.
  - *Mitigation:* HMAC-SHA256 signature calculated over request body and included in `X-MBL-Signature` header.

### 3.4 Information Disclosure
* **Path Traversal outside Library Roots:**
  - *Threat:* Exploiting `/api/v1/stream/direct/{partId}`, `/api/v1/images/*`, or `/api/v1/system/browse` to read `/etc/shadow` or `C:\Windows`.
  - *Mitigation:* Component-wise canonicalization check (`mbl_service::paths::canonical_root_rules`). Every resolved file path must start with a registered library root path. Directory browsing (`/system/browse`) requires admin credentials and normalizes symlinks.
* **Timing Attacks:**
  - *Threat:* Side-channel timing analysis on API tokens or password verification.
  - *Mitigation:* Constant-time comparison using `subtle::ConstantTimeEq` or Argon2id/SHA-256 fixed-length hash verification.
* **Existence Leakage via 404 vs 403:**
  - *Threat:* Unprivileged user probing item IDs to discover titles in restricted adult or personal libraries.
  - *Mitigation:* REST API returns `404 Not Found` (rather than `403 Forbidden`) whenever an item exists but is outside the caller's permitted library ACL.

### 3.5 Denial of Service (DoS)
* **XML Entity Expansion & Quadratic Blowup (Billion Laughs):**
  - *Threat:* Malicious XMLTV or NFO files exhausting server memory.
  - *Mitigation:* `quick-xml` streaming parser without external DTD/entity expansion. Hard limits on payload size (max 50 MB for XMLTV, streaming chunked reads).
* **Transcode Resource Exhaustion:**
  - *Threat:* Multiple concurrent 4K transcodes overwhelming Pi 5 CPU or GPU RAM.
  - *Mitigation:* Max concurrency caps (`max_sessions`), ahead-throttling via SIGSTOP/SIGCONT, automatic 60-second inactivity timeouts killing orphaned FFmpeg processes, and disk quota enforcement.
* **Brute-Force & API Rate Limiting:**
  - *Threat:* Credential stuffing against `/api/v1/auth/login` or flooding MCP endpoints.
  - *Mitigation:* Sliding window rate limiter (max 5 failed login attempts per IP/username per 15 minutes with `Retry-After`; MCP token rate limiting at 120 calls/min and expensive operations capped at 2/min).

### 3.6 Elevation of Privilege
* **MCP Scope Escalation:**
  - *Threat:* Token minted with `read` scope attempting to call `scan_library`, `delete_user`, or `create_webhook`.
  - *Mitigation:* Two-tier authorization check: (1) Caller scope verification in MCP layer (`caller.require(Scope::Library)`), and (2) Actor authorization check in the underlying service layer.
* **Parental Control PIN Bypass:**
  - *Threat:* Child profile attempting to remove rating ceilings or switch to admin profile.
  - *Mitigation:* Profile switching to unmanaged users requires Argon2id master password re-authentication. PIN verification is handled server-side before issuing profile session tokens.

---

## 4. Security Verification Checklist

- [x] OWASP Top 10 compliance audit across all REST API handlers.
- [x] Path traversal integration tests in test suites (`tests/stream.rs`, `tests/security.rs`).
- [x] Origin and Host header validation on WebSocket and `/mcp` endpoints.
- [x] Constant-time token and credential comparisons.
- [x] Zero network calls to third-party telemetry or cloud services without explicit user configuration.
- [x] Full security test suite integrated into CI.
