import { createResource, createSignal, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { Badge, Button } from "../../components/ui";

export default function RemoteAccess() {
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [lanOnly, setLanOnly] = createSignal(true);
  const [tailscaleAuth, setTailscaleAuth] = createSignal(false);
  const [limit, setLimit] = createSignal(0);
  const [diagnosis, setDiagnosis] = createSignal<
    components["schemas"]["ReachabilityReport"] | null
  >(null);
  const [status, { refetch }] = createResource(async () => {
    const result = await client.GET("/api/v1/system/remote-status");
    if (result.error || !result.data) throw new Error("Could not load remote access settings.");
    setLanOnly(result.data.isAdminLanOnly);
    setTailscaleAuth(result.data.isTailscaleAuthEnabled);
    setLimit(result.data.wanUploadLimitMbps);
    return result.data;
  });
  const run = async (action: () => Promise<void>) => {
    setError("");
    setBusy(true);
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  };
  const save = () =>
    run(async () => {
      if (!Number.isSafeInteger(limit()) || limit() < 0)
        throw new Error("Enter a non-negative whole number for the upload limit.");
      const result = await client.PUT("/api/v1/system/remote-policy", {
        body: {
          adminLanOnly: lanOnly(),
          tailscaleAuthEnabled: tailscaleAuth(),
          wanUploadLimitMbps: limit(),
        },
      });
      if (result.error) throw new Error("Could not save remote access settings.");
      await refetch();
    });
  const diagnose = () =>
    run(async () => {
      const result = await client.POST("/api/v1/system/reachability-test", { body: {} });
      if (result.error || !result.data) throw new Error("Could not run diagnostics.");
      setDiagnosis(result.data);
    });
  return (
    <div class="mbl-page" style={{ "max-width": "840px", display: "grid", gap: "20px" }}>
      <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "700" }}>Remote access</h1>
      <Show when={error() || status.error}>
        <p role="alert">{error() || "Could not load remote access settings. Please reload."}</p>
      </Show>
      <Show when={status()} fallback={<p>Loading remote access…</p>}>
        {(info) => (
          <>
            <section class="mbl-card-section" style={{ display: "grid", gap: "14px" }}>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                  "flex-wrap": "wrap",
                  gap: "10px",
                }}
              >
                <h2 style={{ margin: 0, "font-size": "18px" }}>Connection status</h2>
                <Badge variant={info().tailscale ? "direct-play" : "default"}>
                  Tailscale: {info().tailscale ? "Available" : "Unavailable"}
                </Badge>
              </div>
              <p style={{ margin: 0, "font-size": "var(--font-size-sm)" }}>
                Active WAN sessions: <strong>{info().activeWanSessions}</strong>
              </p>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-sm)",
                  color: "var(--text-secondary)",
                }}
              >
                Use Tailscale on the server and your devices for private remote access.
                Administrators signing in over WAN need two-factor authentication under Account &
                security.
              </p>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void save();
                }}
                style={{ display: "grid", gap: "14px", "margin-top": "4px" }}
              >
                <label
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "10px",
                    cursor: "pointer",
                    "font-size": "var(--font-size-sm)",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={lanOnly()}
                    onChange={(e) => setLanOnly(e.currentTarget.checked)}
                  />
                  <span>Restrict administration to LAN and tailnet</span>
                </label>
                <label
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "10px",
                    cursor: "pointer",
                    "font-size": "var(--font-size-sm)",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={tailscaleAuth()}
                    onChange={(e) => setTailscaleAuth(e.currentTarget.checked)}
                  />
                  <span>Enable Tailscale identity sign-in</span>
                </label>
                <label
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "6px",
                    "max-width": "320px",
                    "font-size": "var(--font-size-xs)",
                    "font-weight": "var(--font-weight-semibold)",
                    color: "var(--text-secondary)",
                  }}
                >
                  <span>WAN upload limit (Mbps; 0 = unlimited)</span>
                  <input
                    type="number"
                    min="0"
                    step="1"
                    required
                    value={limit()}
                    onInput={(e) => setLimit(e.currentTarget.valueAsNumber)}
                  />
                </label>
                <div>
                  <Button type="submit" disabled={busy()}>
                    Save settings
                  </Button>
                </div>
              </form>
            </section>
            <section class="mbl-card-section" style={{ display: "grid", gap: "12px" }}>
              <h2 style={{ margin: 0, "font-size": "18px" }}>Network diagnostics</h2>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-sm)",
                  color: "var(--text-secondary)",
                }}
              >
                The current diagnostic uses available network information; it does not establish
                that this server is reachable from the internet.
              </p>
              <div>
                <Button variant="secondary" disabled={busy()} onClick={diagnose}>
                  Run diagnostics
                </Button>
              </div>
              <Show when={diagnosis()}>
                <pre style={{ "white-space": "pre-wrap", "overflow-wrap": "anywhere", margin: 0 }}>
                  {JSON.stringify(diagnosis(), null, 2)}
                </pre>
              </Show>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}
