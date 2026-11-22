import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import {
  canSpeak,
  readAloud,
  readAloudProblem,
  setReadAloud,
  speak,
  stopSpeaking,
} from "../utils/speech";
import MicButton from "./MicButton";

type Reaction = components["schemas"]["CompanionReaction"];
type Mode = components["schemas"]["CompanionMode"];
type Frequency = components["schemas"]["CompanionFrequency"];
type Frames = components["schemas"]["CompanionFrames"];

const VISIBLE_MS = 12_000;
const HISTORY = 30;
const MODE_KEY = "mbl.companion.mode";
/** Ready-made questions about the scene; `text` is what the companion receives. */
const QUICK_QUESTIONS = [
  { label: "Who is on screen?", text: "Who is on screen right now?" },
  { label: "What just happened?", text: "What just happened?" },
  { label: "The story so far", text: "Remind me of the story so far, without spoilers." },
];
const FREQUENCY_KEY = "mbl.companion.frequency";
const FREQUENCIES: { value: Frequency; label: string }[] = [
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
];
const FRAMES_KEY = "mbl.companion.frames";
const FRAMES: { value: Frames; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "30s", label: "Every 30 seconds" },
  { value: "1m", label: "Every minute" },
  { value: "5m", label: "Every 5 minutes" },
];

function storedFrames(): Frames {
  try {
    const value = localStorage.getItem(FRAMES_KEY);
    if (FRAMES.some((f) => f.value === value)) return value as Frames;
  } catch {}
  return "off";
}

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
  const { t, locale } = useI18n();
  const [speakReplies, setSpeakReplies] = createSignal(readAloud("player"));
  const [shown, setShown] = createSignal<Reaction[]>([]);
  const [history, setHistory] = createSignal<Reaction[]>([]);
  const [companions, setCompanions] = createSignal<string[]>([]);
  const [mode, setMode] = createSignal<Mode>(storedMode());
  const [frequency, setFrequency] = createSignal<Frequency>(storedFrequency());
  const [frames, setFrames] = createSignal<Frames>(storedFrames());
  const [listening, setListening] = createSignal<string[]>([]);
  const [assistant, setAssistant] = createSignal(false);
  // Someone will hear the viewer: a connected companion or the built-in assistant.
  const heard = () => companions().length > 0 || assistant();
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
    if (speakReplies()) for (const r of fresh.filter((r) => !r.fromViewer)) sayAloud(r.commentary);
  };

  /** Read a companion's reply aloud, with the film quieter meanwhile. */
  const sayAloud = (text: string) => {
    const video = document.querySelector("video");
    let restore: number | undefined;
    speak(text, locale(), {
      onStart: () => {
        if (video && restore === undefined) {
          restore = video.volume;
          video.volume = Math.min(video.volume, 0.3);
        }
      },
      onEnd: () => {
        if (video && restore !== undefined && !window.speechSynthesis?.speaking)
          video.volume = restore;
      },
    });
  };
  onCleanup(stopSpeaking);

  createEffect(
    on(
      () => props.sessionId,
      (sessionId) => {
        setShown([]);
        setHistory([]);
        setCompanions([]);
        setListening([]);
        setAssistant(false);
        setError("");
        if (!sessionId) return;
        // The viewer's last choice follows them to the next session.
        if (mode() !== "on" || frequency() !== "medium" || frames() !== "off")
          void client
            .PUT("/api/v1/companion/sessions/{id}/mode", {
              params: { path: { id: sessionId } },
              body: { mode: mode(), frequency: frequency(), frames: frames() },
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
            setAssistant(feed.assistantAvailable === true);
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
  // Read-aloud already on from an earlier visit: still say when this browser cannot speak.
  createEffect(
    on([() => props.open, speakReplies], ([isOpen, speaking]) => {
      if (isOpen && speaking)
        void readAloudProblem(locale()).then((problem) => problem && setError(problem));
    }),
  );
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

  const chooseFrames = async (next: Frames) => {
    const sessionId = props.sessionId;
    if (!sessionId || next === frames()) return;
    const previous = frames();
    setFrames(next);
    setError("");
    try {
      const result = await client.PUT("/api/v1/companion/sessions/{id}/mode", {
        params: { path: { id: sessionId } },
        body: { mode: mode(), frequency: frequency(), frames: next },
      });
      if (!result.response.ok) throw new Error();
      store(FRAMES_KEY, next);
    } catch {
      setFrames(previous);
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
    await say(draft().trim(), true);
  };

  /** Posts a viewer message; `fromDraft` clears the input once sent. */
  const say = async (text: string, fromDraft: boolean) => {
    const sessionId = props.sessionId;
    if (!sessionId || !text || sending()) return;
    setSending(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/companion/sessions/{id}/messages", {
        params: { path: { id: sessionId } },
        body: { text },
      });
      if (!result.data) throw new Error();
      if (fromDraft) setDraft("");
      addEntries([result.data]);
    } catch {
      setError("Could not send the message.");
    } finally {
      setSending(false);
    }
  };

  const author = (r: Reaction) => (r.fromViewer ? "You" : r.botName);

  return (
    // Also open without a session, to say why there is nothing to chat in yet.
    <Show when={props.sessionId || props.open}>
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
            <Show
              when={props.sessionId}
              fallback={
                <p role="status" style={{ margin: 0, "font-size": "0.85rem", opacity: 0.8 }}>
                  The chat starts when playback has started.
                </p>
              }
            >
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
                    disabled={!heard()}
                    onClick={() => void nudge()}
                    style={choice(false)}
                  >
                    Comment now
                  </button>
                </div>
              </Show>
              <label style={{ display: "flex", gap: "6px", "align-items": "center" }}>
                <span style={{ flex: 1 }}>Screenshots to the AI</span>
                <select
                  value={frames()}
                  onChange={(e) => void chooseFrames(e.currentTarget.value as Frames)}
                >
                  <For each={FRAMES}>
                    {(option) => (
                      <option value={option.value} selected={frames() === option.value}>
                        {option.label}
                      </option>
                    )}
                  </For>
                </select>
              </label>
              <Show when={frames() !== "off"}>
                <p style={{ margin: 0, "font-size": "0.85rem", opacity: 0.8 }}>
                  A picture of the scene goes to the AI provider, never one from later than where
                  you are.
                </p>
              </Show>
              <Show when={!heard()}>
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
              <Show when={heard() && mode() !== "off"}>
                <fieldset
                  aria-label="Quick questions"
                  style={{
                    display: "flex",
                    gap: "6px",
                    "flex-wrap": "wrap",
                    margin: 0,
                    padding: 0,
                    border: "none",
                  }}
                >
                  <For each={QUICK_QUESTIONS}>
                    {(question) => (
                      <button
                        type="button"
                        disabled={sending()}
                        // Sent in the viewer's language, so the answer comes back in it.
                        onClick={() => void say(t(question.text), false)}
                        style={{ ...choice(false), flex: "none" }}
                      >
                        {question.label}
                      </button>
                    )}
                  </For>
                </fieldset>
              </Show>
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
                <MicButton
                  language={locale()}
                  onText={(text) => void say(text, false)}
                  onError={setError}
                />
              </form>
              <Show when={canSpeak()}>
                <label style={{ display: "flex", gap: "6px", "align-items": "center" }}>
                  <input
                    type="checkbox"
                    checked={speakReplies()}
                    onChange={(e) => {
                      const on = e.currentTarget.checked;
                      setSpeakReplies(on);
                      setReadAloud("player", on);
                      if (!on) stopSpeaking();
                      // A browser without voices would stay silent; say so.
                      else
                        void readAloudProblem(locale()).then(
                          (problem) => problem && setError(problem),
                        );
                    }}
                  />
                  Read replies aloud
                </label>
              </Show>
              <Show when={error()}>
                <p role="alert" style={{ margin: 0, color: "#ffb4b4", "font-size": "0.85rem" }}>
                  {error()}
                </p>
              </Show>
            </Show>
          </section>
        </Show>
      </div>
    </Show>
  );
}
