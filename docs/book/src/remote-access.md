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

## Fair WAN Bandwidth Sharing

When external users stream simultaneously over WAN, MBL enforces a strict bandwidth cap (`wan_upload_limit_kbps`) with dynamic fair-share rebalancing. If upload bandwidth is saturated, active transcode streams are downgraded gracefully to prevent buffering for other users.
