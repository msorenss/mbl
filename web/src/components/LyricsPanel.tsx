import { createEffect, createMemo, createResource, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";

type Lyrics = components["schemas"]["LyricsResponse"];

/** Index of the line being sung at `ms`, or -1 before the first timed line. */
export function activeLine(lines: Lyrics["lines"], ms: number): number {
  let active = -1;
  for (let i = 0; i < lines.length; i++) {
    const start = lines[i].startMs;
    if (start === undefined || start === null) continue;
    if (start <= ms) active = i;
    else break;
  }
  return active;
}

/** Lyrics of a music track; synced lyrics follow playback. Renders nothing without lyrics. */
export default function LyricsPanel(props: { itemId: number; timeMs: number }) {
  const [lyrics] = createResource(
    () => props.itemId,
    async (id) => {
      const result = await client.GET("/api/v1/items/{id}/lyrics", {
        params: { path: { id } },
      });
      const data = result.data;
      return data && Array.isArray(data.lines) && data.lines.length > 0 ? data : null;
    },
  );
  const current = createMemo(() => {
    const data = lyrics();
    return data?.synced ? activeLine(data.lines, props.timeMs) : -1;
  });
  let list: HTMLOListElement | undefined;
  createEffect(() => {
    const index = current();
    if (index < 0 || !list) return;
    const line = list.children[index] as HTMLElement | undefined;
    line?.scrollIntoView?.({ block: "center", behavior: "smooth" });
  });

  return (
    <Show when={lyrics()}>
      {(data) => (
        <section
          aria-label="Lyrics"
          style={{
            "pointer-events": "auto",
            "max-height": "45vh",
            width: "min(560px, 90vw)",
            "overflow-y": "auto",
            "text-align": "center",
            "line-height": "1.7",
            "font-size": "18px",
            padding: "8px 12px",
          }}
        >
          <ol ref={list} style={{ "list-style": "none", margin: 0, padding: 0 }}>
            <For each={data().lines}>
              {(line, index) => (
                <li
                  aria-current={index() === current() ? "true" : undefined}
                  style={{
                    opacity: !data().synced || index() === current() ? 1 : 0.55,
                    "font-weight": index() === current() ? 700 : 400,
                    transition: "opacity 0.2s",
                    "min-height": "1.7em",
                  }}
                >
                  {line.text}
                </li>
              )}
            </For>
          </ol>
        </section>
      )}
    </Show>
  );
}
