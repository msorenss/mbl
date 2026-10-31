import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import type { components } from "../api/schema";
import { OnlineHistory } from "./OnlineHistory";
import { OnlinePlayer } from "./OnlinePlayer";

const hlsMock = vi.hoisted(() => ({ load: vi.fn(), attach: vi.fn(), destroy: vi.fn() }));
vi.mock("hls.js", () => ({
  default: class {
    static Events = { ERROR: "error" };
    static isSupported() {
      return true;
    }
    on() {}
    loadSource = hlsMock.load;
    attachMedia = hlsMock.attach;
    destroy = hlsMock.destroy;
  },
}));
vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
const source: components["schemas"]["OnlineSource"] = {
  id: "source",
  url: "https://example.org/show.m3u8",
  title: "Episode",
  summary: null,
  kind: "episode",
  seriesTitle: null,
  seasonNumber: 1,
  episodeNumber: 2,
  episodes: [],
  availability: "external_player",
  checkedAt: 1,
  mediaUrl: null,
  hasVideo: null,
  detail: "HLS",
};
const playback = {
  id: "session",
  streamUrl: "/api/v1/online/playback/session/index.m3u8",
  hls: true,
  position: 42,
  expiresAt: Date.now() + 600000,
};
let dispose: (() => void) | undefined;
const tick = () => vi.advanceTimersByTimeAsync(0);
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockReturnValue("probably");
  vi.spyOn(client, "POST").mockResolvedValue({ data: playback, response: new Response() } as never);
  vi.spyOn(client, "PATCH").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
});
afterEach(async () => {
  dispose?.();
  dispose = undefined;
  await tick();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("restores saved position, waits for real playing, persists progress and releases on close", async () => {
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await tick();
  const video = document.querySelector("video")!;
  Object.defineProperty(video, "duration", { value: 120, configurable: true });
  video.dispatchEvent(new Event("loadedmetadata"));
  expect(video.currentTime).toBe(42);
  expect(document.body.textContent).not.toContain("Playing");
  video.dispatchEvent(new Event("playing"));
  await tick();
  expect(document.body.textContent).toContain("Playing");
  expect(client.PATCH).toHaveBeenCalledWith(
    "/api/v1/online/playback/{id}",
    expect.objectContaining({
      body: { position: 42, duration: 120, ended: false, heartbeatOnly: false },
    }),
  );
  dispose();
  dispose = undefined;
  await tick();
  expect(video.hasAttribute("src")).toBe(false);
  expect(client.DELETE).toHaveBeenCalledWith(
    "/api/v1/online/playback/{id}",
    expect.objectContaining({ params: { path: { id: "session" } } }),
  );
});
it("clears buffered media when the authenticated heartbeat is rejected", async () => {
  vi.mocked(client.PATCH).mockResolvedValue({
    error: { error: { message: "Access revoked" } },
    response: new Response(null, { status: 403 }),
  } as never);
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await tick();
  const video = document.querySelector("video")!;
  Object.defineProperty(video, "duration", { value: 120, configurable: true });
  video.dispatchEvent(new Event("loadedmetadata"));
  await vi.advanceTimersByTimeAsync(10000);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Access revoked");
  expect(video.hasAttribute("src")).toBe(false);
});
it("releases a late created session after the player was closed", async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(client.POST).mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }) as never,
  );
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await tick();
  dispose();
  dispose = undefined;
  finish({ data: playback, response: new Response() });
  await tick();
  expect(client.DELETE).toHaveBeenCalled();
});
it("lists online history with a resume action", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: [{ source, position: 42, duration: 120, updatedAt: 1 }],
    response: new Response(),
  } as never);
  const play = vi.fn();
  dispose = render(() => createComponent(OnlineHistory, { onPlay: play }), document.body);
  await tick();
  const button = document.querySelector("button")!;
  expect(button.textContent).toContain("0:42");
  button.click();
  expect(play).toHaveBeenCalledWith(source);
});

it("keeps an idle ready session alive without claiming the user has watched it", async () => {
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await tick();
  const video = document.querySelector("video")!;
  Object.defineProperty(video, "duration", { value: 120, configurable: true });
  video.dispatchEvent(new Event("loadedmetadata"));
  await vi.advanceTimersByTimeAsync(10000);
  expect(client.PATCH).toHaveBeenCalledWith(
    "/api/v1/online/playback/{id}",
    expect.objectContaining({ body: expect.objectContaining({ heartbeatOnly: true }) }),
  );
  expect(document.body.textContent).not.toContain("Playing");
});

it("preserves completion when a final heartbeat or close follows ended", async () => {
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await tick();
  const video = document.querySelector("video")!;
  Object.defineProperty(video, "duration", { value: 120, configurable: true });
  video.dispatchEvent(new Event("loadedmetadata"));
  video.dispatchEvent(new Event("playing"));
  await tick();
  video.currentTime = 120;
  video.dispatchEvent(new Event("ended"));
  await tick();
  await vi.advanceTimersByTimeAsync(10000);
  dispose();
  dispose = undefined;
  await tick();
  expect(client.PATCH).toHaveBeenLastCalledWith(
    "/api/v1/online/playback/{id}",
    expect.objectContaining({
      body: expect.objectContaining({ ended: true, heartbeatOnly: false }),
    }),
  );
});

it("uses Hls.js with the local authenticated relay when native HLS is unavailable", async () => {
  vi.mocked(HTMLMediaElement.prototype.canPlayType).mockReturnValue("");
  hlsMock.load.mockClear();
  hlsMock.attach.mockClear();
  hlsMock.destroy.mockClear();
  dispose = render(
    () => createComponent(OnlinePlayer, { source, onClose: () => {} }),
    document.body,
  );
  await vi.waitFor(() => expect(hlsMock.load).toHaveBeenCalledWith(playback.streamUrl));
  expect(hlsMock.attach).toHaveBeenCalledWith(document.querySelector("video"));
  dispose();
  dispose = undefined;
  await tick();
  expect(hlsMock.destroy).toHaveBeenCalled();
});
