import { onCleanup, onMount } from "solid-js";
import { websocketProtocols } from "../api/client";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";
import { formatPlannedTime } from "../utils/plannedTime";

type ServerEvent =
  | {
      type: "movieNightScheduled";
      data: { sessionId: string; startsAt: number; hostId: number; host: string };
    }
  | { type: "movieNightDecided"; data: { sessionId: string; title?: string | null } }
  | { type: string; data: unknown };

/** Text of the notice for a movie night event, or null when it is not one (or is your own). */
export function movieNightNotice(event: ServerEvent, me?: number): string | null {
  if (event.type === "movieNightScheduled") {
    const data = (event as Extract<ServerEvent, { type: "movieNightScheduled" }>).data;
    if (data.hostId === me) return null;
    return `${data.host} invited you to movie night ${formatPlannedTime(data.startsAt)}. Open Movie night to vote.`;
  }
  if (event.type === "movieNightDecided") {
    const data = (event as Extract<ServerEvent, { type: "movieNightDecided" }>).data;
    return data.title
      ? `Movie night: tonight's pick is ${data.title}.`
      : "Movie night: voting closed and no title got a like.";
  }
  return null;
}

/**
 * Listens for the server's events and shows movie night invitations and results, also as a system
 * notification when the browser allows them. Reconnects with a growing delay.
 */
export default function MovieNightNotices() {
  const { user } = useAuth();
  const { showToast } = useToast();
  let socket: WebSocket | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let delay = 2000;
  let stopped = false;

  const notify = (message: string, sessionId: string) => {
    showToast({ title: "Movie night", message, type: "info" });
    try {
      if (typeof Notification !== "undefined" && Notification.permission === "granted") {
        const notice = new Notification("Movie night", { body: message, tag: sessionId });
        notice.onclick = () => {
          window.focus();
          window.location.assign(`/matchmaker/${sessionId}`);
        };
      }
    } catch {}
  };

  const connect = () => {
    if (stopped || typeof WebSocket === "undefined") return;
    const url = new URL("/api/v1/events", window.location.origin);
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const ws = new WebSocket(url, websocketProtocols());
    socket = ws;
    ws.onopen = () => {
      delay = 2000;
    };
    ws.onmessage = (message) => {
      if (typeof message.data !== "string") return;
      try {
        const event = JSON.parse(message.data) as ServerEvent;
        const text = movieNightNotice(event, user()?.id);
        const session = (event.data as { sessionId?: string } | null)?.sessionId;
        if (text && session) notify(text, session);
      } catch {}
    };
    ws.onclose = () => {
      if (socket === ws) socket = undefined;
      if (stopped) return;
      retry = setTimeout(connect, delay);
      delay = Math.min(delay * 2, 60_000);
    };
  };

  onMount(connect);
  onCleanup(() => {
    stopped = true;
    if (retry) clearTimeout(retry);
    socket?.close();
  });
  return null;
}
