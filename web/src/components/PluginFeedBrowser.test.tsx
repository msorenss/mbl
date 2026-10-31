import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { PluginFeedBrowser } from "./PluginFeedBrowser";

vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
const entry = (title: string) => ({
  id: title,
  pluginId: "webdrama",
  title,
  kind: "episode",
  subtitles: [],
});
const feeds = [
  { id: "a", title: "First feed", items: [entry("First demo")] },
  { id: "b", title: "Second feed", items: [entry("Second demo")] },
];
let dispose: (() => void) | undefined;
const tick = () => vi.advanceTimersByTimeAsync(0);
const button = (label: string) => {
  const found = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === label,
  );
  if (!found) throw new Error(`Button not found: ${label}`);
  return found;
};
async function mount() {
  dispose = render(
    () => createComponent(PluginFeedBrowser, { pluginId: "webdrama" }),
    document.body,
  );
  await tick();
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(client, "GET").mockResolvedValue({ data: feeds, response: new Response() } as never);
});
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("explains demo status and never offers a working Play button for sample entries", async () => {
  await mount();
  expect(document.body.textContent).toContain("First demo");
  expect(document.querySelector('[role="note"]')?.textContent).toContain("cannot be played");
  expect(button("Demo — playback unavailable").disabled).toBe(true);
  button("Demo — playback unavailable").click();
  expect(client.GET).toHaveBeenCalledTimes(1);
  expect(document.body.textContent).not.toContain("Playing");
});

it("shows an HTTP failure inside the dialog and can retry it", async () => {
  vi.mocked(client.GET).mockResolvedValueOnce({
    error: { error: { code: "forbidden", message: "Provider is disabled" } },
    response: new Response(null, { status: 403 }),
  } as never);
  await mount();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Provider is disabled");
  button("Retry").click();
  await tick();
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(document.body.textContent).toContain("First demo");
});

it("clears old entries while switching feeds and ignores a superseded response", async () => {
  await mount();
  let oldResolve!: (result: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as { signal?: AbortSignal } | undefined)?.signal;
    return new Promise((resolve) => {
      oldResolve = resolve as (value: unknown) => void;
    }) as never;
  });
  button("Second feed").click();
  await tick();
  expect(document.body.textContent).not.toContain("First demo");
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: [entry("New first episode")],
    response: new Response(),
  } as never);
  button("First feed").click();
  await tick();
  expect(signal?.aborted).toBe(true);
  oldResolve({ data: [entry("Stale second episode")], response: new Response() });
  await tick();
  expect(document.body.textContent).toContain("New first episode");
  expect(document.body.textContent).not.toContain("Stale second episode");
});

it("aborts catalogue loading when the dialog is closed", async () => {
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as { signal?: AbortSignal } | undefined)?.signal;
    return new Promise((_resolve, reject) => {
      signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), {
        once: true,
      });
    }) as never;
  });
  await mount();
  dispose?.();
  dispose = undefined;
  await tick();
  expect(signal?.aborted).toBe(true);
});
