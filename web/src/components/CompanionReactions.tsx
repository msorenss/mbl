import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";

type Reaction = components["schemas"]["CompanionReaction"];
type Mode = components["schemas"]["CompanionMode"];
type Frequency = components["schemas"]["CompanionFrequency"];

const VISIBLE_MS = 12_000;
const HISTORY = 30;
const MODE_KEY = "mbl.companion.mode";
const FREQUENCY_KEY = "mbl.companion.frequency";
const FREQUENCIES: { value: Frequency; label: string }[] = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];
const MODES: { value: Mode; label: string }[] = [
  { value: "on", label: "Comments on" },
  { value: "replies", label: "Replies only" },
  { value: "off", label: "Comments off" },
];

function storedMode(): Mode {
  try {
    const value = localStorage.getItem(MODE_KEY);
    if (value === "on" || value === "replies" || value === "off") return value;
  } catch {}
  return "on";
}

function storedFrequency(): Frequency {
  try {
    const value = localStorage.getItem(FREQUENCY_KEY);
    if (value === "low" || value === "medium" || value === "high") return value;
  } catch {}
  return "medium";
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

const choice = (active: boolean) =>
  ({
    flex: 1,
    padding: "6px 8px",
    "border-radius": "8px",
    border: "1px solid rgba(255,255,255,0.3)",
    background: active ? "rgba(255,255,255,0.9)" : "transparent",
    color: active ? "#111" : "#fff",
    "font-size": "0.8rem",
    cursor: "pointer",
  }) as const;

const bubble = {
  margin: 0,
  padding: "10px 14px",
  "border-radius": "12px",
  background: "rgba(20, 20, 24, 0.85)",
  color: "#fff",
  "font-size": "0.95rem",
} as const;

/**
 * Shows what connected AI watch companions say about the current playback session, and lets
 * the viewer answer them or choose how much they may say.
 */
export default function CompanionReactions(props: {
  sessionId: string | undefined;
  /** The chat panel is open; toggled from the player's control bar. */
  open: boolean;
  onClose: () => void;
}) {
  const [shown, setShown] = createSignal<Reaction[]>([]);
  const [history, setHistory] = createSignal<Reaction[]>([]);
  const [companions, setCompanions] = createSignal<string[]>([]);
  const [mode, setMode] = createSignal<Mode>(storedMode());
  const [frequency, setFrequency] = createSignal<Frequency>(storedFrequency());
  const [listening, setListening] = createSignal<string[]>([]);
  let log: HTMLDivElement | undefined;
  const [draft, setDraft] = createSignal("");
  const [sending, setSending] = createSignal(false);
  const [error, setError] = createSignal("");

  const visible = (list: Reaction[]) =>
    mode() === "off" ? list.filter((r) => r.fromViewer) : list;

  const addEntries = (entries: Reaction[]) => {
    const fresh = entries.filter((r) => !history().some((h) => h.id === r.id));
    if (fresh.length === 0) return;
    setHistory((list) => [...list, ...fresh].slice(-HISTORY));
    setShown((list) => [...list, ...fresh.filter((r) => !r.fromViewer)].slice(-3));
  };

  createEffect(
    on(
      () => props.sessionId,
      (sessionId) => {
        setShown([]);
        setHistory([]);
        setCompanions([]);
        setListening([]);
        setError("");
        if (!sessionId) return;
        // The viewer's last choice follows them to the next session.
        if (mode() !== "on" || frequency() !== "medium")
          void client
            .PUT("/api/v1/companion/sessions/{id}/mode", {
              params: { path: { id: sessionId } },
              body: { mode: mode(), frequency: frequency() },
            })
            .catch(() => undefined);
        // Only entries sent after the player opened; older ones are not news.
        const after = Date.now();
        let afterId: number | undefined;
        let busy = false;
        let disposed = false;
        const poll = async () => {
          if (busy) return;
          busy = true;
          try {
            const query = afterId === undefined ? { after } : { after, afterId };
            const result = await client.GET("/api/v1/companion/sessions/{id}/reactions", {
              params: { path: { id: sessionId }, query },
            });
            const feed = result.data;
            if (!feed || disposed) return;
            setCompanions(feed.companions);
            setListening(feed.listening ?? []);
            if (feed.reactions.length > 0) {
              afterId = Math.max(afterId ?? 0, ...feed.reactions.map((r) => r.id ?? 0));
              addEntries(feed.reactions);
            }
          } catch {
          } finally {
            busy = false;
            const now = Date.now();
            setShown((list) => list.filter((r) => now - r.createdAt < VISIBLE_MS));
          }
        };
        void poll();
        const timer = setInterval(() => void poll(), 2000);
        onCleanup(() => {
          disposed = true;
          clearInterval(timer);
        });
      },
    ),
  );

  // Newest message in view whenever the chat opens or grows.
  createEffect(
    on([() => history().length, () => props.open], () => {
      if (props.open && log) log.scrollTop = log.scrollHeight;
    }),
  );

  const choose = async (nextMode: Mode, nextFrequency: Frequency) => {
    const sessionId = props.sessionId;
    if (!sessionId || (nextMode === mode() && nextFrequency === frequency())) return;
    const previous = [mode(), frequency()] as const;
    setMode(nextMode);
    setFrequency(nextFrequency);
    setError("");
    try {
      const result = await client.PUT("/api/v1/companion/sessions/{id}/mode", {
        params: { path: { id: sessionId } },
        body: { mode: nextMode, frequency: nextFrequency },
      });
      if (!result.response.ok) throw new Error();
      store(MODE_KEY, nextMode);
      store(FREQUENCY_KEY, nextFrequency);
    } catch {
      setMode(previous[0]);
      setFrequency(previous[1]);
      setError("Could not change the setting.");
    }
  };

  const nudge = async () => {
    const sessionId = props.sessionId;
    if (!sessionId) return;
    setError("");
    try {
      const result = await client.POST("/api/v1/companion/sessions/{id}/nudge", {
        params: { path: { id: sessionId } },
      });
      if (!result.response.ok) throw new Error();
    } catch {
      setError("Could not ask for a comment.");
    }
  };

  const send = async (event: Event) => {
    event.preventDefault();
    const sessionId = props.sessionId;
    const text = draft().trim();
    if (!sessionId || !text || sending()) return;
    setSending(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/companion/sessions/{id}/messages", {
        params: { path: { id: sessionId } },
        body: { text },
      });
      if (!result.data) throw new Error();
      setDraft("");
      addEntries([result.data]);
    } catch {
      setError("Could not send the message.");
    } finally {
      setSending(false);
    }
  };

  const author = (r: Reaction) => (r.fromViewer ? "You" : r.botName);

  return (
    <Show when={props.sessionId}>
      <div
        style={{
          // Top right, clear of the control bar and of subtitles at the bottom.
          position: "absolute",
          top: "max(80px, calc(env(safe-area-inset-top) + 56px))",
          right: "max(16px, env(safe-area-inset-right))",
          "max-width": "min(420px, calc(100% - 32px))",
          display: "flex",
          "flex-direction": "column",
          "align-items": "flex-end",
          gap: "8px",
          "z-index": 10005,
          "pointer-events": "none",
        }}
      >
        <Show
          when={props.open}
          fallback={
            <div
              aria-live="polite"
              role="log"
              aria-label="Watch companions"
              style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
            >
              <For each={visible(shown())}>
                {(reaction) => (
                  <p style={bubble}>
                    <strong>{author(reaction)}:</strong> {reaction.commentary}
                  </p>
                )}
              </For>
            </div>
          }
        >
          <section
            aria-label="Chat with watch companions"
            style={{
              ...bubble,
              width: "min(420px, calc(100vw - 32px))",
              display: "flex",
              "flex-direction": "column",
              gap: "10px",
              "pointer-events": "auto",
            }}
          >
            <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
              <strong style={{ flex: 1, "font-size": "0.9rem" }}>
                <Show when={listening().length > 0}>
                  <span aria-hidden="true" style={{ color: "#5be37d" }}>
                    ●{" "}
                  </span>
                </Show>
                {companions().length > 0 ? `Chat · ${companions().join(", ")}` : "Chat"}
              </strong>
              <button
                type="button"
                aria-label="Hide chat"
                title="Hide chat"
                onClick={() => props.onClose()}
                style={{ background: "none", border: "none", color: "#fff", cursor: "pointer" }}
              >
                ✕
              </button>
            </div>
            <fieldset
              aria-label="Companion comments"
              style={{ display: "flex", gap: "6px", margin: 0, padding: 0, border: "none" }}
            >
              <For each={MODES}>
                {(option) => (
                  <button
                    type="button"
                    aria-pressed={mode() === option.value}
                    onClick={() => void choose(option.value, frequency())}
                    style={choice(mode() === option.value)}
                  >
                    {option.label}
                  </button>
                )}
              </For>
            </fieldset>
            <Show when={mode() !== "off"}>
              <div style={{ display: "flex", gap: "6px", "align-items": "center" }}>
                <Show when={mode() === "on"}>
                  <fieldset
                    aria-label="How often"
                    style={{
                      display: "flex",
                      gap: "6px",
                      flex: 1,
                      margin: 0,
                      padding: 0,
                      border: "none",
                    }}
                  >
                    <For each={FREQUENCIES}>
                      {(option) => (
                        <button
                          type="button"
                          aria-pressed={frequency() === option.value}
                          onClick={() => void choose(mode(), option.value)}
                          style={choice(frequency() === option.value)}
                        >
                          {option.label}
                        </button>
                      )}
                    </For>
                  </fieldset>
                </Show>
                <button
                  type="button"
                  disabled={companions().length === 0}
                  onClick={() => void nudge()}
                  style={choice(false)}
                >
                  Comment now
                </button>
              </div>
            </Show>
            <Show when={companions().length === 0}>
              <p style={{ margin: 0, "font-size": "0.85rem", opacity: 0.8 }}>
                No companion has joined yet. Your messages will wait for one.
              </p>
            </Show>
            <div
              ref={log}
              role="log"
              aria-live="polite"
              aria-label="Watch companions"
              style={{
                "max-height": "38vh",
                "overflow-y": "auto",
                display: "flex",
                "flex-direction": "column",
                gap: "6px",
              }}
            >
              <For each={visible(history())}>
                {(reaction) => (
                  <p
                    style={{
                      margin: 0,
                      "align-self": reaction.fromViewer ? "flex-end" : "flex-start",
                      "max-width": "90%",
                      padding: "6px 10px",
                      "border-radius": "10px",
                      background: reaction.fromViewer
                        ? "rgba(80, 120, 255, 0.45)"
                        : "rgba(255,255,255,0.12)",
                    }}
                  >
                    <strong>{author(reaction)}:</strong> {reaction.commentary}
                  </p>
                )}
              </For>
            </div>
            <form onSubmit={send} style={{ display: "flex", gap: "6px" }}>
              <input
                type="text"
                maxLength={500}
                value={draft()}
                onInput={(e) => setDraft(e.currentTarget.value)}
                placeholder="Write to your companions"
                aria-label="Write to your companions"
                style={{ flex: 1, "min-width": 0 }}
              />
              <button type="submit" disabled={sending() || !draft().trim()}>
                Send
              </button>
            </form>
            <Show when={error()}>
              <p role="alert" style={{ margin: 0, color: "#ffb4b4", "font-size": "0.85rem" }}>
                {error()}
              </p>
            </Show>
          </section>
        </Show>
      </div>
    </Show>
  );
}
