import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { localPlayerId } from "../utils/localRemote";

type PlayerInfo = components["schemas"]["PlayerInfo"];
type Command = components["schemas"]["PlayerCommand"];

export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(s).padStart(2, "0")}`;
}

const DEVICE_LABEL: Record<string, string> = {
  web: "Web browser",
  chromecast: "Chromecast",
  remote: "Remote display",
  tv: "TV app",
  dlna: "DLNA",
};

/** Lists the user's active players and controls one of them. */
export default function Remote() {
  const ownId = localPlayerId();
  const [players, setPlayers] = createSignal<PlayerInfo[]>([]);
  const [loaded, setLoaded] = createSignal(false);
  const [error, setError] = createSignal("");
  const [titles, setTitles] = createSignal<Record<number, string>>({});
  const requested = new Set<number>();

  const loadTitle = async (id: number) => {
    if (requested.has(id)) return;
    requested.add(id);
    try {
      const result = await client.GET("/api/v1/items/{id}", { params: { path: { id } } });
      const item = result.data;
      if (item) setTitles((t) => ({ ...t, [id]: item.title }));
    } catch {}
  };

  const refresh = async () => {
    try {
      const result = await client.GET("/api/v1/players", {});
      if (!result.data) {
        setError("Could not load players.");
        return;
      }
      setError("");
      const list = result.data.filter((p) => p.id !== ownId);
      setPlayers(list);
      for (const p of list) if (p.nowPlayingItemId) void loadTitle(p.nowPlayingItemId);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setLoaded(true);
    }
  };

  onMount(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    onCleanup(() => clearInterval(timer));
  });

  const send = async (player: PlayerInfo, command: Command) => {
    setError("");
    try {
      const result = await client.POST("/api/v1/players/{id}/command", {
        params: { path: { id: player.id } },
        body: command,
      });
      if (result.response?.status !== 204) {
        setError(
          result.response?.status === 503
            ? `${player.name} is not listening for commands right now.`
            : `Could not send the command to ${player.name}.`,
        );
        return;
      }
      // Show the expected state at once; the next refresh confirms it.
      setPlayers((list) =>
        list.map((p) => {
          if (p.id !== player.id) return p;
          if (command.action === "pause") return { ...p, state: "paused" };
          if (command.action === "resume") return { ...p, state: "playing" };
          if (command.action === "seek") return { ...p, positionMs: command.params.positionMs };
          if (command.action === "setVolume") return { ...p, volume: command.params.volume };
          return p;
        }),
      );
    } catch {
      setError("Could not reach the server.");
    }
  };

  const seekBy = (player: PlayerInfo, deltaMs: number) =>
    send(player, {
      action: "seek",
      params: {
        positionMs: Math.max(
          0,
          Math.min(player.durationMs || Number.MAX_SAFE_INTEGER, player.positionMs + deltaMs),
        ),
      },
    });

  return (
    <div class="mbl-page" style={{ "max-width": "720px" }}>
      <h1>Remote control</h1>
      <p style={{ color: "var(--color-text-muted)" }}>
        Control MBL playing on your other devices: browsers, Chromecast and TV apps signed in to
        your account.
      </p>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={loaded() && players().length === 0}>
        <p class="mbl-card-section">
          No other device is playing right now. Start playback on another device and it shows up
          here.
        </p>
      </Show>
      <For each={players()}>
        {(player) => (
          <section class="mbl-card-section" aria-label={player.name}>
            <div style={{ display: "flex", "justify-content": "space-between", gap: "1rem" }}>
              <div>
                <h2 style={{ margin: 0, "font-size": "1.1rem" }}>{player.name}</h2>
                <small style={{ color: "var(--color-text-muted)" }}>
                  {DEVICE_LABEL[player.deviceType] ?? player.deviceType}
                </small>
              </div>
              <small>
                {player.state === "playing"
                  ? "Playing"
                  : player.state === "paused"
                    ? "Paused"
                    : "Idle"}
              </small>
            </div>
            <Show when={player.nowPlayingItemId}>
              {(id) => (
                <p style={{ margin: "0.5rem 0" }}>
                  <a href={`/item/${id()}`}>{titles()[id()] ?? "Loading title…"}</a>
                  {" · "}
                  {formatClock(player.positionMs)} / {formatClock(player.durationMs)}
                </p>
              )}
            </Show>
            <div style={{ display: "flex", gap: "0.5rem", "flex-wrap": "wrap" }}>
              <Button variant="secondary" onClick={() => seekBy(player, -10_000)}>
                −10 s
              </Button>
              <Show
                when={player.state === "playing"}
                fallback={<Button onClick={() => send(player, { action: "resume" })}>Play</Button>}
              >
                <Button onClick={() => send(player, { action: "pause" })}>Pause</Button>
              </Show>
              <Button variant="secondary" onClick={() => seekBy(player, 30_000)}>
                +30 s
              </Button>
              <Button
                variant="secondary"
                onClick={() => send(player, { action: "stop", params: {} })}
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
                  send(player, {
                    action: "setVolume",
                    params: { volume: Number(e.currentTarget.value) },
                  })
                }
              />
              {player.volume}%
            </label>
          </section>
        )}
      </For>
    </div>
  );
}
