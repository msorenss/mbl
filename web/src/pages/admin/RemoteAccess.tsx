import { createResource, createSignal, For, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { Badge, Button } from "../../components/ui";
import type { BadgeVariant } from "../../components/ui/Badge";

type CheckStatus = components["schemas"]["CheckStatus"];

const CHECK_LABELS: Record<string, string> = {
  publicIp: "Public address",
  routerWan: "Router",
  dns: "DNS",
  port: "HTTPS port",
  port80: "Port 80",
  certificate: "Certificate",
  tailscale: "Tailscale",
};

const STATUS_LABELS: Record<CheckStatus, string> = {
  ok: "OK",
  warn: "Check",
  fail: "Problem",
  skipped: "Skipped",
};

const statusVariant = (status: CheckStatus): BadgeVariant =>
  status === "ok"
    ? "success"
    : status === "warn"
      ? "warning"
      : status === "fail"
        ? "danger"
        : "default";

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
    if (result.response?.status === 403)
      throw new Error(
        "Remote access settings can only be opened from the home network or Tailscale. Open MBL with its local address, for example http://192.168.x.x:8420.",
      );
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
        <p role="alert">
          {error() ||
            (status.error instanceof Error
              ? status.error.message
              : "Could not load remote access settings. Please reload.")}
        </p>
      </Show>
      <Show
        when={!status.error && status()}
        fallback={
          <Show when={!status.error}>
            <p>Loading remote access…</p>
          </Show>
        }
      >
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
              <Show when={info().tailscale?.magicDnsName}>
                {(name) => (
                  <p style={{ margin: 0, "font-size": "var(--font-size-sm)" }}>
                    Tailnet address: <strong>{name().replace(/\.$/, "")}</strong>
                  </p>
                )}
              </Show>
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
                Checks the public address, the router's WAN address (UPnP), DNS for the public host
                name, the forwarded HTTPS port, the certificate and Tailscale. The server contacts
                api.ipify.org once to learn its public address.
              </p>
              <div>
                <Button variant="secondary" disabled={busy()} onClick={diagnose}>
                  {busy() ? "Checking…" : "Run diagnostics"}
                </Button>
              </div>
              <Show when={diagnosis()}>
                {(report) => (
                  <div style={{ display: "grid", gap: "12px" }}>
                    <div
                      role="status"
                      style={{
                        display: "flex",
                        gap: "10px",
                        "align-items": "center",
                        "flex-wrap": "wrap",
                      }}
                    >
                      <Badge
                        variant={
                          report().isDirectReachable
                            ? "success"
                            : report().isCgnat
                              ? "danger"
                              : "warning"
                        }
                      >
                        {report().isDirectReachable
                          ? "Reachable from the internet"
                          : report().isCgnat
                            ? "Behind CGNAT"
                            : "Not confirmed reachable"}
                      </Badge>
                      <span style={{ "font-size": "var(--font-size-sm)" }}>{report().details}</span>
                    </div>
                    <ul
                      aria-label="Diagnostic checks"
                      style={{
                        margin: 0,
                        padding: 0,
                        "list-style": "none",
                        display: "grid",
                        gap: "8px",
                      }}
                    >
                      <For each={report().checks ?? []}>
                        {(check) => (
                          <li
                            style={{
                              display: "grid",
                              "grid-template-columns": "minmax(110px, auto) 1fr",
                              gap: "10px",
                              "align-items": "start",
                              "font-size": "var(--font-size-sm)",
                            }}
                          >
                            <Badge variant={statusVariant(check.status)}>
                              {CHECK_LABELS[check.id] ?? check.id}: {STATUS_LABELS[check.status]}
                            </Badge>
                            <span style={{ "overflow-wrap": "anywhere" }}>{check.detail}</span>
                          </li>
                        )}
                      </For>
                    </ul>
                    <Show when={(report().publicHostnames ?? []).length > 0 && report().httpsPort}>
                      <p style={{ margin: 0, "font-size": "var(--font-size-sm)" }}>
                        Remote address:{" "}
                        <strong>
                          https://{(report().publicHostnames ?? [])[0]}
                          {report().httpsPort === 443 ? "" : `:${report().httpsPort}`}
                        </strong>
                      </p>
                    </Show>
                  </div>
                )}
              </Show>
            </section>
          </>
        )}
      </Show>
    </div>
  );
}
