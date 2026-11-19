import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";

type Reaction = components["schemas"]["CompanionReaction"];

const VISIBLE_MS = 12_000;

/** Shows what connected AI watch companions say about the current playback session. */
export default function CompanionReactions(props: { sessionId: string | undefined }) {
  const [shown, setShown] = createSignal<Reaction[]>([]);
  const [companions, setCompanions] = createSignal<string[]>([]);

  createEffect(
    on(
      () => props.sessionId,
      (sessionId) => {
        setShown([]);
        setCompanions([]);
        if (!sessionId) return;
        // Only reactions sent after the player opened; older ones are not news.
        let after = Date.now();
        let busy = false;
        const poll = async () => {
          if (busy) return;
          busy = true;
          try {
            const result = await client.GET("/api/v1/companion/sessions/{id}/reactions", {
              params: { path: { id: sessionId }, query: { after } },
            });
            const feed = result.data;
            if (!feed) return;
            setCompanions(feed.companions);
            if (feed.reactions.length > 0) {
              after = Math.max(after, ...feed.reactions.map((r) => r.createdAt));
              setShown((list) => [...list, ...feed.reactions].slice(-3));
            }
          } catch {
          } finally {
            busy = false;
            const now = Date.now();
            setShown((list) => list.filter((r) => now - r.createdAt < VISIBLE_MS));
          }
        };
        const timer = setInterval(() => void poll(), 4000);
        onCleanup(() => clearInterval(timer));
      },
    ),
  );

  return (
    <Show when={companions().length > 0}>
      <div
        aria-live="polite"
        role="log"
        aria-label="Watch companions"
        style={{
          position: "absolute",
          right: "24px",
          bottom: "120px",
          "max-width": "min(420px, 80vw)",
          display: "flex",
          "flex-direction": "column",
          gap: "8px",
          "z-index": 10005,
          "pointer-events": "none",
        }}
      >
        <For each={shown()}>
          {(reaction) => (
            <p
              style={{
                margin: 0,
                padding: "10px 14px",
                "border-radius": "12px",
                background: "rgba(20, 20, 24, 0.85)",
                color: "#fff",
                "font-size": "0.95rem",
              }}
            >
              <strong>{reaction.botName}:</strong> {reaction.commentary}
            </p>
          )}
        </For>
      </div>
    </Show>
  );
}
