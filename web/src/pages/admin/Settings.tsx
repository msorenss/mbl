import { createResource, createSignal, Show } from "solid-js";
import client from "../../api/client";
import CastDiagnostics from "../../components/CastDiagnostics";
import { Button } from "../../components/ui";
import { useToast } from "../../context/ToastContext";

export default function Settings() {
  const { showToast } = useToast();
  const [apiKey, setApiKey] = createSignal("");
  const [showKey, setShowKey] = createSignal(false);
  const [saving, setSaving] = createSignal(false);
  const [testing, setTesting] = createSignal(false);
  const [testResult, setTestResult] = createSignal<{ success: boolean; message: string } | null>(
    null,
  );

  const [settings, { refetch }] = createResource(async () => {
    const res = await client.GET("/api/v1/system/settings");
    if (res.data) {
      setApiKey(res.data.tmdbApiKey || "");
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
        body: { tmdbApiKey: apiKey().trim() },
      });
      if (res.data) {
        await refetch();
        showToast({
          title: "Settings Saved",
          message: res.data.tmdbConfigured
            ? "TMDB API key saved and activated."
            : "TMDB API key removed. MBL will use local NFO files.",
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

  const handleClear = async () => {
    setApiKey("");
    await handleSave();
  };

  return (
    <div style={{ "max-width": "800px", padding: "16px 0" }}>
      <div style={{ "margin-bottom": "28px" }}>
        <h1 style={{ margin: "0 0 6px 0", "font-size": "26px", "font-weight": "800" }}>
          Server Settings
        </h1>
        <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
          Configure external metadata providers, API keys, and diagnostic logging.
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

      {/* TMDB Section Card */}
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
                The Movie Database (TMDB)
              </h2>
              <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                Metadata, posters, backdrops and cast info
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
            {settings()?.tmdbConfigured ? "● Configured" : "○ Local NFO only"}
          </span>
        </div>

        <p
          style={{
            "font-size": "13px",
            color: "var(--text-secondary)",
            "line-height": "1.6",
            "margin-bottom": "18px",
          }}
        >
          Supplying a free TMDB API key allows MBL to automatically identify movies and TV shows,
          download high-resolution artwork, and enrich episodes with summaries and ratings. Without
          an API key, MBL operates exclusively in local-metadata mode reading embedded tags and
          sidecar NFO files.
        </p>

        <form onSubmit={handleSave}>
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
                onClick={handleClear}
              >
                Remove Key
              </Button>
            </Show>
          </div>
        </form>
      </div>

      {/* Diagnostics Section Card */}
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
              Inspect live receiver handshakes and connection logs
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
