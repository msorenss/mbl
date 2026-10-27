import { createResource, createSignal, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { Button } from "../../components/ui";

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
    <div style={{ "max-width": "800px" }}>
      <h1>Remote access</h1>
      <Show when={error() || status.error}>
        <p role="alert">{error() || "Could not load remote access settings. Please reload."}</p>
      </Show>
      <Show when={status()} fallback={<p>Loading remote access…</p>}>
        {(info) => (
          <>
            <h2>Connection status</h2>
            <p>Tailscale: {info().tailscale ? "Available" : "Unavailable"}</p>
            <p>Active WAN sessions: {info().activeWanSessions}</p>
            <p>
              Use Tailscale on the server and your devices for private remote access. Administrators
              signing in over WAN need two-factor authentication under Account & security.
            </p>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
              style={{ display: "grid", gap: "16px" }}
            >
              <label>
                <input
                  type="checkbox"
                  checked={lanOnly()}
                  onChange={(e) => setLanOnly(e.currentTarget.checked)}
                />{" "}
                Restrict administration to LAN and tailnet
              </label>
              <label>
                <input
                  type="checkbox"
                  checked={tailscaleAuth()}
                  onChange={(e) => setTailscaleAuth(e.currentTarget.checked)}
                />{" "}
                Enable Tailscale identity sign-in
              </label>
              <label>
                WAN upload limit (Mbps; 0 = unlimited){" "}
                <input
                  type="number"
                  min="0"
                  step="1"
                  required
                  value={limit()}
                  onInput={(e) => setLimit(e.currentTarget.valueAsNumber)}
                />
              </label>
              <Button type="submit" disabled={busy()}>
                Save settings
              </Button>
            </form>
            <h2>Network diagnostics</h2>
            <p>
              The current diagnostic uses available network information; it does not establish that
              this server is reachable from the internet.
            </p>
            <Button disabled={busy()} onClick={diagnose}>
              Run diagnostics
            </Button>
            <Show when={diagnosis()}>
              <pre style={{ "white-space": "pre-wrap", "overflow-wrap": "anywhere" }}>
                {JSON.stringify(diagnosis(), null, 2)}
              </pre>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
