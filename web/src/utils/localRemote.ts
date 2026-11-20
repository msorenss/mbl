import { client, websocketProtocols } from "../api/client";
import type { RemoteCommand } from "./cast";

/** What the local player currently reports to the player registry. */
export interface LocalPlayerState {
  state: "idle" | "playing" | "paused";
  itemId: number | null;
  positionMs: number;
  durationMs: number;
  volume: number;
}

const ID_KEY = "mbl.player.id";

/** One id per browser tab, so two tabs are two players. */
export function localPlayerId(): string {
  try {
    const saved = sessionStorage.getItem(ID_KEY);
    if (saved) return saved;
    const id = `web-${crypto.randomUUID()}`;
    sessionStorage.setItem(ID_KEY, id);
    return id;
  } catch {
    return `web-${Math.random().toString(36).slice(2)}`;
  }
}

/** A readable name such as "Firefox on Linux". */
export function localPlayerName(userAgent = navigator.userAgent): string {
  const browser = /Edg\//.test(userAgent)
    ? "Edge"
    : /Firefox\//.test(userAgent)
      ? "Firefox"
      : /Chrome\//.test(userAgent)
        ? "Chrome"
        : /Safari\//.test(userAgent)
          ? "Safari"
          : "Browser";
  const os = /Android/.test(userAgent)
    ? "Android"
    : /iPhone|iPad/.test(userAgent)
      ? "iOS"
      : /Windows/.test(userAgent)
        ? "Windows"
        : /Mac OS X/.test(userAgent)
          ? "macOS"
          : /Linux/.test(userAgent)
            ? "Linux"
            : "";
  return os ? `${browser} on ${os}` : browser;
}

const clampMs = (value: number) =>
  Math.round(Math.min(Number.MAX_SAFE_INTEGER, Math.max(0, Number.isFinite(value) ? value : 0)));

/**
 * Registers this browser's player so the user's other devices can control it.
 * Returns a function that unregisters it.
 */
export function startLocalRemote(
  read: () => LocalPlayerState | null,
  onCommand: (command: RemoteCommand) => void | Promise<void>,
): () => void {
  const id = localPlayerId();
  const name = localPlayerName();
  let stopped = false;
  let socket: WebSocket | null = null;
  let registered = false;
  let queue: Promise<void> = Promise.resolve();

  const connect = () => {
    if (stopped || socket || typeof WebSocket === "undefined") return;
    const url = new URL(
      `/api/v1/players/${encodeURIComponent(id)}/connect`,
      window.location.origin,
    );
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(url, websocketProtocols());
    socket = ws;
    ws.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      try {
        void Promise.resolve(onCommand(JSON.parse(event.data) as RemoteCommand)).catch(() => {});
      } catch {}
    };
    ws.onclose = () => {
      if (socket === ws) socket = null;
    };
  };

  const sync = () => {
    if (stopped) return;
    const state = read();
    if (!state) return;
    const body = {
      id,
      name,
      deviceType: "web",
      lastSeenAt: Date.now(),
      state: state.state,
      nowPlayingItemId: state.itemId,
      positionMs: clampMs(state.positionMs),
      durationMs: clampMs(state.durationMs),
      volume: Math.max(0, Math.min(100, Math.round(state.volume))),
    };
    queue = queue
      .then(async () => {
        if (stopped) return;
        const result = await client.POST("/api/v1/players", {
          body,
          signal: AbortSignal.timeout(5000),
        });
        if (!result.error) {
          registered = true;
          connect();
        }
      })
      .catch(() => {});
  };

  sync();
  const timer = setInterval(sync, 5000);
  return () => {
    stopped = true;
    clearInterval(timer);
    socket?.close();
    socket = null;
    if (registered)
      void client
        .DELETE("/api/v1/players/{id}", { params: { path: { id } } })
        .catch(() => undefined);
  };
}
