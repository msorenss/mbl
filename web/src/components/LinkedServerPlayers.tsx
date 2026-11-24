import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { formatClock } from "../pages/Remote";
import { createTimedSignal } from "../utils/timedSignal";
import { CommentWatcher } from "./RemoteComments";
import { Button } from "./ui";

type View = components["schemas"]["LinkedServerView"];
type Player = components["schemas"]["LinkedPlayer"];
type Item = components["schemas"]["LinkedItem"];
type Command = components["schemas"]["LinkedPlayerCommand"];

function message(error: unknown, fallback: string): string {
  const body = error as { error?: { message?: string } } | undefined;
  return body?.error?.message?.replace(/^[a-z_]+:\s*/, "") || fallback;
}

/** Players on other MBL servers this server is linked to, with remote control. */
export default function LinkedServerPlayers() {
  const [servers, setServers] = createSignal<View[]>([]);
  const [error, setError] = createTimedSignal<string>("", 10000);
  let timer: ReturnType<typeof setInterval> | undefined;

  const refresh = async () => {
    try {
      const result = await client.GET("/api/v1/linked-servers");
      if (result.data) setServers(result.data);
    } catch {}
  };
  onMount(() => {
    void refresh();
    timer = setInterval(() => void refresh(), 4000);
  });
  onCleanup(() => clearInterval(timer));

  const playing = () =>
    servers().flatMap((s) =>
      s.players
        .filter((p) => p.state === "playing" && p.nowPlayingItemId)
        .map((p) => `${s.id}\u0000${p.id}`),
    );

  const send = async (server: View, player: Player, command: Command) => {
    setError("");
    try {
      const result = await client.POST("/api/v1/linked-servers/{id}/players/{player}/command", {
        params: { path: { id: server.id, player: player.id } },
        body: command,
      });
      if (!result.response.ok) {
        setError(message(result.error, `Could not send the command to ${player.name}.`));
        return;
      }
      void refresh();
    } catch {
      setError("Could not reach the server.");
    }
  };

  return (
    <Show when={servers().length > 0}>
      <h2 style={{ "font-size": "1.2rem", margin: "1.5rem 0 0.5rem" }}>Other MBL servers</h2>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      {/* Keyed by server and player ID so the watchers survive each refresh. */}
      <For each={playing()}>
        {(key) => {
          const [serverId, playerId] = key.split("\u0000");
          return (
            <CommentWatcher
              player={
                servers()
                  .find((s) => s.id === serverId)
                  ?.players.find((p) => p.id === playerId)?.name ?? ""
              }
              fetch={async (afterId) =>
                (
                  await client.GET("/api/v1/linked-servers/{id}/players/{player}/comments", {
                    params: { path: { id: serverId, player: playerId }, query: { afterId } },
                  })
                ).data
              }
            />
          );
        }}
      </For>
      <For each={servers()}>
        {(server) => (
          <section class="mbl-card-section" aria-label={server.name}>
            <h3 style={{ margin: 0, "font-size": "1.05rem" }} translate="no">
              {server.name}
            </h3>
            <Show when={server.error}>
              <p role="status" style={{ color: "var(--danger)" }}>
                {`Not reachable: ${server.error}`}
              </p>
            </Show>
            <Show when={!server.error && server.players.length === 0}>
              <p style={{ color: "var(--color-text-muted)" }}>
                Nothing is playing there right now.
              </p>
            </Show>
            <For each={server.players}>
              {(player) => (
                <div
                  style={{
                    "border-top": "1px solid var(--border)",
                    "padding-top": "0.75rem",
                    "margin-top": "0.75rem",
                    display: "grid",
                    gap: "0.5rem",
                  }}
                >
                  <div style={{ display: "flex", "justify-content": "space-between", gap: "1rem" }}>
                    <strong translate="no">{player.name}</strong>
                    <small>
                      {player.state === "playing"
                        ? "Playing"
                        : player.state === "paused"
                          ? "Paused"
                          : "Idle"}
                    </small>
                  </div>
                  <Show when={player.nowPlayingTitle}>
                    <span>
                      <span translate="no">{player.nowPlayingTitle}</span>
                      {` · ${formatClock(player.positionMs)} / ${formatClock(player.durationMs)}`}
                    </span>
                  </Show>
                  <div style={{ display: "flex", gap: "0.5rem", "flex-wrap": "wrap" }}>
                    <Button
                      variant="secondary"
                      onClick={() =>
                        send(server, player, {
                          action: "seek",
                          positionMs: Math.max(0, player.positionMs - 10_000),
                        })
                      }
                    >
                      −10 s
                    </Button>
                    <Show
                      when={player.state === "playing"}
                      fallback={
                        <Button onClick={() => send(server, player, { action: "resume" })}>
                          Play
                        </Button>
                      }
                    >
                      <Button onClick={() => send(server, player, { action: "pause" })}>
                        Pause
                      </Button>
                    </Show>
                    <Button
                      variant="secondary"
                      onClick={() =>
                        send(server, player, {
                          action: "seek",
                          positionMs: Math.min(
                            player.durationMs || Number.MAX_SAFE_INTEGER,
                            player.positionMs + 30_000,
                          ),
                        })
                      }
                    >
                      +30 s
                    </Button>
                    <Button
                      variant="secondary"
                      onClick={() => send(server, player, { action: "stop" })}
                    >
                      Stop
                    </Button>
                  </div>
                  <label style={{ display: "flex", gap: "0.5rem", "align-items": "center" }}>
                    Volume
                    <input
                      type="range"
                      min="0"
                      max="100"
                      value={player.volume}
                      onChange={(e) =>
                        send(server, player, {
                          action: "volume",
                          volume: Number(e.currentTarget.value),
                        })
                      }
                    />
                    {`${player.volume}%`}
                  </label>
                  <PlayFromLibrary server={server} player={player} onPlayed={refresh} />
                </div>
              )}
            </For>
          </section>
        )}
      </For>
    </Show>
  );
}

/** Search the linked server's library and start a title on its player. */
function PlayFromLibrary(props: { server: View; player: Player; onPlayed: () => void }) {
  const [query, setQuery] = createSignal("");
  const [items, setItems] = createSignal<Item[] | null>(null);
  const [status, setStatus] = createSignal("");

  const search = async (e: Event) => {
    e.preventDefault();
    setStatus("");
    try {
      const result = await client.GET("/api/v1/linked-servers/{id}/search", {
        params: { path: { id: props.server.id }, query: { q: query() } },
      });
      if (!result.data) throw new Error(message(result.error, "The search did not work."));
      setItems(result.data);
    } catch (err) {
      setStatus(err instanceof Error ? err.message : "The search did not work.");
    }
  };

  const play = async (item: Item) => {
    setStatus("");
    try {
      const result = await client.POST("/api/v1/linked-servers/{id}/players/{player}/play", {
        params: { path: { id: props.server.id, player: props.player.id } },
        body: { itemId: item.id },
      });
      if (!result.response.ok) throw new Error(message(result.error, "Could not start it."));
      setItems(null);
      setQuery("");
      props.onPlayed();
    } catch (err) {
      setStatus(err instanceof Error ? err.message : "Could not start it.");
    }
  };

  return (
    <details>
      <summary>Play something from this server</summary>
      <form onSubmit={search} style={{ display: "flex", gap: "0.5rem", margin: "0.5rem 0" }}>
        <input
          type="search"
          aria-label="Search this server's library"
          placeholder="Title"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          style={{ flex: 1 }}
        />
        <Button type="submit" variant="secondary" disabled={!query().trim()}>
          Search
        </Button>
      </form>
      <Show when={items()}>
        {(list) => (
          <Show when={list().length > 0} fallback={<p style={{ margin: 0 }}>Nothing found.</p>}>
            <ul
              style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "4px" }}
            >
              <For each={list()}>
                {(item) => (
                  <li style={{ display: "flex", gap: "0.5rem", "align-items": "center" }}>
                    <span translate="no" style={{ flex: 1 }}>
                      {item.title}
                      {item.year ? ` (${item.year})` : ""}
                    </span>
                    <Button size="sm" onClick={() => void play(item)}>
                      Play
                    </Button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        )}
      </Show>
      <Show when={status()}>
        <p role="alert" style={{ margin: 0 }}>
          {status()}
        </p>
      </Show>
    </details>
  );
}
