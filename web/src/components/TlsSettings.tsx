import { createResource, createSignal, Show } from "solid-js";
import client, { getStoredToken } from "../api/client";
import { apiErrorMessage } from "../api/errors";
import { useToast } from "../context/ToastContext";
import { Button } from "./ui";

type CertTab = "local_ca" | "tailscale" | "letsencrypt";

export default function TlsSettings() {
  const { showToast } = useToast();
  const defaultHost =
    typeof window !== "undefined" && window.location?.hostname
      ? `${window.location.hostname}, localhost, 127.0.0.1`
      : "localhost, 127.0.0.1";

  const [tab, setTab] = createSignal<CertTab>("local_ca");
  const [enabled, setEnabled] = createSignal(false);
  const [port, setPort] = createSignal(8443);
  const [hostnamesInput, setHostnamesInput] = createSignal(defaultHost);
  const [acmeDomain, setAcmeDomain] = createSignal("");
  const [acmeEmail, setAcmeEmail] = createSignal("");
  const [acmeCertPem, setAcmeCertPem] = createSignal("");
  const [acmeKeyPem, setAcmeKeyPem] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const [tls, { refetch }] = createResource(async () => {
    const res = await client.GET("/api/v1/system/tls");
    if (res.data) {
      setEnabled(res.data.enabled);
      setPort(res.data.port || 8443);
      if (res.data.hostnames.length > 0) {
        setHostnamesInput(res.data.hostnames.join(", "));
        if (res.data.mode === "letsencrypt") {
          setAcmeDomain(res.data.hostnames[0] || "");
        }
      }
      if (res.data.acmeEmail) {
        setAcmeEmail(res.data.acmeEmail);
      }
      if (
        res.data.mode === "local_ca" ||
        res.data.mode === "tailscale" ||
        res.data.mode === "letsencrypt"
      ) {
        setTab(res.data.mode);
      }
      return res.data;
    }
    return null;
  });

  const saveListener = async (e?: Event) => {
    e?.preventDefault();
    setBusy(true);
    try {
      const res = await client.PATCH("/api/v1/system/tls", {
        body: {
          enabled: enabled(),
          port: Number(port()),
        },
      });
      if (res.error || !res.data) {
        throw new Error(apiErrorMessage(res.error, "Could not update HTTPS settings."));
      }
      await refetch();
      showToast({
        title: "HTTPS Updated",
        message: res.data.enabled
          ? `HTTPS listener active on port ${res.data.port}.`
          : "HTTPS listener disabled.",
        type: "success",
      });
    } catch (err) {
      showToast({
        title: "HTTPS Update Failed",
        message: apiErrorMessage(err, "Failed to update HTTPS settings."),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  };

  const handleIssueLocalCa = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    try {
      const hostnames = hostnamesInput()
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      const res = await client.POST("/api/v1/system/tls/local-ca", {
        body: { hostnames },
      });
      if (res.error || !res.data) {
        throw new Error(apiErrorMessage(res.error, "Failed to generate local CA certificate."));
      }
      await refetch();
      showToast({
        title: "Local Certificate Issued",
        message: `Certificate issued for ${res.data.hostnames.join(", ")}.`,
        type: "success",
      });
    } catch (err) {
      showToast({
        title: "Certificate Error",
        message: apiErrorMessage(err, "Could not issue local certificate."),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  };

  const handleIssueTailscale = async () => {
    setBusy(true);
    try {
      const res = await client.POST("/api/v1/system/tls/tailscale");
      if (res.error || !res.data) {
        throw new Error(apiErrorMessage(res.error, "Failed to fetch Tailscale certificate."));
      }
      await refetch();
      showToast({
        title: "Tailscale Certificate Ready",
        message: `Certificate fetched for ${res.data.hostnames.join(", ")}.`,
        type: "success",
      });
    } catch (err) {
      showToast({
        title: "Tailscale Certificate Error",
        message: apiErrorMessage(err, "Could not fetch certificate from Tailscale."),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  };

  const handleIssueAcme = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    try {
      const res = await client.POST("/api/v1/system/tls/acme", {
        body: {
          domain: acmeDomain().trim(),
          email: acmeEmail().trim() || null,
          certPem: acmeCertPem().trim() || null,
          keyPem: acmeKeyPem().trim() || null,
        },
      });
      if (res.error || !res.data) {
        throw new Error(
          apiErrorMessage(res.error, "Failed to configure Let's Encrypt certificate."),
        );
      }
      setAcmeCertPem("");
      setAcmeKeyPem("");
      await refetch();
      showToast({
        title: "Let's Encrypt Certificate Ready",
        message: `Certificate configured for ${res.data.hostnames.join(", ")}.`,
        type: "success",
      });
    } catch (err) {
      showToast({
        title: "Let's Encrypt Error",
        message: apiErrorMessage(err, "Could not configure Let's Encrypt certificate."),
        type: "error",
      });
    } finally {
      setBusy(false);
    }
  };

  const downloadRootCa = async () => {
    try {
      const token = getStoredToken();
      const resp = await fetch("/api/v1/system/tls/ca.pem", {
        headers: token ? { Authorization: `Bearer ${token}` } : {},
        credentials: "include",
      });
      if (!resp.ok) {
        throw new Error("Local root CA is not generated yet.");
      }
      const blob = await resp.blob();
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "mbl-local-ca.crt";
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch (err) {
      showToast({
        title: "Download Failed",
        message: apiErrorMessage(err, "Could not download root CA certificate."),
        type: "error",
      });
    }
  };

  const inputStyle = {
    width: "100%",
    padding: "9px 12px",
    background: "var(--bg-base)",
    border: "1px solid var(--border)",
    "border-radius": "var(--radius-sm)",
    color: "var(--text-primary)",
    "font-size": "14px",
  } as const;

  const tabButtonStyle = (active: boolean) => ({
    padding: "8px 14px",
    background: active ? "var(--accent)" : "var(--bg-base)",
    color: active ? "#fff" : "var(--text-secondary)",
    border: "1px solid var(--border)",
    "border-radius": "var(--radius-sm)",
    cursor: "pointer",
    "font-size": "13px",
    "font-weight": "600",
  });

  return (
    <div
      style={{
        background: "var(--bg-surface)",
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-lg)",
        padding: "24px",
        "margin-bottom": "24px",
      }}
    >
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "16px",
        }}
      >
        <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
          <span style={{ "font-size": "24px" }}>🔒</span>
          <div>
            <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
              HTTPS & TLS Certificates
            </h2>
            <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
              Secure origins for Chromecast, Service Workers & remote access without browser flags
            </span>
          </div>
        </div>

        <Show
          when={tls()?.enabled && tls()?.hasCertificate}
          fallback={
            <span
              style={{
                padding: "4px 10px",
                background: "rgba(245, 158, 11, 0.12)",
                color: "var(--warning)",
                border: "1px solid var(--warning)",
                "border-radius": "999px",
                "font-size": "12px",
                "font-weight": "600",
              }}
            >
              {tls()?.hasCertificate ? "Certificate Ready (HTTPS Off)" : "HTTP Only"}
            </span>
          }
        >
          <span
            style={{
              padding: "4px 10px",
              background: "rgba(16, 185, 129, 0.12)",
              color: "var(--success)",
              border: "1px solid var(--success)",
              "border-radius": "999px",
              "font-size": "12px",
              "font-weight": "600",
            }}
          >
            ✓ HTTPS Active (:{tls()?.port})
          </span>
        </Show>
      </div>

      <Show when={tls()?.hasCertificate}>
        <div
          style={{
            padding: "10px 14px",
            background: "var(--bg-base)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-sm)",
            "font-size": "13px",
            "margin-bottom": "16px",
          }}
        >
          <strong>Active certificate ({tls()?.mode}):</strong> {tls()?.hostnames.join(", ") || "—"}
          <Show when={tls()?.expiresAt}>
            {(exp) => (
              <span style={{ color: "var(--text-secondary)", "margin-left": "8px" }}>
                (expires {new Date(exp() * 1000).toLocaleDateString()})
              </span>
            )}
          </Show>
        </div>
      </Show>

      {/* Mode selector tabs */}
      <div style={{ display: "flex", "flex-wrap": "wrap", gap: "8px", "margin-bottom": "16px" }}>
        <button
          type="button"
          style={tabButtonStyle(tab() === "local_ca")}
          onClick={() => setTab("local_ca")}
        >
          1. Local CA (LAN / IP)
        </button>
        <button
          type="button"
          style={tabButtonStyle(tab() === "tailscale")}
          onClick={() => setTab("tailscale")}
        >
          2. Tailscale (*.ts.net)
        </button>
        <button
          type="button"
          style={tabButtonStyle(tab() === "letsencrypt")}
          onClick={() => setTab("letsencrypt")}
        >
          3. Let's Encrypt (ACME)
        </button>
      </div>

      {/* Tab 1: Local CA */}
      <Show when={tab() === "local_ca"}>
        <form onSubmit={handleIssueLocalCa} style={{ "margin-bottom": "18px" }}>
          <p
            style={{
              "font-size": "13px",
              color: "var(--text-secondary)",
              margin: "0 0 10px 0",
            }}
          >
            Generates a self-managed Local Root CA and issues a server certificate valid for your
            LAN IP addresses and hostnames. Install the Root CA once on your device to get a green
            padlock on your LAN without needing a public domain.
          </p>
          <label
            for="tls-local-hosts"
            style={{
              display: "block",
              "font-size": "13px",
              "font-weight": "600",
              "margin-bottom": "6px",
            }}
          >
            Hostnames & IP Addresses (comma-separated)
          </label>
          <input
            id="tls-local-hosts"
            type="text"
            value={hostnamesInput()}
            onInput={(e) => setHostnamesInput(e.currentTarget.value)}
            placeholder="192.168.68.147, hallon.local, localhost"
            style={{ ...inputStyle, "margin-bottom": "10px" }}
          />
          <div style={{ display: "flex", "flex-wrap": "wrap", gap: "10px" }}>
            <Button type="submit" variant="primary" size="md" disabled={busy()}>
              {busy() ? "Generating…" : "Generate Local Certificate"}
            </Button>
            <Show when={tls()?.caAvailable}>
              <Button type="button" variant="secondary" size="md" onClick={downloadRootCa}>
                Download Root CA (mbl-local-ca.crt)
              </Button>
            </Show>
          </div>
        </form>
      </Show>

      {/* Tab 2: Tailscale */}
      <Show when={tab() === "tailscale"}>
        <div style={{ "margin-bottom": "18px" }}>
          <p
            style={{
              "font-size": "13px",
              color: "var(--text-secondary)",
              margin: "0 0 10px 0",
            }}
          >
            Fetches a publicly trusted Let's Encrypt certificate for your node's{" "}
            <code>*.ts.net</code> MagicDNS domain directly from the local Tailscale daemon. No root
            CA installation is needed on client devices.
          </p>
          <Button
            type="button"
            variant="primary"
            size="md"
            disabled={busy()}
            onClick={handleIssueTailscale}
          >
            {busy() ? "Fetching…" : "Fetch Tailscale Certificate"}
          </Button>
        </div>
      </Show>

      {/* Tab 3: Let's Encrypt (ACME) */}
      <Show when={tab() === "letsencrypt"}>
        <form onSubmit={handleIssueAcme} style={{ "margin-bottom": "18px" }}>
          <p
            style={{
              "font-size": "13px",
              color: "var(--text-secondary)",
              margin: "0 0 10px 0",
            }}
          >
            Use a Let's Encrypt certificate for your own public domain. MBL automatically loads{" "}
            <code>/etc/letsencrypt/live/&lt;domain&gt;/</code> if available, or you can paste the
            PEM fullchain and private key below.
          </p>
          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(auto-fit, minmax(220px, 1fr))",
              gap: "12px",
              "margin-bottom": "10px",
            }}
          >
            <div>
              <label
                for="tls-acme-domain"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Public Domain Name
              </label>
              <input
                id="tls-acme-domain"
                type="text"
                value={acmeDomain()}
                onInput={(e) => setAcmeDomain(e.currentTarget.value)}
                placeholder="media.example.com"
                required
                style={inputStyle}
              />
            </div>
            <div>
              <label
                for="tls-acme-email"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Contact Email (optional)
              </label>
              <input
                id="tls-acme-email"
                type="email"
                value={acmeEmail()}
                onInput={(e) => setAcmeEmail(e.currentTarget.value)}
                placeholder="admin@example.com"
                style={inputStyle}
              />
            </div>
          </div>

          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(auto-fit, minmax(220px, 1fr))",
              gap: "12px",
              "margin-bottom": "10px",
            }}
          >
            <div>
              <label
                for="tls-acme-cert"
                style={{
                  display: "block",
                  "font-size": "12px",
                  color: "var(--text-secondary)",
                  "margin-bottom": "4px",
                }}
              >
                Certificate Chain PEM (optional if in /etc/letsencrypt)
              </label>
              <textarea
                id="tls-acme-cert"
                rows={3}
                value={acmeCertPem()}
                onInput={(e) => setAcmeCertPem(e.currentTarget.value)}
                placeholder="-----BEGIN CERTIFICATE-----"
                style={{ ...inputStyle, "font-family": "var(--font-mono)", "font-size": "12px" }}
              />
            </div>
            <div>
              <label
                for="tls-acme-key"
                style={{
                  display: "block",
                  "font-size": "12px",
                  color: "var(--text-secondary)",
                  "margin-bottom": "4px",
                }}
              >
                Private Key PEM (optional if in /etc/letsencrypt)
              </label>
              <textarea
                id="tls-acme-key"
                rows={3}
                value={acmeKeyPem()}
                onInput={(e) => setAcmeKeyPem(e.currentTarget.value)}
                placeholder="-----BEGIN PRIVATE KEY-----"
                style={{ ...inputStyle, "font-family": "var(--font-mono)", "font-size": "12px" }}
              />
            </div>
          </div>

          <Button type="submit" variant="primary" size="md" disabled={busy()}>
            {busy() ? "Configuring…" : "Activate Let's Encrypt Certificate"}
          </Button>
        </form>
      </Show>

      {/* Listener enable & port */}
      <form
        onSubmit={saveListener}
        style={{
          "border-top": "1px solid var(--border)",
          "padding-top": "14px",
          display: "flex",
          "flex-wrap": "wrap",
          "align-items": "center",
          "justify-content": "space-between",
          gap: "12px",
        }}
      >
        <div style={{ display: "flex", "flex-wrap": "wrap", "align-items": "center", gap: "16px" }}>
          <label
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              cursor: "pointer",
              "font-size": "14px",
              "font-weight": "600",
            }}
          >
            <input
              type="checkbox"
              checked={enabled()}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
              style={{ width: "16px", height: "16px" }}
            />
            <span>Enable HTTPS Listener</span>
          </label>

          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <label for="tls-port" style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
              HTTPS Port:
            </label>
            <input
              id="tls-port"
              type="number"
              min={1024}
              max={65535}
              value={port()}
              onInput={(e) => setPort(Number(e.currentTarget.value))}
              style={{ ...inputStyle, width: "100px", padding: "6px 10px" }}
            />
          </div>
        </div>

        <Button type="submit" variant="secondary" size="md" disabled={busy()}>
          Save HTTPS Listener
        </Button>
      </form>
    </div>
  );
}
