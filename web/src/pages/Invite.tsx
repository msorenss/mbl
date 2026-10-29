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
    <main style={{ "max-width": "500px", margin: "40px auto", padding: "20px" }}>
      <h1>Join MBL</h1>
      <Show
        when={user()}
        fallback={
          <>
            <Show when={invite.loading}>
              <p role="status">Checking invitation…</p>
            </Show>
            <Show when={invite.error}>
              <p role="alert">Invitation unavailable, expired or already used.</p>
              <Button onClick={() => refetch()}>Retry</Button>
            </Show>
            <Show when={invite()}>
              {(details) => (
                <>
                  <p>
                    Invitation expires {new Date(details().expiresAt).toLocaleString()}. Maximum
                    rating: {details().contentRatingCeiling || "No age limit"}.
                  </p>
                  <form onSubmit={submit} style={{ display: "grid", gap: "12px" }}>
                    <label for="invite-username">Username</label>
                    <input
                      id="invite-username"
                      required
                      autocomplete="username"
                      disabled={busy()}
                      value={username()}
                      onInput={(e) => setUsername(e.currentTarget.value)}
                    />
                    <label for="invite-password">Password (at least 8 characters)</label>
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
        <p>
          You are already signed in. Sign out from your account page before creating another
          account.
        </p>
        <A href="/account">Account &amp; security</A>
      </Show>
    </main>
  );
}
