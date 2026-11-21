import { A } from "@solidjs/router";
import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { Portal } from "solid-js/web";
import client from "../api/client";
import type { components } from "../api/schema";
import { usePlayer } from "../context/PlayerContext";
import { useI18n } from "../i18n";
import { onAssistantAsk, setAssistantAvailable } from "../utils/assistantBridge";
import { canSpeak, readAloud, setReadAloud, speak, stopSpeaking } from "../utils/speech";
import MicButton from "./MicButton";
import { AppIcon } from "./ui";

type Conversation = components["schemas"]["Conversation"];
type Mention = components["schemas"]["ItemMention"];

interface Line {
  role: "user" | "assistant";
  content: string;
  failed?: boolean;
  items?: Mention[];
}

/** Kinds that start playback directly; others open their page. */
const PLAYABLE = new Set(["movie", "episode", "track", "video"]);

/** `(event, data)` pairs from server-sent events, fed with text as it arrives. */
export function sseParser(onEvent: (name: string, data: unknown) => void) {
  let buffer = "";
  return (chunk: string) => {
    buffer += chunk.replace(/\r\n/g, "\n");
    let end = buffer.indexOf("\n\n");
    while (end >= 0) {
      const block = buffer.slice(0, end);
      buffer = buffer.slice(end + 2);
      let name = "message";
      const data: string[] = [];
      for (const line of block.split("\n")) {
        if (line.startsWith("event:")) name = line.slice(6).trim();
        else if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
      }
      if (data.length > 0) {
        try {
          onEvent(name, JSON.parse(data.join("\n")));
        } catch {
          // Not JSON: ignore (keep-alive comments carry no data).
        }
      }
      end = buffer.indexOf("\n\n");
    }
  };
}

/**
 * The assistant chat, available on every page when an admin has turned the assistant on.
 * Renders a button for the top bar; the panel opens beside the page.
 */
export default function AssistantChat() {
  const { playItem } = usePlayer();
  const [available, setAvailable] = createSignal(false);
  const [open, setOpen] = createSignal(false);
  const [conversations, setConversations] = createSignal<Conversation[]>([]);
  const [current, setCurrent] = createSignal<number>();
  const [lines, setLines] = createSignal<Line[]>([]);
  const [draft, setDraft] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const { locale } = useI18n();
  const [speakReplies, setSpeakReplies] = createSignal(readAloud("assistant"));
  onCleanup(stopSpeaking);
  let log: HTMLDivElement | undefined;
  let input: HTMLTextAreaElement | undefined;
  let abort: AbortController | undefined;

  onMount(async () => {
    const result = await client.GET("/api/v1/assistant/status").catch(() => undefined);
    setAvailable(result?.data?.available === true);
    setAssistantAvailable(available());
  });
  onCleanup(() => {
    abort?.abort();
    setAssistantAvailable(false);
  });

  const loadList = async () => {
    const result = await client.GET("/api/v1/assistant/conversations").catch(() => undefined);
    if (result?.data) setConversations(result.data);
  };

  createEffect(
    on(open, (isOpen) => {
      if (!isOpen) return;
      void loadList();
      queueMicrotask(() => input?.focus());
    }),
  );

  // Newest message in view.
  createEffect(
    on([lines, open], () => {
      if (log) log.scrollTop = log.scrollHeight;
    }),
  );

  const startNew = () => {
    abort?.abort();
    setCurrent(undefined);
    setLines([]);
    setError("");
    input?.focus();
  };

  const openConversation = async (id: number) => {
    abort?.abort();
    setError("");
    const result = await client
      .GET("/api/v1/assistant/conversations/{id}", { params: { path: { id } } })
      .catch(() => undefined);
    if (!result?.data) {
      setError("Could not open the conversation.");
      return;
    }
    setCurrent(id);
    setLines(
      result.data.messages.map((m) => ({
        role: m.role === "assistant" ? "assistant" : "user",
        content: m.content,
        items: m.items,
      })),
    );
  };

  const remove = async () => {
    const id = current();
    if (id === undefined) return;
    const result = await client
      .DELETE("/api/v1/assistant/conversations/{id}", { params: { path: { id } } })
      .catch(() => undefined);
    if (!result?.response.ok) {
      setError("Could not delete the conversation.");
      return;
    }
    startNew();
    void loadList();
  };

  // Questions asked from the command palette start a new chat. Registered after setup, since
  // a question asked before this chat loaded is delivered at once.
  onMount(() =>
    onCleanup(
      onAssistantAsk((text) => {
        if (!available()) return;
        startNew();
        setOpen(true);
        setDraft(text);
        void send();
      }),
    ),
  );

  const updateLast = (change: (line: Line) => Line) =>
    setLines((list) => [...list.slice(0, -1), change(list[list.length - 1])]);

  const send = async (event?: Event) => {
    event?.preventDefault();
    const text = draft().trim();
    if (!text || busy()) return;
    setDraft("");
    setError("");
    setBusy(true);
    setLines((list) => [
      ...list,
      { role: "user", content: text },
      { role: "assistant", content: "" },
    ]);
    abort = new AbortController();
    try {
      const result = await client.POST("/api/v1/assistant/chat", {
        body: { conversationId: current(), text },
        parseAs: "stream",
        signal: abort.signal,
      });
      if (!result.response.ok || !result.data) {
        const status = result.response.status;
        throw new Error(
          status === 503
            ? "The assistant is turned off."
            : status === 429
              ? "You have asked a lot this hour. Try again later."
              : "Could not reach the assistant.",
        );
      }
      let failure = "";
      const feed = sseParser((name, data) => {
        const d = data as Record<string, unknown>;
        if (name === "conversation") setCurrent(d.id as number);
        else if (name === "delta")
          updateLast((l) => ({ ...l, content: l.content + String(d.text ?? "") }));
        else if (name === "done") {
          const message = d.message as { content?: string; items?: Mention[] } | undefined;
          updateLast((l) => ({
            ...l,
            content: message?.content ?? l.content,
            items: message?.items ?? [],
          }));
          if (speakReplies() && message?.content) speak(message.content, locale());
          // The person asked to watch something here.
          const play = d.play as Mention | null | undefined;
          if (play) {
            playItem({ itemId: play.id, title: play.title });
            setOpen(false);
          }
        } else if (name === "error") failure = String(d.message ?? "");
      });
      const reader = (result.data as ReadableStream<Uint8Array>).getReader();
      const decoder = new TextDecoder();
      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        feed(decoder.decode(value, { stream: true }));
      }
      if (failure) throw new Error(failure);
      void loadList();
    } catch (e) {
      if (abort?.signal.aborted) return;
      const message =
        e instanceof Error && e.message ? e.message : "Could not reach the assistant.";
      updateLast((l) => ({ ...l, content: message, failed: true }));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Show when={available()}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open()}
        aria-label="Assistant"
        title="Assistant"
        style={{
          background: open() ? "var(--accent)" : "var(--bg-card)",
          border: "1px solid var(--border)",
          color: open() ? "#fff" : "var(--text-primary)",
          height: "36px",
          padding: "0 12px",
          "border-radius": "var(--radius-full)",
          cursor: "pointer",
          display: "inline-flex",
          "align-items": "center",
          gap: "6px",
          "font-size": "var(--font-size-xs)",
        }}
      >
        <AppIcon name="chat" size={16} />
        <span class="mbl-desktop-only">Assistant</span>
      </button>
      <Show when={open()}>
        <Portal>
          <section
            aria-label="Assistant"
            style={{
              position: "fixed",
              top: "56px",
              right: "0",
              bottom: "0",
              width: "min(420px, 100vw)",
              background: "var(--bg-surface)",
              "border-left": "1px solid var(--border)",
              "box-shadow": "var(--shadow-lg, 0 0 24px rgba(0,0,0,0.3))",
              display: "flex",
              "flex-direction": "column",
              "z-index": 40,
              "padding-bottom": "env(safe-area-inset-bottom)",
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") setOpen(false);
            }}
          >
            <div
              style={{
                display: "flex",
                gap: "8px",
                "align-items": "center",
                padding: "10px 12px",
                "border-bottom": "1px solid var(--border)",
              }}
            >
              <select
                aria-label="Recent chats"
                onChange={(e) => {
                  const id = Number(e.currentTarget.value);
                  if (id) void openConversation(id);
                  else startNew();
                }}
                style={{ flex: 1, "min-width": 0 }}
              >
                <option value="" selected={current() === undefined}>
                  New chat
                </option>
                <For each={conversations()}>
                  {(c) => (
                    <option value={c.id} selected={c.id === current()}>
                      {c.title}
                    </option>
                  )}
                </For>
              </select>
              <Show when={current() !== undefined}>
                <button type="button" onClick={() => void remove()} title="Delete chat">
                  🗑
                </button>
              </Show>
              <button
                type="button"
                onClick={() => setOpen(false)}
                aria-label="Close"
                title="Close"
                style={{ background: "none", border: "none", cursor: "pointer" }}
              >
                ✕
              </button>
            </div>
            <div
              ref={log}
              role="log"
              aria-live="polite"
              style={{
                flex: 1,
                "overflow-y": "auto",
                padding: "12px",
                display: "flex",
                "flex-direction": "column",
                gap: "8px",
              }}
            >
              <Show when={lines().length === 0}>
                <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
                  Ask about your library: what to watch next, or what else there is with an actor
                  you like.
                </p>
              </Show>
              <For each={lines()}>
                {(line) => (
                  <>
                    <p
                      style={{
                        margin: 0,
                        "align-self": line.role === "user" ? "flex-end" : "flex-start",
                        "max-width": "88%",
                        padding: "8px 12px",
                        "border-radius": "12px",
                        "white-space": "pre-wrap",
                        "overflow-wrap": "anywhere",
                        background:
                          line.role === "user"
                            ? "var(--accent)"
                            : "var(--bg-card, rgba(0,0,0,0.05))",
                        color: line.failed
                          ? "var(--danger)"
                          : line.role === "user"
                            ? "#fff"
                            : "var(--text-primary)",
                      }}
                    >
                      {line.content || "Thinking…"}
                    </p>
                    <Show when={line.items?.length}>
                      <ul
                        aria-label="Titles in your library"
                        style={{
                          margin: 0,
                          padding: 0,
                          "list-style": "none",
                          display: "flex",
                          "flex-direction": "column",
                          gap: "4px",
                          "max-width": "88%",
                        }}
                      >
                        <For each={line.items}>
                          {(item) => (
                            <li
                              style={{
                                display: "flex",
                                "align-items": "center",
                                gap: "8px",
                                padding: "6px 10px",
                                "border-radius": "10px",
                                border: "1px solid var(--border)",
                                "font-size": "13px",
                              }}
                            >
                              <span style={{ flex: 1, "min-width": 0 }}>
                                {item.year ? `${item.title} (${item.year})` : item.title}
                              </span>
                              <A href={`/item/${item.id}`} onClick={() => setOpen(false)}>
                                View
                              </A>
                              <Show when={PLAYABLE.has(item.kind)}>
                                <button
                                  type="button"
                                  onClick={() => {
                                    playItem({ itemId: item.id, title: item.title });
                                    setOpen(false);
                                  }}
                                >
                                  Play
                                </button>
                              </Show>
                            </li>
                          )}
                        </For>
                      </ul>
                    </Show>
                  </>
                )}
              </For>
            </div>
            <Show when={error()}>
              <p role="alert" style={{ margin: "0 12px", color: "var(--danger)" }}>
                {error()}
              </p>
            </Show>
            <form
              onSubmit={send}
              style={{
                display: "flex",
                gap: "8px",
                padding: "12px",
                "border-top": "1px solid var(--border)",
              }}
            >
              <textarea
                ref={input}
                rows={2}
                maxLength={2000}
                value={draft()}
                onInput={(e) => setDraft(e.currentTarget.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && !e.shiftKey) void send(e);
                }}
                placeholder="Ask the assistant"
                aria-label="Ask the assistant"
                style={{ flex: 1, "min-width": 0, resize: "none" }}
              />
              <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
                <button type="submit" disabled={busy() || !draft().trim()}>
                  Send
                </button>
                <MicButton
                  language={locale()}
                  onText={(text) => {
                    setDraft(text);
                    void send();
                  }}
                  onError={setError}
                  style={{ border: "1px solid var(--border)" }}
                />
              </div>
            </form>
            <Show when={canSpeak()}>
              <label
                style={{
                  display: "flex",
                  gap: "6px",
                  "align-items": "center",
                  padding: "0 12px 12px",
                  "font-size": "13px",
                }}
              >
                <input
                  type="checkbox"
                  checked={speakReplies()}
                  onChange={(e) => {
                    const on = e.currentTarget.checked;
                    setSpeakReplies(on);
                    setReadAloud("assistant", on);
                    if (!on) stopSpeaking();
                  }}
                />
                Read replies aloud
              </label>
            </Show>
          </section>
        </Portal>
      </Show>
    </Show>
  );
}
