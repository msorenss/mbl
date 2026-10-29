import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { Player } from "./Player";

type Double = {
  config: Record<string, unknown>;
  handlers: Map<string, (event: string, data?: unknown) => void>;
  loadSource: ReturnType<typeof vi.fn>;
  startLoad: ReturnType<typeof vi.fn>;
  attachMedia: ReturnType<typeof vi.fn>;
  stopLoad: ReturnType<typeof vi.fn>;
  recoverMediaError: ReturnType<typeof vi.fn>;
  destroy: ReturnType<typeof vi.fn>;
  emit: (type: string, code?: number, details?: string, fatal?: boolean) => void;
};
const sdk = vi.hoisted(() => ({ instances: [] as Double[], failOnLoad: false }));
vi.mock("hls.js", () => ({
  default: class {
    static isSupported = () => true;
    static Events = { MANIFEST_PARSED: "manifest", ERROR: "error", FRAG_BUFFERED: "buffered" };
    handlers = new Map<string, (event: string, data?: unknown) => void>();
    config: Record<string, unknown>;
    constructor(config: Record<string, unknown>) {
      this.config = config;
      sdk.instances.push(this);
    }
    on = vi.fn((event: string, callback: (event: string, data?: unknown) => void) =>
      this.handlers.set(event, callback),
    );
    emit = (type: string, code = 503, details = "fragLoadError", fatal = true) => {
      this.handlers.get("error")?.("error", { type, fatal, details, response: { code } });
    };
    loadSource = vi.fn(() => {
      if (sdk.failOnLoad) this.emit("mediaError");
    });
    startLoad = vi.fn();
    attachMedia = vi.fn();
    stopLoad = vi.fn();
    recoverMediaError = vi.fn();
    destroy = vi.fn();
  },
}));
vi.mock("../context/ThemeContext", () => ({
  useTheme: () => ({
    subtitleStyle: () => ({ size: "md", background: "dim", color: "white" }),
    nightMode: () => false,
    setNightMode: vi.fn(),
    audioOffsetMs: () => 250,
    setAudioOffsetMs: vi.fn(),
  }),
}));
vi.mock("../api/deviceProfile", () => ({ getPreferredProfile: () => "firefox" }));
vi.mock("../utils/cast", () => ({
  castManager: {
    setCommandHandler: () => vi.fn(),
    disposeSession: vi.fn(),
    subscribe: () => vi.fn(),
  },
}));
const playback = {
  sessionId: "old",
  method: "transcode",
  url: "/api/v1/stream/hls/old/master.m3u8",
  startMs: 5000,
  durationMs: 600000,
  subtitles: [],
  markers: [],
  partId: 2,
  audioOffsetMs: 250,
  audioStreamIndex: 3,
  burnedSubtitleStreamIndex: 5,
  audioTracks: [{ index: 3, codec: "aac", language: "sv", isDefault: true }],
  subtitleOptions: [{ index: 5, codec: "pgs", isDefault: true, requiresTranscode: true }],
};
let dispose: (() => void) | undefined;
let container: HTMLDivElement;
let starts: number;
beforeEach(() => {
  vi.useFakeTimers();
  sdk.instances = [];
  sdk.failOnLoad = false;
  starts = 0;
  container = document.createElement("div");
  document.body.append(container);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(client, "POST").mockImplementation(
    (path) =>
      Promise.resolve({
        data:
          path === "/api/v1/playback"
            ? { ...playback, sessionId: starts++ ? "replacement" : "old" }
            : undefined,
        response: new Response(),
      }) as never,
  );
});
afterEach(async () => {
  dispose?.();
  dispose = undefined;
  await vi.advanceTimersByTimeAsync(0);
  container.remove();
  vi.restoreAllMocks();
  vi.useRealTimers();
});
async function mount() {
  dispose = render(
    () =>
      createComponent(Player, {
        itemId: 7,
        initialStartMs: 5000,
        audioStreamIndex: 3,
        subtitleStreamIndex: 5,
        onClose: vi.fn(),
      }),
    container,
  );
  await vi.advanceTimersByTimeAsync(0);
  return sdk.instances[0];
}
function videoAt(time: number, paused = false) {
  const video = container.querySelector("video") as HTMLVideoElement;
  Object.defineProperties(video, {
    readyState: { configurable: true, value: 4 },
    paused: { configurable: true, value: paused },
  });
  video.currentTime = time;
  return video;
}
function retryButton() {
  return Array.from(container.querySelectorAll("button")).find((b) =>
    /Retry playback|Resume playback/.test(b.textContent ?? ""),
  );
}
function stopCalls() {
  return vi
    .mocked(client.POST)
    .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop");
}
it("coalesces network errors and stops after two application retries at the current position", async () => {
  const hls = await mount();
  videoAt(94).dispatchEvent(new Event("playing"));
  hls.emit("networkError", 503, "fragLoadError", false);
  await vi.advanceTimersByTimeAsync(2000);
  expect(hls.startLoad).not.toHaveBeenCalled();
  hls.emit("networkError");
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(1000);
  expect(hls.startLoad).toHaveBeenLastCalledWith(94);
  expect(container.textContent).toContain("Retrying (1/2)");
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(3000);
  expect(hls.startLoad).toHaveBeenCalledTimes(2);
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(0);
  expect(retryButton()).toBeDefined();
  expect(stopCalls()).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(60000);
  expect(hls.startLoad).toHaveBeenCalledTimes(2);
});
it("reloads a failed master manifest and keeps the requested start before metadata exists", async () => {
  const hls = await mount();
  hls.emit("networkError", 503, "manifestLoadError");
  await vi.advanceTimersByTimeAsync(1000);
  expect(hls.loadSource).toHaveBeenCalledTimes(2);
  expect(hls.loadSource).toHaveBeenLastCalledWith(playback.url);
  expect(hls.config.startPosition).toBe(5);
  expect(hls.startLoad).toHaveBeenLastCalledWith(5);
  expect(hls.config.fragLoadPolicy).toMatchObject({
    default: { maxLoadTimeMs: 25000, timeoutRetry: null },
  });
});
it.each([401, 403, 404, 410, 422, 429])(
  "stops HTTP %s without an automatic retry loop",
  async (code) => {
    const hls = await mount();
    hls.emit("networkError", code);
    await vi.advanceTimersByTimeAsync(60000);
    expect(hls.startLoad).not.toHaveBeenCalled();
    expect(stopCalls()).toHaveLength(1);
    expect(retryButton()?.textContent).toContain(
      code === 404 || code === 410 ? "Resume playback" : "Retry playback",
    );
  },
);
it("releases failed capacity before a manual restart and retains position, tracks, sync and pause", async () => {
  const hls = await mount();
  const video = videoAt(123);
  video.dispatchEvent(new Event("playing"));
  await vi.advanceTimersByTimeAsync(0);
  let release!: (value: unknown) => void;
  vi.mocked(client.POST).mockImplementation((path) => {
    if (path === "/api/v1/playback/{session}/stop")
      return new Promise((resolve) => {
        release = resolve;
      }) as never;
    return Promise.resolve({
      data: { ...playback, sessionId: "replacement", startMs: 123000 },
      response: new Response(),
    }) as never;
  });
  hls.emit("mediaError");
  video.currentTime = 0; // Browser/MediaSource teardown may reset the element's time.
  retryButton()?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(starts).toBe(1);
  expect(sdk.instances).toHaveLength(1);
  release({ response: new Response() });
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenLastCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        startMs: 123000,
        audioOffsetMs: 250,
        audioStreamIndex: 3,
        subtitleStreamIndex: 5,
        waitForReady: true,
      }),
    }),
  );
  expect(stopCalls()).toHaveLength(1);
  expect(hls.recoverMediaError).not.toHaveBeenCalled();
  expect(sdk.instances[1].config.startPosition).toBe(123);
  sdk.instances[1].handlers.get("manifest")?.("manifest");
  expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
  expect(container.textContent).toContain("Press Play to continue");
});
it("ends recovery after its deadline even if no further SDK error arrives", async () => {
  const hls = await mount();
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(1000);
  const video = videoAt(5);
  video.dispatchEvent(new Event("playing")); // An event alone must not reset the deadline.
  await vi.advanceTimersByTimeAsync(29000);
  expect(container.textContent).toContain("The stream could not be loaded");
  expect(stopCalls()).toHaveLength(1);
});
it("accepts real playback recovery without resetting the per-source retry budget", async () => {
  const hls = await mount();
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(1000);
  const video = videoAt(12);
  video.dispatchEvent(new Event("playing"));
  video.dispatchEvent(new Event("timeupdate"));
  await vi.advanceTimersByTimeAsync(35000);
  expect(retryButton()).toBeUndefined();
  expect(container.textContent).not.toContain("Retrying");
  hls.emit("networkError");
  await vi.advanceTimersByTimeAsync(3000);
  video.currentTime = 13;
  video.dispatchEvent(new Event("timeupdate"));
  hls.emit("networkError");
  expect(retryButton()).toBeDefined();
  expect(hls.startLoad).toHaveBeenCalledTimes(2);
});
it("cancels stale retries and callbacks on source replacement and on close", async () => {
  const old = await mount();
  old.handlers.get("manifest")?.("manifest");
  old.emit("networkError");
  // A quality request creates another HLS instance even when the source has an outstanding retry.
  const quality = container.querySelector("#player-quality") as HTMLSelectElement | null;
  if (!quality) throw new Error("Quality control not found");
  quality.value = "4000000";
  quality.dispatchEvent(new Event("change", { bubbles: true }));
  await vi.advanceTimersByTimeAsync(0);
  expect(sdk.instances).toHaveLength(2);
  old.emit("mediaError");
  await vi.advanceTimersByTimeAsync(35000);
  expect(old.startLoad).not.toHaveBeenCalled();
  expect(retryButton()).toBeUndefined();
  const current = sdk.instances[1];
  current.emit("networkError");
  dispose?.();
  dispose = undefined;
  await vi.advanceTimersByTimeAsync(35000);
  expect(current.startLoad).not.toHaveBeenCalled();
});
it("handles a synchronous startup error before attachment and preserves its initial position", async () => {
  sdk.failOnLoad = true;
  const hls = await mount();
  expect(hls.attachMedia).not.toHaveBeenCalled();
  expect(retryButton()).toBeDefined();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.objectContaining({ body: { positionMs: 5000 } }),
  );
  retryButton()?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(sdk.instances).toHaveLength(2);
  expect(container.textContent).not.toContain("Stream prepared");
  expect(stopCalls()).toHaveLength(2); // Each failed server session is released once.
});
it("retries a failed initial negotiation at the requested position without closing nonexistent media", async () => {
  vi.mocked(client.POST).mockResolvedValueOnce({
    error: { error: { message: "Temporary server failure" } },
    response: new Response(null, { status: 503 }),
  } as never);
  await mount();
  retryButton()?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenLastCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        startMs: 5000,
        audioOffsetMs: 250,
        audioStreamIndex: 3,
        subtitleStreamIndex: 5,
      }),
    }),
  );
  expect(stopCalls()).toHaveLength(0);
  expect(sdk.instances).toHaveLength(1);
});

it("respects a pause made while a master manifest is being retried", async () => {
  const hls = await mount();
  hls.handlers.get("manifest")?.("manifest");
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
  hls.emit("networkError", 503, "manifestLoadTimeOut");
  container.querySelector("video")?.dispatchEvent(new Event("pause"));
  await vi.advanceTimersByTimeAsync(1000);
  hls.handlers.get("manifest")?.("manifest");
  hls.handlers.get("buffered")?.("buffered");
  await vi.advanceTimersByTimeAsync(35000);
  expect(retryButton()).toBeUndefined();
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
});
it("bounds a stalled stop before negotiating a replacement", async () => {
  const hls = await mount();
  videoAt(87);
  let stopSignal: AbortSignal | undefined;
  vi.mocked(client.POST).mockImplementation((path, options) => {
    if (path === "/api/v1/playback/{session}/stop") {
      stopSignal = (options as unknown as { signal: AbortSignal }).signal;
      return new Promise((_resolve, reject) => {
        stopSignal?.addEventListener("abort", () => reject(new Error("timeout")), { once: true });
      }) as never;
    }
    return Promise.resolve({
      data: { ...playback, sessionId: "replacement", startMs: 87000 },
      response: new Response(),
    }) as never;
  });
  hls.emit("mediaError");
  retryButton()?.click();
  await vi.advanceTimersByTimeAsync(9999);
  expect(sdk.instances).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(1);
  expect(stopSignal?.aborted).toBe(true);
  expect(sdk.instances).toHaveLength(2);
});
it("offers the same safe restart for a native/direct media error", async () => {
  vi.mocked(client.POST).mockResolvedValueOnce({
    data: { ...playback, method: "directPlay", url: "/movie.mp4" },
    response: new Response(),
  } as never);
  await mount();
  const video = videoAt(132);
  video.dispatchEvent(new Event("error"));
  await vi.advanceTimersByTimeAsync(0);
  expect(retryButton()).toBeDefined();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.objectContaining({ body: { positionMs: 132000 } }),
  );
  retryButton()?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenLastCalledWith(
    "/api/v1/playback",
    expect.objectContaining({ body: expect.objectContaining({ startMs: 132000 }) }),
  );
});
