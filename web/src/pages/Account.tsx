import { createResource, createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import HistoryImport from "../components/HistoryImport";
import { Badge, Button } from "../components/ui";
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
  const [deletePassword, setDeletePassword] = createSignal("");
  const deleteAccount = () =>
    run(async () => {
      if (
        !confirm(
          "Permanently delete your account? Your watch history, preferences and devices are removed and cannot be restored.",
        )
      )
        return;
      const result = await client.DELETE("/api/v1/users/me", {
        body: { password: deletePassword() },
      });
      if (result.error) {
        const status = result.response.status;
        if (status === 401) throw new Error("Incorrect password.");
        if (status === 403)
          throw new Error("Managed profiles cannot delete themselves. Ask an administrator.");
        if (status === 409)
          throw new Error("You are the last administrator. Promote another administrator first.");
        throw new Error("Could not delete the account.");
      }
      setDeletePassword("");
      await logout();
    });
  return (
    <div class="mbl-page" style={{ "max-width": "840px", display: "grid", gap: "20px" }}>
      <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "700" }}>Account & security</h1>
      <Show when={error() || security.error || sessions.error}>
        <p role="alert">
          {error() || "Could not load account information. Please reload the page."}
        </p>
      </Show>
      <section class="mbl-card-section" style={{ display: "grid", gap: "12px" }}>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            gap: "12px",
            "flex-wrap": "wrap",
          }}
        >
          <h2 style={{ margin: 0, "font-size": "18px" }}>Two-factor authentication</h2>
          <Show when={!security.loading && security()}>
            <Badge variant={security()?.totpEnabled ? "direct-play" : "default"}>
              {security()?.totpEnabled ? "Enabled" : "Disabled"}
            </Badge>
          </Show>
        </div>
        <Show when={!security.loading && security()} fallback={<p>Loading security settings…</p>}>
          <div>
            <Show
              when={security()?.totpEnabled}
              fallback={
                <Button disabled={busy() || !!setup()} onClick={beginSetup}>
                  Set up two-factor authentication
                </Button>
              }
            >
              <Button variant="secondary" disabled={busy()} onClick={disable}>
                Disable two-factor authentication
              </Button>
            </Show>
          </div>
        </Show>
        <Show when={setup()}>
          {(pending) => (
            <div
              style={{
                display: "grid",
                gap: "10px",
                padding: "var(--space-4)",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
              }}
            >
              <p style={{ margin: 0, "font-size": "var(--font-size-sm)" }}>
                Add this secret to your authenticator app, then enter the six-digit code to activate
                it.
              </p>
              <code style={{ "overflow-wrap": "anywhere", padding: "8px 10px" }}>
                {pending().secret}
              </code>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-xs)",
                  color: "var(--text-muted)",
                }}
              >
                Two-factor authentication remains disabled until the code is verified.
              </p>
              <form
                style={{
                  display: "flex",
                  "align-items": "flex-end",
                  "flex-wrap": "wrap",
                  gap: "10px",
                }}
                onSubmit={(e) => {
                  e.preventDefault();
                  void verify();
                }}
              >
                <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
                  <label
                    for="verify-totp"
                    style={{
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "var(--font-weight-semibold)",
                      color: "var(--text-secondary)",
                    }}
                  >
                    Authentication code
                  </label>
                  <input
                    id="verify-totp"
                    inputmode="numeric"
                    autocomplete="one-time-code"
                    pattern="[0-9]{6}"
                    required
                    maxlength={6}
                    value={code()}
                    onInput={(e) => setCode(e.currentTarget.value)}
                  />
                </div>
                <Button type="submit" disabled={busy()}>
                  Verify & enable
                </Button>
              </form>
            </div>
          )}
        </Show>
      </section>
      <section class="mbl-card-section" style={{ display: "grid", gap: "12px" }}>
        <h2 style={{ margin: 0, "font-size": "18px" }}>Signed-in devices</h2>
        <Show when={!sessions.loading} fallback={<p>Loading devices…</p>}>
          <For
            each={sessions()}
            fallback={
              <p style={{ margin: 0, color: "var(--text-muted)" }}>No active sessions found.</p>
            }
          >
            {(session) => (
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                  "flex-wrap": "wrap",
                  gap: "12px",
                  padding: "12px 0",
                  "border-bottom": "1px solid var(--border)",
                }}
              >
                <div>
                  <div>
                    <strong>{session.deviceName || session.clientName || "Unknown device"}</strong>
                    {session.current ? " (this device)" : ""}
                  </div>
                  <p
                    style={{
                      margin: "4px 0 0",
                      "font-size": "var(--font-size-xs)",
                      color: "var(--text-muted)",
                    }}
                  >
                    Last active: {new Date(session.lastSeenAt).toLocaleString()}
                  </p>
                </div>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={busy()}
                  onClick={() => revoke(session.id, session.current)}
                >
                  Sign out
                </Button>
              </div>
            )}
          </For>
        </Show>
      </section>
      <HistoryImport />
      <section
        aria-labelledby="delete-account-heading"
        class="mbl-card-section"
        style={{
          display: "grid",
          gap: "12px",
          "border-color": "color-mix(in srgb, var(--danger) 35%, var(--border))",
        }}
      >
        <h2 id="delete-account-heading" style={{ margin: 0, "font-size": "18px" }}>
          Delete account
        </h2>
        <p
          style={{ margin: 0, "font-size": "var(--font-size-sm)", color: "var(--text-secondary)" }}
        >
          Permanently removes your account, watch history and signed-in devices from this server.
          Media files are not affected.
        </p>
        <form
          style={{
            display: "flex",
            "align-items": "flex-end",
            "flex-wrap": "wrap",
            gap: "10px",
          }}
          onSubmit={(e) => {
            e.preventDefault();
            void deleteAccount();
          }}
        >
          <div
            style={{ display: "flex", "flex-direction": "column", gap: "6px", flex: "1 1 220px" }}
          >
            <label
              for="delete-account-password"
              style={{
                "font-size": "var(--font-size-xs)",
                "font-weight": "var(--font-weight-semibold)",
                color: "var(--text-secondary)",
              }}
            >
              Confirm with your password
            </label>
            <input
              id="delete-account-password"
              type="password"
              autocomplete="current-password"
              required
              value={deletePassword()}
              onInput={(e) => setDeletePassword(e.currentTarget.value)}
            />
          </div>
          <Button type="submit" variant="danger" disabled={busy() || !deletePassword()}>
            Delete my account
          </Button>
        </form>
      </section>
    </div>
  );
}
