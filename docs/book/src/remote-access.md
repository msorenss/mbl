# Remote Access, Tailscale & Network Security

MBL eliminates the need for complex port-forwarding and exposure to public internet attacks by deeply integrating with **Tailscale** and zero-trust networking.

## Tailscale LocalAPI Integration (`mbl-remote`)

MBL communicates with the local Tailscale daemon via Unix domain socket (`/var/run/tailscale/tailscaled.sock`) or Windows named pipe (`\\.\pipe\ProtectedPrefix\Administrators\Tailscale\tailscaled`):

1. **Automatic Node Detection:** Reads the server's Tailscale IP (`100.64.0.0/10` CGNAT range) and MagicDNS FQDN.
2. **Zero-Configuration Authentication:** Clients accessing the server over a Tailnet can authenticate using Tailscale identity headers verified against the local daemon (`POST /api/v1/auth/tailscale`), preventing spoofing.
3. **No Inbound Open Ports:** The server remains invisible to port scanners on the public internet.

## Network Classification & Admin LAN Guard

Every incoming request is categorized into one of four trust tiers based on source IP and trusted proxy headers:
- `Loopback` (`127.0.0.1`, `::1`)
- `Lan` (RFC 1918 `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`)
- `Tailscale` (`100.64.0.0/10` CGNAT)
- `Wan` (Public Internet)

### Admin LAN-Only Policy
By default, administrative endpoints (`/api/v1/users`, `/api/v1/settings`, `/api/v1/mcp`) and dangerous mutations are strictly barred on `Wan` connections unless explicitly relaxed by the admin with mandatory TOTP 2FA.

## Network diagnostics

**Administration → Remote access → Run diagnostics** checks, in order:

| Check | What it does |
|---|---|
| Public address | Asks `api.ipify.org` which address the internet sees. This is the only outside service the diagnostic contacts, and only when an administrator runs it. |
| Router | Asks the router for its WAN address over UPnP (IGD `GetExternalIPAddress`). A WAN address in `100.64.0.0/10`, or one that differs from the public address, means CGNAT or double NAT: port forwarding on your router cannot work. If UPnP is off, compare the WAN address on the router's admin page yourself. |
| DNS | Resolves the certificate host names and the host in `server.public_url`, and compares them with the public address. A mismatch usually means the dynamic DNS client has not updated. |
| HTTPS port | Connects to the public address on the HTTPS port. From inside the home network this needs NAT loopback on the router; if it fails, test from mobile data before concluding that the forward is missing. |
| Port 80 | For Let's Encrypt certificates: renewal needs external port 80 forwarded to the server (port 8081, or through a reverse proxy). |
| Certificate | HTTPS on, certificate present and days left. A local-CA certificate works but must be installed on every device outside the home. |
| Tailscale | Whether Tailscale runs and the tailnet name of the server. |

The result says *Reachable from the internet* only when the HTTPS port actually answered on the public address. The same report is available to AI agents through the MCP tool `run_reachability_test`.

## Fair WAN Bandwidth Sharing

When external users stream simultaneously over WAN, MBL enforces a strict bandwidth cap (`wan_upload_limit_kbps`) with dynamic fair-share rebalancing. If upload bandwidth is saturated, active transcode streams are downgraded gracefully to prevent buffering for other users.

---

## HTTPS & Encrypted Connections

For details on generating Local CA certificates, provisioning Let's Encrypt certificates via Tailscale LocalAPI, or loading custom ACME PEM certificates, see the dedicated [HTTPS, TLS & Certificate Management](tls-certificates.md) guide.
