import { A, useNavigate, useParams } from "@solidjs/router";
import { createResource, createSignal, Show } from "solid-js";
import client, { setStoredToken } from "../api/client";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";
export default function Invite() {
  const params = useParams();
  const navigate = useNavigate();
  const { user, refreshUser } = useAuth();
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [invite, { refetch }] = createResource(
    () => params.code,
    async (code) => {
      const result = await client.GET("/api/v1/invites/inspect/{code}", {
        params: { path: { code } },
      });
      if (result.error || !result.data)
        throw new Error("Invitation unavailable, expired or already used.");
      return result.data;
    },
  );
  const submit = async (e: Event) => {
    e.preventDefault();
    if (busy() || !invite() || user()) return;
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/auth/register-invite", {
        body: { code: params.code ?? "", username: username().trim(), password: password() },
      });
      if (result.error || !result.data)
        throw new Error(
          "Registration failed. Check the username and invitation validity before retrying.",
        );
      setStoredToken(result.data.token);
      setPassword("");
      await refreshUser();
      navigate("/", { replace: true });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Registration failed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div
      style={{
        display: "flex",
        "justify-content": "center",
        "align-items": "center",
        "min-height": "100dvh",
        padding:
          "max(20px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(20px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left))",
        background: "var(--bg-base)",
      }}
    >
      <main
        class="mbl-page mbl-card-section"
        style={{ width: "min(460px, 100%)", padding: "clamp(24px, 5vw, 34px)" }}
      >
        <h1 style={{ margin: "0 0 12px", "font-size": "24px", "font-weight": "700" }}>Join MBL</h1>
        <Show
          when={user()}
          fallback={
            <>
              <Show when={invite.loading}>
                <p role="status">Checking invitation…</p>
              </Show>
              <Show when={invite.error}>
                <p role="alert">Invitation unavailable, expired or already used.</p>
                <Button variant="secondary" onClick={() => refetch()}>
                  Retry
                </Button>
              </Show>
              <Show when={invite()}>
                {(details) => (
                  <>
                    <p
                      style={{
                        margin: "0 0 16px",
                        "font-size": "var(--font-size-sm)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      Invitation expires {new Date(details().expiresAt).toLocaleString()}. Maximum
                      rating: {details().contentRatingCeiling || "No age limit"}.
                    </p>
                    <form onSubmit={submit} style={{ display: "grid", gap: "12px" }}>
                      <div style={{ display: "grid", gap: "6px" }}>
                        <label
                          for="invite-username"
                          style={{
                            "font-size": "var(--font-size-xs)",
                            "font-weight": "var(--font-weight-semibold)",
                            color: "var(--text-secondary)",
                          }}
                        >
                          Username
                        </label>
                        <input
                          id="invite-username"
                          required
                          autocomplete="username"
                          disabled={busy()}
                          value={username()}
                          onInput={(e) => setUsername(e.currentTarget.value)}
                        />
                      </div>
                      <div style={{ display: "grid", gap: "6px" }}>
                        <label
                          for="invite-password"
                          style={{
                            "font-size": "var(--font-size-xs)",
                            "font-weight": "var(--font-weight-semibold)",
                            color: "var(--text-secondary)",
                          }}
                        >
                          Password (at least 8 characters)
                        </label>
                        <input
                          id="invite-password"
                          type="password"
                          minlength={8}
                          required
                          autocomplete="new-password"
                          disabled={busy()}
                          value={password()}
                          onInput={(e) => setPassword(e.currentTarget.value)}
                        />
                      </div>
                      <Button type="submit" disabled={busy()}>
                        Create account
                      </Button>
                    </form>
                  </>
                )}
              </Show>
              <Show when={error()}>
                <p role="alert">{error()}</p>
              </Show>
            </>
          }
        >
          <p style={{ color: "var(--text-secondary)", "font-size": "var(--font-size-sm)" }}>
            You are already signed in. Sign out from your account page before creating another
            account.
          </p>
          <A href="/account" style={{ color: "var(--accent)", "font-weight": "600" }}>
            Account &amp; security
          </A>
        </Show>
      </main>
    </div>
  );
}
