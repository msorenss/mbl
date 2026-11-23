import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type Linked = components["schemas"]["LinkedServer"];

/**
 * Links to other MBL servers (administrators). With a link, the Remote page shows that
 * server's players and can control them and start titles from its library there.
 */
export default function LinkedServersSettings() {
  const [links, setLinks] = createSignal<Linked[]>([]);
  const [name, setName] = createSignal("");
  const [url, setUrl] = createSignal("");
  const [token, setToken] = createSignal("");
  const [shared, setShared] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");

  const load = async () => {
    try {
      const result = await client.GET("/api/v1/system/linked-servers");
      if (result.data) setLinks(result.data);
    } catch {
      setError("Could not load the linked servers.");
    }
  };
  onMount(() => void load());

  const add = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/system/linked-servers", {
        body: { name: name(), url: url(), token: token(), shared: shared() },
      });
      if (!result.response.ok) {
        const body = result.error as { error?: { message?: string } } | undefined;
        throw new Error(body?.error?.message?.replace(/^[a-z_]+:\s*/, "") || "");
      }
      setName("");
      setUrl("");
      setToken("");
      setShared(false);
      await load();
    } catch (err) {
      setError(
        err instanceof Error && err.message ? err.message : "The server could not be linked.",
      );
    } finally {
      setBusy(false);
    }
  };

  const remove = async (link: Linked) => {
    if (!window.confirm(`Remove the link to ${link.name}?`)) return;
    try {
      await client.DELETE("/api/v1/system/linked-servers/{id}", {
        params: { path: { id: link.id } },
      });
      await load();
    } catch {
      setError("Could not remove the link.");
    }
  };

  return (
    <section class="mbl-card-section">
      <h2 style={{ "font-size": "18px", "font-weight": "700", margin: "0 0 8px" }}>
        Other MBL servers
      </h2>
      <p style={{ margin: "0 0 12px", color: "var(--text-secondary)" }}>
        Link another MBL to see and control its players from Remote control, and to start titles
        from its library there. On the other server, make a token under AI agents with the scopes
        read and playback, and paste it here. Revoking that token there ends the link.
      </p>
      <Show when={links().length > 0}>
        <ul
          style={{
            margin: "0 0 12px",
            padding: 0,
            "list-style": "none",
            display: "grid",
            gap: "6px",
          }}
        >
          <For each={links()}>
            {(link) => (
              <li style={{ display: "flex", gap: "12px", "align-items": "center" }}>
                <span style={{ flex: 1 }}>
                  <strong translate="no">{link.name}</strong>{" "}
                  <span translate="no" style={{ color: "var(--text-muted)", "font-size": "13px" }}>
                    {link.url}
                  </span>
                  <Show when={link.shared}>
                    <span style={{ "font-size": "13px" }}>
                      {" · "}
                      <span>Everyone here may use it</span>
                    </span>
                  </Show>
                </span>
                <Button size="sm" variant="secondary" onClick={() => void remove(link)}>
                  Remove
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <form onSubmit={add} style={{ display: "grid", gap: "8px", "max-width": "520px" }}>
        <label>
          Name
          <input
            value={name()}
            onInput={(e) => setName(e.currentTarget.value)}
            placeholder="Living room PC"
            required
          />
        </label>
        <label>
          Address
          <input
            value={url()}
            onInput={(e) => setUrl(e.currentTarget.value)}
            placeholder="http://192.168.1.20:8420"
            required
          />
        </label>
        <label>
          Token from the other server
          <input
            type="password"
            autocomplete="off"
            value={token()}
            onInput={(e) => setToken(e.currentTarget.value)}
            required
          />
        </label>
        <label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
          <input
            type="checkbox"
            checked={shared()}
            onChange={(e) => setShared(e.currentTarget.checked)}
          />
          Everyone on this server may use its players (not only administrators)
        </label>
        <div>
          <Button type="submit" disabled={busy()}>
            {busy() ? "Checking…" : "Link server"}
          </Button>
        </div>
      </form>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)" }}>
          {error()}
        </p>
      </Show>
    </section>
  );
}
