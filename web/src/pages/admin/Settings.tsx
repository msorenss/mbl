import { A } from "@solidjs/router";
import { createResource, createSignal, Show } from "solid-js";
import client from "../../api/client";
import CastDiagnostics from "../../components/CastDiagnostics";
import TlsSettings from "../../components/TlsSettings";
import { Button } from "../../components/ui";
import { useToast } from "../../context/ToastContext";
import { useI18n } from "../../i18n";

export default function Settings() {
  const { showToast } = useToast();
  const { t } = useI18n();

  // Server & TMDB state
  const [serverName, setServerName] = createSignal("Media Lounge Box");
  const [apiKey, setApiKey] = createSignal("");
  const [showKey, setShowKey] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [testResult, setTestResult] = createSignal<{ success: boolean; message: string } | null>(
    null,
  );

  // Library Scanning & Automation state
  const [autoScanOnChange, setAutoScanOnChange] = createSignal(true);
  const [periodicInterval, setPeriodicInterval] = createSignal(60);
  const [autoFetchMissingMetadata, setAutoFetchMissingMetadata] = createSignal(true);

  // Home Assistant & MQTT state
  const [mqttEnabled, setMqttEnabled] = createSignal(false);
  const [mqttBrokerUrl, setMqttBrokerUrl] = createSignal("mqtt://localhost:1883");
  const [mqttUsername, setMqttUsername] = createSignal("");
  const [mqttPassword, setMqttPassword] = createSignal("");
  const [showMqttPassword, setShowMqttPassword] = createSignal(false);
  const [mqttTopicPrefix, setMqttTopicPrefix] = createSignal("mbl/media_lounge_box");

  // DLNA state
  const [dlnaEnabled, setDlnaEnabled] = createSignal(true);
  const [dlnaFriendlyName, setDlnaFriendlyName] = createSignal("Media Lounge Box");

  const [serverInfo] = createResource(async () => {
    try {
      const res = await client.GET("/api/v1/system/info");
      return res.data;
    } catch {
      return undefined;
    }
  });

  const [settings, { refetch }] = createResource(async () => {
    const res = await client.GET("/api/v1/system/settings");
    if (res.data) {
      setServerName(res.data.serverName || "Media Lounge Box");
      setApiKey(res.data.tmdbApiKey || "");
      setAutoScanOnChange(res.data.autoScanOnChange ?? true);
      setPeriodicInterval(res.data.periodicScanIntervalMinutes ?? 60);
      setAutoFetchMissingMetadata(res.data.autoFetchMissingMetadata ?? true);
      setMqttEnabled(Boolean(res.data.mqttEnabled));
      setMqttBrokerUrl(res.data.mqttBrokerUrl || "mqtt://localhost:1883");
      setMqttUsername(res.data.mqttUsername || "");
      setMqttTopicPrefix(res.data.mqttTopicPrefix || "mbl/media_lounge_box");
      setDlnaEnabled(res.data.dlnaEnabled ?? true);
      setDlnaFriendlyName(res.data.dlnaFriendlyName || "Media Lounge Box");
      return res.data;
    }
    throw new Error("Could not load system settings.");
  });

  const handleSave = async (e?: Event) => {
    e?.preventDefault();
    setSaving(true);
    setTestResult(null);
    try {
      const res = await client.PATCH("/api/v1/system/settings", {
        body: {
          serverName: serverName().trim() || undefined,
          tmdbApiKey: apiKey().trim(),
          autoScanOnChange: autoScanOnChange(),
          periodicScanIntervalMinutes: Number(periodicInterval()),
          autoFetchMissingMetadata: autoFetchMissingMetadata(),
          mqttEnabled: mqttEnabled(),
          mqttBrokerUrl: mqttBrokerUrl().trim() || undefined,
          mqttUsername: mqttUsername().trim() || undefined,
          mqttPassword: mqttPassword().trim() || undefined,
          mqttTopicPrefix: mqttTopicPrefix().trim() || undefined,
          dlnaEnabled: dlnaEnabled(),
          dlnaFriendlyName: dlnaFriendlyName().trim() || undefined,
        },
      });
      if (res.data) {
        await refetch();
        showToast({
          title: "Settings Saved",
          message: "Media Lounge Box server settings successfully saved and applied.",
          type: "success",
        });
      } else {
        throw new Error("Could not save settings.");
      }
    } catch (err: unknown) {
      showToast({
        title: "Save Failed",
        message: err instanceof Error ? err.message : "Failed to update settings.",
        type: "error",
      });
    } finally {
      setSaving(false);
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const res = await client.POST("/api/v1/system/settings/test-tmdb", {
        body: { apiKey: apiKey().trim() || undefined },
      });
      if (res.data?.success) {
        setTestResult({ success: true, message: res.data.message });
        showToast({
          title: "Connection Successful",
          message: res.data.message,
          type: "success",
        });
      } else {
        const errorMsg =
          (res.error as unknown as { message?: string })?.message || "TMDB connection test failed.";
        setTestResult({ success: false, message: errorMsg });
        showToast({
          title: "Connection Failed",
          message: errorMsg,
          type: "error",
        });
      }
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Failed to connect to TMDB.";
      setTestResult({ success: false, message: msg });
      showToast({
        title: "Connection Failed",
        message: msg,
        type: "error",
      });
    } finally {
      setTesting(false);
    }
  };

  const handleClearTmdb = async () => {
    setApiKey("");
    await handleSave();
  };

  return (
    <div style={{ "max-width": "860px", padding: "16px 0" }}>
      <div style={{ "margin-bottom": "28px" }}>
        <h1 style={{ margin: "0 0 6px 0", "font-size": "26px", "font-weight": "800" }}>
          Server Settings — Media Lounge Box
        </h1>
        <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
          Configure external metadata providers, Home Assistant MQTT automation, DLNA streaming, and
          AI integrations.
        </p>
      </div>

      <Show when={settings.error}>
        <div
          role="alert"
          style={{
            padding: "12px 16px",
            background: "rgba(239, 68, 68, 0.1)",
            border: "1px solid var(--danger)",
            "border-radius": "var(--radius-md)",
            color: "var(--danger)",
            "margin-bottom": "20px",
          }}
        >
          Could not load settings. Please refresh the page.
        </div>
      </Show>

      {/* System & Version Information Card */}
      <div
        style={{
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-md)",
          padding: "16px 20px",
          "margin-bottom": "24px",
          display: "flex",
          "flex-direction": "column",
          gap: "10px",
        }}
      >
        <div
          style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
            <span style={{ "font-size": "20px" }}>ℹ️</span>
            <strong style={{ "font-size": "15px" }}>System- & Versionsinformation</strong>
          </div>
          <span
            style={{
              background: "var(--accent-light, rgba(59, 130, 246, 0.15))",
              color: "var(--accent)",
              padding: "2px 8px",
              "border-radius": "var(--radius-sm)",
              "font-size": "12px",
              "font-weight": "600",
            }}
          >
            v{serverInfo()?.version || __MBL_VERSION__}
          </span>
        </div>
        <div
          style={{
            display: "grid",
            "grid-template-columns": "repeat(auto-fit, minmax(180px, 1fr))",
            gap: "12px",
            "font-size": "13px",
            "border-top": "1px solid var(--border)",
            "padding-top": "10px",
          }}
        >
          <div>
            <div style={{ color: "var(--text-muted)", "font-size": "11px", "font-weight": "600" }}>
              SERVERVERSION
            </div>
            <div>v{serverInfo()?.version || __MBL_VERSION__}</div>
          </div>
          <div>
            <div style={{ color: "var(--text-muted)", "font-size": "11px", "font-weight": "600" }}>
              GIT COMMIT / BYGGVERSION
            </div>
            <div style={{ "font-family": "monospace", "font-size": "12px" }}>
              {serverInfo()?.gitCommit || __MBL_GIT_COMMIT__}
            </div>
          </div>
          <div>
            <div style={{ color: "var(--text-muted)", "font-size": "11px", "font-weight": "600" }}>
              BYGGDATUM
            </div>
            <div style={{ "font-size": "12px" }}>
              {serverInfo()?.buildTime || __MBL_BUILD_DATE__}
            </div>
          </div>
          <div>
            <div style={{ color: "var(--text-muted)", "font-size": "11px", "font-weight": "600" }}>
              PLATTFORM & NPU
            </div>
            <div style={{ "text-transform": "capitalize" }}>
              {serverInfo()?.os || "linux"} {serverInfo()?.arch || "arm64"} (
              {serverInfo()?.platformProfile || "standard"})
              {serverInfo()?.npuProfile && serverInfo()?.npuProfile !== "none"
                ? ` • ${serverInfo()?.npuProfile}`
                : ""}
            </div>
          </div>
        </div>
      </div>

      {/* Central Admin Hub Quick Navigation */}
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(auto-fit, minmax(130px, 1fr))",
          gap: "10px",
          "margin-bottom": "24px",
        }}
      >
        <A
          href="/admin/users"
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            padding: "12px 8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "text-decoration": "none",
            gap: "6px",
            "font-size": "13px",
            "font-weight": "600",
          }}
        >
          <span style={{ "font-size": "20px" }}>👥</span>
          <span>Users & PINs</span>
        </A>
        <A
          href="/admin/ai-agents"
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            padding: "12px 8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "text-decoration": "none",
            gap: "6px",
            "font-size": "13px",
            "font-weight": "600",
          }}
        >
          <span style={{ "font-size": "20px" }}>🤖</span>
          <span>AI Agents</span>
        </A>
        <A
          href="/admin/hardware"
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            padding: "12px 8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "text-decoration": "none",
            gap: "6px",
            "font-size": "13px",
            "font-weight": "600",
          }}
        >
          <span style={{ "font-size": "20px" }}>⚡</span>
          <span>Hardware & NPU</span>
        </A>
        <A
          href="/admin/remote"
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            padding: "12px 8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "text-decoration": "none",
            gap: "6px",
            "font-size": "13px",
            "font-weight": "600",
          }}
        >
          <span style={{ "font-size": "20px" }}>🌐</span>
          <span>Remote Access</span>
        </A>
        <A
          href="/admin/workers"
          style={{
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            padding: "12px 8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "text-decoration": "none",
            gap: "6px",
            "font-size": "13px",
            "font-weight": "600",
          }}
        >
          <span style={{ "font-size": "20px" }}>🖥️</span>
          <span>Distributed Nodes</span>
        </A>
      </div>

      {/* 1. Server Branding & TMDB Section */}
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
            "margin-bottom": "14px",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "24px" }}>🎬</span>
            <div>
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                Server & TMDB Metadata
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                Server identification and The Movie Database metadata provider
              </span>
            </div>
          </div>

          <span
            style={{
              padding: "4px 10px",
              "border-radius": "999px",
              "font-size": "12px",
              "font-weight": "600",
              background: settings()?.tmdbConfigured
                ? "rgba(34, 197, 94, 0.15)"
                : "rgba(156, 163, 175, 0.15)",
              color: settings()?.tmdbConfigured ? "var(--success)" : "var(--text-muted)",
              border: `1px solid ${settings()?.tmdbConfigured ? "var(--success)" : "var(--border)"}`,
            }}
          >
            {settings()?.tmdbConfigured ? "● TMDB Configured" : "○ Local NFO only"}
          </span>
        </div>

        <form onSubmit={handleSave}>
          <div style={{ "margin-bottom": "16px" }}>
            <label
              for="server-name"
              style={{
                display: "block",
                "font-size": "13px",
                "font-weight": "600",
                "margin-bottom": "6px",
              }}
            >
              Server Name
            </label>
            <input
              id="server-name"
              type="text"
              value={serverName()}
              onInput={(e) => setServerName(e.currentTarget.value)}
              placeholder="Media Lounge Box"
              style={{
                width: "100%",
                padding: "9px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
              }}
            />
          </div>

          <div style={{ "margin-bottom": "14px" }}>
            <label
              for="tmdb-key"
              style={{
                display: "block",
                "font-size": "13px",
                "font-weight": "600",
                "margin-bottom": "6px",
              }}
            >
              TMDB API Key (v3 Key or v4 Read Access Token)
            </label>
            <div style={{ display: "flex", gap: "8px" }}>
              <input
                id="tmdb-key"
                type={showKey() ? "text" : "password"}
                value={apiKey()}
                onInput={(e) => setApiKey(e.currentTarget.value)}
                placeholder="e.g. 3a7b8c... or v4 JWT token"
                spellcheck={false}
                autocomplete="off"
                style={{
                  flex: 1,
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "14px",
                  "font-family": "monospace",
                }}
              />
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setShowKey(!showKey())}
                title={showKey() ? "Hide API key" : "Show API key"}
              >
                {showKey() ? "Hide" : "Show"}
              </Button>
            </div>
            <div style={{ "margin-top": "6px", "font-size": "12px", color: "var(--text-muted)" }}>
              Need an API key? Get one for free at{" "}
              <a
                href="https://www.themoviedb.org/settings/api"
                target="_blank"
                rel="noreferrer"
                style={{ color: "var(--accent)", "text-decoration": "underline" }}
              >
                themoviedb.org/settings/api
              </a>
              .
            </div>
          </div>

          <Show when={testResult()}>
            <div
              role="alert"
              style={{
                padding: "10px 14px",
                "border-radius": "var(--radius-sm)",
                "font-size": "13px",
                "margin-bottom": "14px",
                background: testResult()?.success
                  ? "rgba(34, 197, 94, 0.1)"
                  : "rgba(239, 68, 68, 0.1)",
                color: testResult()?.success ? "var(--success)" : "var(--danger)",
                border: `1px solid ${testResult()?.success ? "var(--success)" : "var(--danger)"}`,
              }}
            >
              {testResult()?.success ? "✓ " : "✕ "}
              {testResult()?.message}
            </div>
          </Show>

          <div style={{ display: "flex", gap: "10px", "align-items": "center" }}>
            <Button type="submit" variant="primary" size="md" disabled={saving() || testing()}>
              {saving() ? "Saving…" : "Save Settings"}
            </Button>
            <Button
              type="button"
              variant="outline"
              size="md"
              disabled={saving() || testing() || !apiKey().trim()}
              onClick={handleTest}
            >
              {testing() ? "Testing…" : "Test Connection"}
            </Button>
            <Show when={settings()?.tmdbConfigured}>
              <Button
                type="button"
                variant="danger"
                size="md"
                disabled={saving() || testing()}
                onClick={handleClearTmdb}
              >
                Remove Key
              </Button>
            </Show>
          </div>
        </form>
      </div>

      {/* 1.5. HTTPS & TLS Certificates Section */}
      <TlsSettings />

      {/* 2. Library Scanning & Automation Section */}
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
            "margin-bottom": "14px",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "24px" }}>🔄</span>
            <div>
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                {t("libraryScanningTitle")}
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                {t("libraryScanningDesc")}
              </span>
            </div>
          </div>
        </div>

        <p
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "line-height": "1.6",
            "margin-bottom": "16px",
          }}
        >
          {t("autoScanOnChangeHelp")}
        </p>

        <form onSubmit={handleSave}>
          <div style={{ "margin-bottom": "16px" }}>
            <label
              style={{
                display: "flex",
                "align-items": "flex-start",
                gap: "10px",
                cursor: "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={autoScanOnChange()}
                onChange={(e) => setAutoScanOnChange(e.currentTarget.checked)}
                style={{ width: "18px", height: "18px", "margin-top": "2px" }}
              />
              <div>
                <span style={{ "font-size": "14px", "font-weight": "600", display: "block" }}>
                  {t("autoScanOnChange")}
                </span>
                <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                  {t("autoScanOnChangeHelp")}
                </span>
              </div>
            </label>
          </div>

          <div style={{ "margin-bottom": "16px" }}>
            <label
              style={{
                display: "flex",
                "align-items": "flex-start",
                gap: "10px",
                cursor: "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={autoFetchMissingMetadata()}
                onChange={(e) => setAutoFetchMissingMetadata(e.currentTarget.checked)}
                style={{ width: "18px", height: "18px", "margin-top": "2px" }}
              />
              <div>
                <span style={{ "font-size": "14px", "font-weight": "600", display: "block" }}>
                  {t("autoFetchMissingMetadata")}
                </span>
                <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                  {t("autoFetchMissingMetadataHelp")}
                </span>
              </div>
            </label>
          </div>

          <div style={{ "margin-bottom": "20px", "max-width": "360px" }}>
            <label
              for="periodic-interval"
              style={{
                display: "block",
                "font-size": "13px",
                "font-weight": "600",
                "margin-bottom": "6px",
              }}
            >
              {t("periodicScanInterval")}
            </label>
            <select
              id="periodic-interval"
              value={periodicInterval()}
              onChange={(e) => setPeriodicInterval(Number(e.currentTarget.value))}
              style={{
                width: "100%",
                padding: "9px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
              }}
            >
              <option value="0">{t("periodicScanDisabled")}</option>
              <option value="15">{t("periodicScan15m")}</option>
              <option value="30">{t("periodicScan30m")}</option>
              <option value="60">{t("periodicScan1h")}</option>
              <option value="360">{t("periodicScan6h")}</option>
              <option value="1440">{t("periodicScan24h")}</option>
            </select>
          </div>

          <Button type="submit" variant="primary" size="md" disabled={saving()}>
            {saving() ? "Saving…" : "Save Automation Settings"}
          </Button>
        </form>
      </div>

      {/* 3. Home Assistant & MQTT Automation Section */}
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
            "margin-bottom": "14px",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "24px" }}>🏠</span>
            <div>
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                Home Assistant & MQTT Automation
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                Auto-discovery of media player entities, live telemetry & incoming control commands
              </span>
            </div>
          </div>

          <label
            style={{ display: "flex", "align-items": "center", gap: "8px", cursor: "pointer" }}
          >
            <input
              type="checkbox"
              checked={mqttEnabled()}
              onChange={(e) => setMqttEnabled(e.currentTarget.checked)}
              style={{ width: "18px", height: "18px" }}
            />
            <span style={{ "font-size": "13px", "font-weight": "600" }}>
              {mqttEnabled() ? "Enabled" : "Disabled"}
            </span>
          </label>
        </div>

        <p
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "line-height": "1.6",
            "margin-bottom": "16px",
          }}
        >
          Connect Media Lounge Box to your Home Assistant instance via MQTT Discovery. Home
          Assistant automatically discovers all active players as <code>media_player</code>{" "}
          entities, receives real-time telemetry (playing state, title, series/episode, position,
          duration, volume), and can send playback commands (<code>PLAY</code>, <code>PAUSE</code>,{" "}
          <code>STOP</code>, <code>SEEK &lt;seconds&gt;</code>, <code>VOLUME &lt;0-100&gt;</code>).
        </p>

        <form onSubmit={handleSave}>
          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(auto-fit, minmax(240px, 1fr))",
              gap: "14px",
              "margin-bottom": "16px",
            }}
          >
            <div>
              <label
                for="mqtt-broker"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Broker URL
              </label>
              <input
                id="mqtt-broker"
                type="text"
                value={mqttBrokerUrl()}
                onInput={(e) => setMqttBrokerUrl(e.currentTarget.value)}
                placeholder="mqtt://192.168.1.50:1883"
                disabled={!mqttEnabled()}
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "13px",
                  "font-family": "monospace",
                }}
              />
            </div>

            <div>
              <label
                for="mqtt-prefix"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Topic Prefix
              </label>
              <input
                id="mqtt-prefix"
                type="text"
                value={mqttTopicPrefix()}
                onInput={(e) => setMqttTopicPrefix(e.currentTarget.value)}
                placeholder="mbl/media_lounge_box"
                disabled={!mqttEnabled()}
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "13px",
                  "font-family": "monospace",
                }}
              />
            </div>

            <div>
              <label
                for="mqtt-user"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Username (optional)
              </label>
              <input
                id="mqtt-user"
                type="text"
                value={mqttUsername()}
                onInput={(e) => setMqttUsername(e.currentTarget.value)}
                placeholder="homeassistant"
                disabled={!mqttEnabled()}
                style={{
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                  "font-size": "13px",
                }}
              />
            </div>

            <div>
              <label
                for="mqtt-pass"
                style={{
                  display: "block",
                  "font-size": "13px",
                  "font-weight": "600",
                  "margin-bottom": "6px",
                }}
              >
                Password (optional)
              </label>
              <div style={{ display: "flex", gap: "6px" }}>
                <input
                  id="mqtt-pass"
                  type={showMqttPassword() ? "text" : "password"}
                  value={mqttPassword()}
                  onInput={(e) => setMqttPassword(e.currentTarget.value)}
                  placeholder="••••••••"
                  disabled={!mqttEnabled()}
                  style={{
                    flex: 1,
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                    "font-size": "13px",
                  }}
                />
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => setShowMqttPassword(!showMqttPassword())}
                  disabled={!mqttEnabled()}
                >
                  {showMqttPassword() ? "Hide" : "Show"}
                </Button>
              </div>
            </div>
          </div>

          <Button type="submit" variant="primary" size="md" disabled={saving()}>
            {saving() ? "Saving…" : "Save MQTT Settings"}
          </Button>
        </form>
      </div>

      {/* 3. DLNA & UPnP Media Server Section */}
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
            "margin-bottom": "14px",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "24px" }}>📺</span>
            <div>
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                DLNA & UPnP Media Server
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                SSDP local network broadcasting and Smart TV ContentDirectory
              </span>
            </div>
          </div>

          <label
            style={{ display: "flex", "align-items": "center", gap: "8px", cursor: "pointer" }}
          >
            <input
              type="checkbox"
              checked={dlnaEnabled()}
              onChange={(e) => setDlnaEnabled(e.currentTarget.checked)}
              style={{ width: "18px", height: "18px" }}
            />
            <span style={{ "font-size": "13px", "font-weight": "600" }}>
              {dlnaEnabled() ? "Enabled" : "Disabled"}
            </span>
          </label>
        </div>

        <p
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "line-height": "1.6",
            "margin-bottom": "16px",
          }}
        >
          When enabled, MBL announces itself via UPnP SSDP on port 1900 and serves XML
          ContentDirectory endpoints. Supported smart TVs (Samsung Tizen, LG webOS, Sony Bravia) and
          DLNA media renderers can browse movies, TV shows, and music natively without installing
          client applications.
        </p>

        <form onSubmit={handleSave}>
          <div style={{ "margin-bottom": "16px" }}>
            <label
              for="dlna-name"
              style={{
                display: "block",
                "font-size": "13px",
                "font-weight": "600",
                "margin-bottom": "6px",
              }}
            >
              DLNA Friendly Name
            </label>
            <input
              id="dlna-name"
              type="text"
              value={dlnaFriendlyName()}
              onInput={(e) => setDlnaFriendlyName(e.currentTarget.value)}
              placeholder="Media Lounge Box"
              disabled={!dlnaEnabled()}
              style={{
                width: "100%",
                padding: "9px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
              }}
            />
          </div>

          <Button type="submit" variant="primary" size="md" disabled={saving()}>
            {saving() ? "Saving…" : "Save DLNA Settings"}
          </Button>
        </form>
      </div>

      {/* 4. AI & MCP Integrations Card */}
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
            "margin-bottom": "14px",
          }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <span style={{ "font-size": "24px" }}>🤖</span>
            <div>
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                AI Agents & MCP Protocols
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                Google Antigravity, Hermes Agent (Co-watching Companion), OpenClaw & Token Wizard
              </span>
            </div>
          </div>

          <a
            href="/admin/agents"
            style={{
              padding: "6px 14px",
              background: "var(--accent)",
              color: "#fff",
              "border-radius": "var(--radius-sm)",
              "font-size": "13px",
              "font-weight": "600",
              "text-decoration": "none",
            }}
          >
            Manage AI Agents →
          </a>
        </div>

        <p
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "line-height": "1.6",
            margin: 0,
          }}
        >
          MBL implements 78 scoped Model Context Protocol (MCP) tools. You can connect{" "}
          <strong>Google Antigravity</strong>, pair <strong>Hermes Agent</strong> for real-time
          co-watching dialogue cues and commentary, or configure <strong>OpenClaw</strong> with
          scoped Personal Access Tokens. Use the AI Agents panel to create tokens and copy one-click
          client snippets.
        </p>
      </div>

      {/* 5. Diagnostics Section Card */}
      <div
        style={{
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-lg)",
          padding: "24px",
        }}
      >
        <div
          style={{ display: "flex", "align-items": "center", gap: "10px", "margin-bottom": "14px" }}
        >
          <span style={{ "font-size": "24px" }}>📡</span>
          <div>
            <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
              Cast & Stream Diagnostics
            </h2>
            <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
              Inspect live receiver handshakes, telemetry and connection logs
            </span>
          </div>
        </div>
        <p style={{ "font-size": "13px", color: "var(--text-secondary)", "margin-bottom": "14px" }}>
          Use the log below to inspect Chromecast sender/receiver events and server playback
          exchanges.
        </p>
        <CastDiagnostics />
      </div>
    </div>
  );
}
