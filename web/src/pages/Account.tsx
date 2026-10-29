import { createResource, createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import HistoryImport from "../components/HistoryImport";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

export default function Account() {
  const { user, logout } = useAuth();
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [setup, setSetup] = createSignal<components["schemas"]["TotpSetup"] | null>(null);
  const [code, setCode] = createSignal("");
  const [security, { refetch: refreshSecurity }] = createResource(async () => {
    const id = user()?.id;
    if (!id) return null;
    const result = await client.GET("/api/v1/users/{id}/permissions", { params: { path: { id } } });
    if (result.error || !result.data) throw new Error("Could not load account security.");
    return result.data;
  });
  const [sessions, { refetch: refreshSessions }] = createResource(async () => {
    const result = await client.GET("/api/v1/auth/sessions");
    if (result.error || !result.data) throw new Error("Could not load sessions.");
    return result.data;
  });
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  };
  const beginSetup = () =>
    run(async () => {
      const result = await client.POST("/api/v1/auth/totp/setup");
      if (result.error || !result.data) throw new Error("Could not start two-factor setup.");
      setSetup(result.data);
      setCode("");
    });
  const verify = () =>
    run(async () => {
      const result = await client.POST("/api/v1/auth/totp/verify", { body: { pin: code() } });
      if (result.error || result.data !== true) throw new Error("Invalid authentication code.");
      setSetup(null);
      setCode("");
      await refreshSecurity();
    });
  const disable = () => {
    if (!confirm("Disable two-factor authentication? Remote administrator sign-in requires it."))
      return;
    void run(async () => {
      const result = await client.POST("/api/v1/auth/totp/disable");
      if (result.error) throw new Error("Could not disable two-factor authentication.");
      setSetup(null);
      await refreshSecurity();
    });
  };
  const revoke = (id: number, current: boolean) =>
    run(async () => {
      if (!confirm("Sign out this device?")) return;
      const result = await client.DELETE("/api/v1/auth/sessions/{id}", {
        params: { path: { id } },
      });
      if (result.error) throw new Error("Could not revoke session.");
      if (current) await logout();
      else await refreshSessions();
    });
  return (
    <div style={{ "max-width": "800px", display: "grid", gap: "24px" }}>
      <h1>Account & security</h1>
      <Show when={error() || security.error || sessions.error}>
        <p role="alert">
          {error() || "Could not load account information. Please reload the page."}
        </p>
      </Show>
      <section>
        <h2>Two-factor authentication</h2>
        <Show when={!security.loading && security()} fallback={<p>Loading security settings…</p>}>
          <p>{security()?.totpEnabled ? "Enabled" : "Disabled"}</p>
          <Show
            when={security()?.totpEnabled}
            fallback={
              <Button disabled={busy() || !!setup()} onClick={beginSetup}>
                Set up two-factor authentication
              </Button>
            }
          >
            <Button disabled={busy()} onClick={disable}>
              Disable two-factor authentication
            </Button>
          </Show>
        </Show>
        <Show when={setup()}>
          {(pending) => (
            <div>
              <p>
                Add this secret to your authenticator app, then enter the six-digit code to activate
                it.
              </p>
              <code style={{ "overflow-wrap": "anywhere" }}>{pending().secret}</code>
              <p>Two-factor authentication remains disabled until the code is verified.</p>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void verify();
                }}
              >
                <label for="verify-totp">Authentication code</label>{" "}
                <input
                  id="verify-totp"
                  inputmode="numeric"
                  autocomplete="one-time-code"
                  pattern="[0-9]{6}"
                  required
                  maxlength={6}
                  value={code()}
                  onInput={(e) => setCode(e.currentTarget.value)}
                />{" "}
                <Button type="submit" disabled={busy()}>
                  Verify & enable
                </Button>
              </form>
            </div>
          )}
        </Show>
      </section>
      <section>
        <h2>Signed-in devices</h2>
        <Show when={!sessions.loading} fallback={<p>Loading devices…</p>}>
          <For each={sessions()}>
            {(session) => (
              <div style={{ padding: "12px 0", "border-bottom": "1px solid var(--border)" }}>
                <strong>{session.deviceName || session.clientName || "Unknown device"}</strong>
                {session.current ? " (this device)" : ""}
                <p>Last active: {new Date(session.lastSeenAt).toLocaleString()}</p>
                <Button disabled={busy()} onClick={() => revoke(session.id, session.current)}>
                  Sign out
                </Button>
              </div>
            )}
          </For>
        </Show>
      </section>
      <HistoryImport />
    </div>
  );
}
