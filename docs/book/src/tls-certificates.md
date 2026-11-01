# HTTPS, TLS & Certificate Management

Media Lounge Box (MBL) includes a built-in HTTPS listener and TLS certificate manager, configurable from the web interface at **Admin -> Server Settings -> HTTPS & TLS Certificates** or via the REST API (`/api/v1/system/tls`).

The HTTPS listener runs concurrently with HTTP (default HTTPS port `8443`, HTTP port `8420`) and reloads dynamically without restarting the server when certificates are generated, renewed, or changed.

---

## Certificate Modes

MBL supports three certificate modes tailored for home lab, remote, and public domain setups:

### Mode 1: MBL Local CA (Private LAN & Hostnames)

For private home networks accessing MBL via LAN IP (e.g., `192.168.68.147`) or mDNS hostname (e.g., `hallon.local`, `mbl.local`).

1. **How it works:**
   - MBL acts as its own Certificate Authority, generating a 10-year root certificate (**MBL Local CA**) and a 1-year server certificate.
   - The server certificate automatically includes Subject Alternative Names (SANs) for all local LAN IP addresses, machine hostnames, `localhost`, and `127.0.0.1`.
2. **Download Root CA:**
   - Click **Download Root CA (`.crt`)** in the web UI, or download directly via `GET /api/v1/system/tls/ca.pem`.
3. **Client Installation (One-time trust):**
   - **Linux / Raspberry Pi OS:**
     ```bash
     sudo cp mbl-local-ca.crt /usr/local/share/ca-certificates/
     sudo update-ca-certificates
     ```
     In Google Chrome or Chromium, navigate to `chrome://certificate-manager` -> **Local certificates** -> ensure operating system certificates are enabled, or import `mbl-local-ca.crt` under **Customised**.
   - **Windows:**
     Double-click `mbl-local-ca.crt` -> **Install Certificate** -> Choose **Local Machine** (or Current User) -> Select **Place all certificates in the following store** -> Click **Browse** and choose **Trusted Root Certification Authorities** (Betrodda rotcertifikatutfärdare) -> Complete wizard.
   - **macOS:**
     Double-click `mbl-local-ca.crt` to open **Keychain Access** -> Move to **System** keychain -> Double-click certificate -> Expand **Trust** -> Set **When using this certificate** to **Always Trust**.
   - **iOS / iPadOS:**
     AirDrop or download `mbl-local-ca.crt` -> Install Profile in **Settings** -> Go to **Settings -> General -> About -> Certificate Trust Settings** -> Enable full trust for MBL Local CA.
   - **Android:**
     Settings -> **Security** -> **More security settings** -> **Encryption & credentials** -> **Install a certificate** -> **CA certificate** -> Select `mbl-local-ca.crt`.

---

### Mode 2: Tailscale HTTPS (`*.ts.net`)

For users with Tailscale enabled on their server.

1. **How it works:**
   - MBL communicates directly with the local Tailscale daemon (`tailscaled`) via its LocalAPI Unix socket or Windows named pipe.
   - It fetches an officially trusted Let's Encrypt certificate pair for the node's Tailscale domain (e.g. `your-node.tailnet-xyz.ts.net`).
2. **Benefits:**
   - 100% publicly trusted Let's Encrypt certificate without opening any ports on your router or firewall.
   - Automatically trusted on all phones, laptops, and tablets connected to your Tailnet without installing custom CA root certificates.

---

### Mode 3: ACME / Let's Encrypt & Custom PEM

For users with their own public domain name pointed to their home server (via DDNS, Cloudflare, Traefik, or Nginx).

1. **Certbot / ACME Auto-Detect:**
   - If Certbot is running on the host, MBL can load `/etc/letsencrypt/live/<your-domain>/fullchain.pem` and `privkey.pem`.
2. **Custom PEM Import:**
   - Paste any valid PEM-encoded certificate chain and unencrypted private key directly into the settings form.

---

## Client Compatibility: Chromecast & Local CAs

> [!NOTE]
> **Google Chromecast Hardware Root Store:**
> Google Cast devices (Chromecast with Google TV, Nest Hub, Cast-enabled smart TVs) run proprietary Google firmware with a fixed root certificate store that does **not** allow importing custom local root CAs.
> 
> MBL handles this automatically: when you cast from an HTTPS session (`https://192.168.x.x:8443`), MBL routes the media stream to the Chromecast using HTTP on the LAN port (`http://192.168.x.x:8420`). The web app and API remain securely encrypted over HTTPS while the Chromecast plays smoothly without SSL handshake errors.

---

## Why Enable HTTPS on LAN?

Even on a private home network, running HTTPS provides essential browser capabilities:
1. **Progressive Web App (PWA):** Enables installing MBL as an app on Android, iOS, Windows, and macOS with offline caching.
2. **Web Cryptography & Secure Context:** Browsers require HTTPS to enable hardware-backed credential storage and clipboard operations.
3. **No Mixed Content Warnings:** Safe embedding and communication with secure online plugins and WebTV streams.
