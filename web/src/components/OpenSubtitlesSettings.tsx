import { createSignal, onMount, Show } from "solid-js";
import client from "../api/client";
import { Badge, Button } from "./ui";

/** Server settings: OpenSubtitles API key and an optional account for more downloads per day. */
export default function OpenSubtitlesSettings() {
  const [hasKey, setHasKey] = createSignal(false);
  const [hasPassword, setHasPassword] = createSignal(false);
  const [apiKey, setApiKey] = createSignal("");
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  const apply = (data: { hasApiKey: boolean; username: string; hasPassword: boolean }) => {
    setHasKey(data.hasApiKey);
    setHasPassword(data.hasPassword);
    setUsername(data.username);
    setApiKey("");
    setPassword("");
  };

  onMount(async () => {
    try {
      const result = await client.GET("/api/v1/system/opensubtitles");
      if (result.data) apply(result.data);
    } catch {
      setError("Could not load the subtitle settings.");
    }
  });

  const save = async (clear = false) => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PUT("/api/v1/system/opensubtitles", {
        body: {
          apiKey: apiKey().trim() || undefined,
          username: username().trim(),
          password: password() || undefined,
          clear,
        },
      });
      if (!result.data) throw new Error();
      apply(result.data);
      setNotice(clear ? "Removed." : "Saved.");
    } catch {
      setError("Could not save the subtitle settings.");
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.POST("/api/v1/system/opensubtitles/test");
      if (!result.response.ok) {
        const message = (result.error as { message?: string } | undefined)?.message;
        throw new Error(message?.replace(/^[a-z_]+:\s*/, "") || "OpenSubtitles did not accept it.");
      }
      setNotice("OpenSubtitles accepted the settings.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "OpenSubtitles did not accept it.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="mbl-card-section">
      <h2>
        Subtitle search <Show when={hasKey()}>{<Badge>On</Badge>}</Show>
      </h2>
      <p style={{ color: "var(--text-secondary)" }}>
        With an OpenSubtitles API key, people can find and download subtitles from a title's page
        and from the player. Create a key under API consumers on opensubtitles.com. An account
        (username and password) allows more downloads per day.
      </p>
      <div style={{ display: "grid", gap: "8px", "max-width": "420px" }}>
        <label>
          API key
          <input
            type="password"
            value={apiKey()}
            placeholder={hasKey() ? "Saved; enter a new key to replace it" : ""}
            onInput={(e) => setApiKey(e.currentTarget.value)}
            autocomplete="off"
            style={{ width: "100%" }}
          />
        </label>
        <label>
          Username (optional)
          <input
            value={username()}
            onInput={(e) => setUsername(e.currentTarget.value)}
            autocomplete="off"
            style={{ width: "100%" }}
          />
        </label>
        <label>
          Password (optional)
          <input
            type="password"
            value={password()}
            placeholder={hasPassword() ? "Saved; enter a new password to replace it" : ""}
            onInput={(e) => setPassword(e.currentTarget.value)}
            autocomplete="new-password"
            style={{ width: "100%" }}
          />
        </label>
      </div>
      <div style={{ display: "flex", gap: "8px", "margin-top": "12px", "flex-wrap": "wrap" }}>
        <Button onClick={() => void save()} disabled={busy()}>
          Save
        </Button>
        <Button variant="secondary" onClick={() => void test()} disabled={busy() || !hasKey()}>
          Test connection
        </Button>
        <Show when={hasKey()}>
          <Button variant="secondary" onClick={() => void save(true)} disabled={busy()}>
            Remove
          </Button>
        </Show>
      </div>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ color: "var(--success)" }}>
          {notice()}
        </p>
      </Show>
    </section>
  );
}
