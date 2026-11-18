import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import LyricsPanel, { activeLine } from "./LyricsPanel";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const lines = [
  { startMs: 1000, text: "First" },
  { startMs: 5000, text: "Second" },
  { startMs: 9000, text: "Third" },
];

it("finds the line being sung", () => {
  expect(activeLine(lines, 0)).toBe(-1);
  expect(activeLine(lines, 1000)).toBe(0);
  expect(activeLine(lines, 7000)).toBe(1);
  expect(activeLine(lines, 60_000)).toBe(2);
  expect(activeLine([{ text: "unsynced" }], 5000)).toBe(-1);
});

it("highlights synced lyrics as playback moves and hides when there are none", async () => {
  const get = vi.spyOn(client, "GET").mockResolvedValue({
    data: { itemId: 4, synced: true, source: "sidecar", lines },
    response: new Response(),
  } as never);
  const [time, setTime] = createSignal(0);
  dispose = render(
    () =>
      createComponent(LyricsPanel, {
        itemId: 4,
        get timeMs() {
          return time();
        },
      }),
    document.body,
  );
  await tick();
  expect(get).toHaveBeenCalledWith("/api/v1/items/{id}/lyrics", { params: { path: { id: 4 } } });
  expect(document.body.textContent).toContain("Second");
  expect(document.querySelector('[aria-current="true"]')).toBeNull();
  setTime(5500);
  expect(document.querySelector('[aria-current="true"]')?.textContent).toBe("Second");

  dispose?.();
  document.body.innerHTML = "";
  get.mockResolvedValue({ error: {}, response: new Response(null, { status: 404 }) } as never);
  dispose = render(() => createComponent(LyricsPanel, { itemId: 5, timeMs: 0 }), document.body);
  await tick();
  expect(document.querySelector('[aria-label="Lyrics"]')).toBeNull();
});
