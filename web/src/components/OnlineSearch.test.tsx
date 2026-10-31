import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import type { components } from "../api/schema";
import { OnlineSearch } from "./OnlineSearch";

vi.mock("./OnlineHistory", () => ({ OnlineHistory: () => null }));
vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
type Source = components["schemas"]["OnlineSource"];
const source = (id = "first", checked = false): Source => ({
  id,
  url: `https://example.org/${id}`,
  title: `${id} title`,
  summary: "Public web result",
  kind: "episode",
  seriesTitle: "Fixture show",
  seasonNumber: 2,
  episodeNumber: 3,
  episodes: [],
  availability: checked ? "media_detected" : "unchecked",
  checkedAt: checked ? Date.now() : null,
  mediaUrl: checked ? "https://example.org/video.mp4" : null,
  hasVideo: checked ? true : null,
  detail: "Source result",
});
let dispose: (() => void) | undefined;
const tick = () => vi.advanceTimersByTimeAsync(0);
function button(text: string) {
  const match = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!match) throw new Error(`Missing button: ${text}`);
  return match;
}
function input(id: string, value: string) {
  const el = document.getElementById(id) as HTMLInputElement;
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  return el;
}
async function search(title = "Some show") {
  input("online-query", title).form?.dispatchEvent(
    new Event("submit", { bubbles: true, cancelable: true }),
  );
  await tick();
}
async function mount() {
  dispose = render(() => createComponent(OnlineSearch, {}), document.body);
  await tick();
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { query: "Some show", provider: "Fixture search", results: [source()], indexed: [] },
    response: new Response(),
  } as never);
  vi.spyOn(client, "POST").mockResolvedValue({
    data: source("first", true),
    response: new Response(),
  } as never);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("searches only on submission and presents unchecked web hits without claiming playback", async () => {
  await mount();
  expect(client.GET).not.toHaveBeenCalled();
  await search();
  expect(client.GET).toHaveBeenCalledWith(
    "/api/v1/online/search",
    expect.objectContaining({ params: { query: { q: "Some show" } } }),
  );
  expect(document.body.textContent).toContain("Not checked");
  expect(document.body.textContent).toContain("Fixture search");
  expect(client.POST).not.toHaveBeenCalled();
  expect(document.querySelector("video")).toBeNull();
});

it("rechecks before preview and reports Playing only after a real media event", async () => {
  await mount();
  await search();
  document
    .querySelector("article button")
    ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await tick();
  expect(document.querySelector('section[aria-label="Checked source"]')?.textContent).toContain(
    "Season 2",
  );
  expect(document.querySelector("video")).toBeNull();
  button("Preview").click();
  await tick();
  expect(client.POST).toHaveBeenCalledTimes(2);
  const media = document.querySelector("video")!;
  expect(media.src).toBe("https://example.org/video.mp4");
  expect(document.querySelector('section[aria-label="Preview"]')?.textContent).not.toContain(
    "Playing",
  );
  media.dispatchEvent(new Event("playing"));
  expect(document.querySelector('section[aria-label="Preview"]')?.textContent).toContain("Playing");
  button("Close").click();
  await tick();
  expect(document.querySelector("video")).toBeNull();
  expect(media.hasAttribute("src")).toBe(false);
  expect(HTMLMediaElement.prototype.pause).toHaveBeenCalled();
});

it("does not offer preview when a previously verified source becomes restricted", async () => {
  vi.mocked(client.POST).mockResolvedValueOnce({
    data: { ...source(), availability: "restricted", detail: "Sign-in required" },
    response: new Response(),
  } as never);
  await mount();
  await search();
  document
    .querySelector("article button")
    ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await tick();
  expect(document.body.textContent).toContain("Access restricted or sign-in required");
  expect(
    [...document.querySelectorAll("button")].some((b) => b.textContent?.trim() === "Preview"),
  ).toBe(false);
  expect(document.querySelector("video")).toBeNull();
});

it("ignores an older search after a new title is submitted", async () => {
  let resolveOld!: (value: unknown) => void;
  let oldSignal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    oldSignal = (options as { signal: AbortSignal }).signal;
    return new Promise((resolve) => {
      resolveOld = resolve as (v: unknown) => void;
    }) as never;
  });
  await mount();
  await search("Old title");
  await search("New title");
  expect(oldSignal?.aborted).toBe(true);
  resolveOld({
    data: { query: "Old title", provider: "Old", results: [source("old")], indexed: [] },
    response: new Response(),
  });
  await tick();
  expect(document.body.textContent).not.toContain("old title");
  expect(document.body.textContent).toContain("first title");
});

it("rejects non-web source URLs and renders external titles as text", async () => {
  const hit = source();
  hit.title = '<img src="x" onerror="alert(1)">';
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { query: "A title", provider: "Fixture", results: [hit], indexed: [] },
    response: new Response(),
  } as never);
  await mount();
  await search();
  expect(document.querySelector("img")).toBeNull();
  expect(document.body.textContent).toContain(hit.title);
  input("online-source-url", "javascript:alert(1)").form?.dispatchEvent(
    new Event("submit", { bubbles: true, cancelable: true }),
  );
  await tick();
  expect(client.POST).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alert"]')).not.toBeNull();
});

it("shows HTTP failures and aborts outstanding calls on close", async () => {
  vi.mocked(client.GET).mockResolvedValueOnce({
    error: { error: { code: "unavailable", message: "Search unavailable" } },
    response: new Response(null, { status: 503 }),
  } as never);
  await mount();
  await search();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Search unavailable");
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return new Promise((_resolve, reject) => {
      signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")));
    }) as never;
  });
  await search("Retry title");
  dispose?.();
  dispose = undefined;
  await tick();
  expect(signal?.aborted).toBe(true);
});

it("stops a preview that never loads and explains the browser failure", async () => {
  await mount();
  await search();
  document
    .querySelector("article button")
    ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await tick();
  button("Preview").click();
  await tick();
  const media = document.querySelector("video")!;
  await vi.advanceTimersByTimeAsync(15000);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("could not play");
  expect(media.hasAttribute("src")).toBe(false);
});

it("lists season and episode metadata and checks the selected episode before any preview", async () => {
  vi.mocked(client.POST).mockResolvedValueOnce({
    data: {
      ...source("series"),
      availability: "external_page",
      mediaUrl: null,
      episodes: [
        {
          title: "Next episode",
          url: "https://example.org/s2e4",
          seasonNumber: 2,
          episodeNumber: 4,
        },
      ],
    },
    response: new Response(),
  } as never);
  await mount();
  await search();
  document
    .querySelector("article button")
    ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await tick();
  expect(document.querySelector("ol")?.textContent).toContain("Season 2 Episode 4: Next episode");
  document.querySelector<HTMLButtonElement>("ol button")?.click();
  await tick();
  expect(client.POST).toHaveBeenLastCalledWith(
    "/api/v1/online/inspect",
    expect.objectContaining({ body: { url: "https://example.org/s2e4" } }),
  );
  expect(document.querySelector("video")).toBeNull();
});

it("keeps a metadata-ready preview available while the user decides when to press play", async () => {
  await mount();
  await search();
  document
    .querySelector("article button")
    ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  await tick();
  button("Preview").click();
  await tick();
  const media = document.querySelector("video")!;
  media.dispatchEvent(new Event("loadedmetadata"));
  await vi.advanceTimersByTimeAsync(15000);
  expect(document.querySelector('[role="alert"]')).toBeNull();
  expect(media.hasAttribute("src")).toBe(true);
  expect(document.querySelector('section[aria-label="Preview"]')?.textContent).toContain(
    "Use the player controls",
  );
});
