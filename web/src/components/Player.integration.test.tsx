import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import client from "../api/client";
import { Player } from "./Player";

const preferences = vi.hoisted(() => ({ offset: 0, saveOffset: vi.fn() }));
vi.mock("../context/ThemeContext", () => ({
  useTheme: () => ({
    subtitleStyle: () => ({ size: "md", background: "dim", color: "white" }),
    nightMode: () => false,
    setNightMode: vi.fn(),
    audioOffsetMs: () => preferences.offset,
    setAudioOffsetMs: preferences.saveOffset,
  }),
}));
vi.mock("../api/deviceProfile", () => ({ getPreferredProfile: () => "firefox" }));
const playback = {
  sessionId: "session",
  method: "directPlay",
  url: "/movie.mp4",
  startMs: 0,
  subtitles: [],
  markers: [],
  durationMs: 120000,
};
let dispose: (() => void) | undefined;
let container: HTMLDivElement;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
beforeEach(() => {
  preferences.offset = 0;
  preferences.saveOffset.mockClear();
  container = document.createElement("div");
  document.body.append(container);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(client, "POST").mockResolvedValue({ data: playback, response: new Response() } as never);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  container.remove();
  vi.restoreAllMocks();
});
describe("rendered player lifecycle", () => {
  it("sends explicit start-over and selected tracks to playback negotiation", async () => {
    dispose = render(
      () =>
        createComponent(Player, {
          itemId: 7,
          initialStartMs: 0,
          audioStreamIndex: 2,
          subtitleStreamIndex: null,
          onClose: vi.fn(),
        }),
      container,
    );
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({
          itemId: 7,
          startMs: 0,
          audioStreamIndex: 2,
          profile: "firefox",
        }),
      }),
    );
  });
  it("preserves a requested start position under ten seconds", async () => {
    dispose = render(
      () => createComponent(Player, { itemId: 7, initialStartMs: 5000, onClose: vi.fn() }),
      container,
    );
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({ body: expect.objectContaining({ startMs: 5000 }) }),
    );
  });
  it("releases exactly one session when unmounted", async () => {
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await tick();
    dispose();
    dispose = undefined;
    await tick();
    expect(
      vi
        .mocked(client.POST)
        .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop"),
    ).toHaveLength(1);
  });
  it("releases a session returned after the player was closed", async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(client.POST).mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }) as never,
    );
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    dispose();
    dispose = undefined;
    resolve({ data: playback, response: new Response() });
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback/{session}/stop",
      expect.objectContaining({ params: { path: { session: "session" } } }),
    );
    expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
  });
});

it("renegotiates quality at the current position and releases the previous session", async () => {
  const post = vi.mocked(client.POST);
  post.mockImplementation(async (path, options) =>
    path === "/api/v1/playback"
      ? ({
          data: {
            ...playback,
            sessionId: (options as { body?: { maxBitrate?: number } } | undefined)?.body?.maxBitrate
              ? "limited"
              : "session",
            partId: 22,
          },
          response: new Response(),
        } as never)
      : ({ response: new Response(null, { status: 204 }) } as never),
  );
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  const video = container.querySelector("video") as HTMLVideoElement;
  video.currentTime = 37;
  const select = container.querySelector("#player-quality") as HTMLSelectElement;
  select.value = "4000000";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(post).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({ itemId: 7, partId: 22, startMs: 37000, maxBitrate: 4000000 }),
    }),
  );
  expect(post).toHaveBeenCalledWith("/api/v1/playback/{session}/stop", {
    params: { path: { session: "session" } },
    body: { positionMs: 37000 },
    signal: expect.any(AbortSignal),
  });
  expect(select.value).toBe("4000000");
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
});
it("retains the current session and selected quality if renegotiation fails", async () => {
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  vi.mocked(client.POST).mockResolvedValue({
    error: {},
    response: new Response(null, { status: 503 }),
  } as never);
  vi.mocked(client.POST).mockClear();
  const select = container.querySelector("#player-quality") as HTMLSelectElement;
  select.value = "2000000";
  select.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Playback adjustment failed",
  );
  expect(select.value).toBe("0");
  expect(client.POST).not.toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.anything(),
  );
});
it.each([-250, 250])(
  "negotiates signed audio sync %i at the current position and preserves it on quality change",
  async (offset) => {
    const post = vi.mocked(client.POST);
    post.mockImplementation(async (path, options) => {
      const body = (options as { body?: { audioOffsetMs?: number; maxBitrate?: number } })?.body;
      return (
        path === "/api/v1/playback"
          ? {
              data: {
                ...playback,
                sessionId: `sync-${body?.audioOffsetMs}-${body?.maxBitrate ?? 0}`,
                audioOffsetMs: body?.audioOffsetMs ?? 0,
                partId: 22,
              },
              response: new Response(),
            }
          : { response: new Response(null, { status: 204 }) }
      ) as never;
    });
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await tick();
    const video = container.querySelector("video") as HTMLVideoElement;
    video.currentTime = 37;
    const sync = container.querySelector("#player-audio-delay") as HTMLSelectElement;
    sync.value = String(offset);
    sync.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();
    expect(post).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({ startMs: 37000, audioOffsetMs: offset, partId: 22 }),
      }),
    );
    expect(preferences.saveOffset).toHaveBeenLastCalledWith(offset);
    expect(sync.value).toBe(String(offset));
    const quality = container.querySelector("#player-quality") as HTMLSelectElement;
    quality.value = "4000000";
    quality.dispatchEvent(new Event("change", { bubbles: true }));
    await tick();
    expect(post).toHaveBeenLastCalledWith("/api/v1/playback/{session}/stop", expect.anything());
    expect(post).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({ maxBitrate: 4000000, audioOffsetMs: offset }),
      }),
    );
  },
);

it("keeps the old audio sync and preference when the server rejects the adjustment", async () => {
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  vi.mocked(client.POST).mockResolvedValueOnce({
    error: { detail: "Full" },
    response: new Response(null, { status: 503 }),
  } as never);
  const sync = container.querySelector("#player-audio-delay") as HTMLSelectElement;
  sync.value = "-500";
  sync.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(sync.value).toBe("0");
  expect(preferences.saveOffset).not.toHaveBeenCalled();
  expect(container.textContent).toContain("Previous settings are still selected");
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop"),
  ).toHaveLength(0);
});

it("sends saved negative synchronization on initial playback", async () => {
  preferences.offset = -500;
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({ audioOffsetMs: -500 }),
    }),
  );
});

it("waits for an in-flight stop before starting the next episode without duplicate cleanup", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: String(path).endsWith("children")
          ? [{ id: 8, kind: "episode", idx: 2, title: "Episode two" }]
          : { id: 7, kind: "episode", idx: 1, parentId: 10 },
        response: new Response(),
      }) as never,
  );
  const next = vi.fn();
  dispose = render(
    () => createComponent(Player, { itemId: 7, onClose: vi.fn(), onPlayNext: next }),
    container,
  );
  await tick();
  let release: (() => void) | undefined;
  vi.mocked(client.POST).mockImplementation(
    async () =>
      await new Promise((resolve) => {
        release = () => resolve({ response: new Response(null, { status: 204 }) } as never);
      }),
  );
  vi.mocked(client.POST).mockClear();
  container.querySelector("video")?.dispatchEvent(new Event("ended"));
  [...container.querySelectorAll("button")]
    .find((button) => button.textContent === "Play next episode")
    ?.click();
  await tick();
  expect(next).not.toHaveBeenCalled();
  expect(client.POST).toHaveBeenCalledTimes(1);
  release?.();
  await tick();
  expect(next).toHaveBeenCalledWith({
    itemId: 8,
    title: "Episode two",
    initialStartMs: 0,
    autoplayNext: false,
  });
});
it("plays next automatically only when explicitly enabled for this playback chain", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: String(path).endsWith("children")
          ? [{ id: 8, kind: "episode", idx: 2, title: "Episode two" }]
          : { id: 7, kind: "episode", idx: 1, parentId: 10 },
        response: new Response(),
      }) as never,
  );
  const next = vi.fn();
  dispose = render(
    () =>
      createComponent(Player, {
        itemId: 7,
        onClose: vi.fn(),
        onPlayNext: next,
        autoplayNext: true,
      }),
    container,
  );
  await tick();
  container.querySelector("video")?.dispatchEvent(new Event("ended"));
  await tick();
  expect(next).toHaveBeenCalledWith(
    expect.objectContaining({ itemId: 8, initialStartMs: 0, autoplayNext: true }),
  );
});
it("does not turn select navigation into player seek shortcuts", async () => {
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  const video = container.querySelector("video") as HTMLVideoElement;
  video.currentTime = 40;
  container
    .querySelector("select")
    ?.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  expect(video.currentTime).toBe(40);
});

it("loads server diagnostics only while statistics are open and releases the request on close", async () => {
  let signal: AbortSignal | undefined;
  const get = vi.spyOn(client, "GET").mockImplementation(async (_path, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return {
      data: {
        sessionId: "session",
        network: "wan",
        profileName: "firefox",
        isHls: true,
        transcode: {
          backend: "software",
          processRunning: true,
          throttled: true,
          restarts: 2,
          lastExitFailed: false,
        },
      },
      response: new Response(),
    } as never;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  expect(get).not.toHaveBeenCalled();
  const button = container.querySelector<HTMLButtonElement>('[title="Stats for Nerds (s)"]');
  button?.click();
  await tick();
  expect(get).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/diagnostics",
    expect.objectContaining({ params: { path: { session: "session" } } }),
  );
  expect(container.textContent).toContain("Session request network: wan");
  expect(container.textContent).toContain("Active backend: software");
  expect(container.textContent).toContain("Paused ahead of playback");
  button?.click();
  expect(signal?.aborted).toBe(true);
});
it("does not present unavailable server diagnostics as direct play", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 503 }),
  } as never);
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  container.querySelector<HTMLButtonElement>('[title="Stats for Nerds (s)"]')?.click();
  await tick();
  expect(container.querySelector('[role="alert"]')?.textContent).toContain(
    "Server diagnostics unavailable",
  );
  expect(container.textContent).not.toContain("Server transcoding: not used");
});

it("measures the current local buffer range and refreshes resolution after metadata changes", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { profileName: "firefox", network: "lan", isHls: false, transcode: null },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  const video = container.querySelector("video") as HTMLVideoElement;
  video.currentTime = 10;
  Object.defineProperty(video, "videoWidth", { configurable: true, value: 1280 });
  Object.defineProperty(video, "videoHeight", { configurable: true, value: 720 });
  Object.defineProperty(video, "buffered", {
    configurable: true,
    value: { length: 1, start: () => 100, end: () => 110 },
  });
  container.querySelector<HTMLButtonElement>('[title="Stats for Nerds (s)"]')?.click();
  await tick();
  expect(container.textContent).toContain("Local buffer: 0s");
  expect(container.textContent).toContain("Local resolution: 1280x720");
  Object.defineProperty(video, "videoWidth", { configurable: true, value: 640 });
  Object.defineProperty(video, "videoHeight", { configurable: true, value: 360 });
  video.dispatchEvent(new Event("loadedmetadata"));
  expect(container.textContent).toContain("Local resolution: 640x360");
});

it.each([true, false])(
  "can retry without saved sync after unavailable HLS, preserving resume/start-over choice (%s)",
  async (resume) => {
    preferences.offset = -500;
    vi.mocked(client.POST).mockResolvedValueOnce({
      error: { detail: "No free HLS capacity" },
      response: new Response(null, { status: 503 }),
    } as never);
    dispose = render(
      () => createComponent(Player, { itemId: 7, initialStartMs: 37000, onClose: vi.fn() }),
      container,
    );
    await tick();
    const choice = Array.from(container.querySelectorAll("button")).find((button) =>
      resume
        ? button.textContent?.startsWith("Resume (")
        : button.textContent?.includes("Start Over"),
    );
    expect(choice).toBeDefined();
    choice?.click();
    await tick();
    expect(container.textContent).toContain("No free HLS capacity");
    expect(preferences.saveOffset).not.toHaveBeenCalled();
    const retry = Array.from(container.querySelectorAll("button")).find((button) =>
      button.textContent?.includes("Retry without audio sync"),
    );
    expect(retry).toBeDefined();
    retry?.click();
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({ audioOffsetMs: 0, startMs: resume ? 37000 : 0 }),
      }),
    );
    expect(preferences.saveOffset).toHaveBeenCalledWith(0);
    expect(container.textContent).not.toContain("Retry without audio sync");
  },
);

it("loads seek previews only on timeline focus and cancels them on blur", async () => {
  vi.mocked(client.POST).mockResolvedValue({
    data: { ...playback, partId: 22 },
    response: new Response(),
  } as never);
  let signal: AbortSignal | undefined;
  const get = vi.spyOn(client, "GET").mockImplementation(async (_path, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return { data: { status: "queued" }, response: new Response(null, { status: 202 }) } as never;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  expect(get).not.toHaveBeenCalled();
  const timeline = container.querySelector('input[aria-label="Seek video"]') as HTMLInputElement;
  timeline.focus();
  await tick();
  expect(get).toHaveBeenCalledWith(
    "/api/v1/stream/trickplay/{partId}/index.json",
    expect.objectContaining({ params: { path: { partId: 22 } } }),
  );
  expect(container.textContent).toContain("Previews queued");
  timeline.blur();
  await tick();
  expect(signal?.aborted).toBe(true);
  expect(container.querySelector(".trickplay-preview")).toBeNull();
});

function playbackWithTrackChoices(
  body: { audioStreamIndex?: number; subtitleStreamIndex?: number } = {},
  sessionId = "tracks",
) {
  const subtitles = [
    {
      index: 2,
      title: "English captions",
      codec: "subrip",
      language: "en",
      isDefault: true,
      isForced: false,
      format: "vtt",
      url: `/captions-${sessionId}-2.vtt`,
    },
    {
      index: 4,
      title: "Danish captions",
      codec: "subrip",
      language: "da",
      isDefault: false,
      isForced: false,
      format: "vtt",
      url: `/captions-${sessionId}-4.vtt`,
    },
  ];
  return {
    ...playback,
    sessionId,
    partId: 22,
    audioStreamIndex: body.audioStreamIndex ?? 1,
    audioTracks: [
      { index: 1, title: "English audio", codec: "aac", channels: 2 },
      { index: 3, title: "Swedish audio", codec: "aac", channels: 2 },
    ],
    burnedSubtitleStreamIndex: body.subtitleStreamIndex === 5 ? 5 : null,
    subtitles,
    subtitleOptions: [
      ...subtitles.map((sub) => ({ ...sub, requiresTranscode: false })),
      { index: 5, title: "PGS captions", requiresTranscode: true },
    ],
  };
}
function mockTrackPlayback() {
  let version = 0;
  vi.mocked(client.POST).mockImplementation(async (path, options) => {
    const body = (options as { body?: { audioStreamIndex?: number; subtitleStreamIndex?: number } })
      ?.body;
    return (
      path === "/api/v1/playback"
        ? { data: playbackWithTrackChoices(body, `tracks-${version++}`), response: new Response() }
        : { response: new Response(null, { status: 204 }) }
    ) as never;
  });
}
async function chooseSubtitle(title: string) {
  container.querySelector<HTMLButtonElement>('[aria-label="Subtitle tracks"]')?.click();
  Array.from(container.querySelectorAll("button"))
    .find((button) => button.textContent?.trim() === title)
    ?.click();
  await tick();
}
it("switches audio at the current position and keeps current captions through quality changes", async () => {
  mockTrackPlayback();
  dispose = render(
    () =>
      createComponent(Player, {
        itemId: 7,
        audioStreamIndex: 1,
        subtitleStreamIndex: null,
        onClose: vi.fn(),
      }),
    container,
  );
  await tick();
  await chooseSubtitle("Danish captions");
  const video = container.querySelector("video") as HTMLVideoElement;
  video.currentTime = 37;
  const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        audioStreamIndex: 3,
        subtitleStreamIndex: 4,
        startMs: 37000,
        partId: 22,
      }),
    }),
  );
  expect(audio.value).toBe("3");
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
  const quality = container.querySelector("#player-quality") as HTMLSelectElement;
  quality.value = "4000000";
  quality.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        audioStreamIndex: 3,
        subtitleStreamIndex: 4,
        maxBitrate: 4000000,
      }),
    }),
  );
  expect(container.querySelector("track")?.getAttribute("src")).toContain("-4.vtt");
});
it("retains the selected audio and session when the new track cannot be admitted", async () => {
  mockTrackPlayback();
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  vi.mocked(client.POST).mockResolvedValueOnce({
    error: { error: { message: "No free transcode capacity" } },
    response: new Response(null, { status: 503 }),
  } as never);
  const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(audio.value).toBe("1");
  expect(container.textContent).toContain("No free transcode capacity");
  expect(container.textContent).toContain("Previous settings are still selected");
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop"),
  ).toHaveLength(0);
});
it("renegotiates image subtitles and keeps Off after removing a burned subtitle", async () => {
  mockTrackPlayback();
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  await chooseSubtitle("PGS captions (video conversion)");
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({ body: expect.objectContaining({ subtitleStreamIndex: 5 }) }),
  );
  expect(container.querySelector("track")).toBeNull();
  await chooseSubtitle("Off");
  const calls = vi
    .mocked(client.POST)
    .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback");
  expect(
    (calls[calls.length - 1][1] as { body: { subtitleStreamIndex?: number } }).body
      .subtitleStreamIndex,
  ).toBeUndefined();
  expect(container.querySelector("track")).toBeNull();
  const quality = container.querySelector("#player-quality") as HTMLSelectElement;
  quality.value = "4000000";
  quality.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(container.querySelector("track")).toBeNull();
});

it("preserves a pause made while an audio change is still negotiating", async () => {
  mockTrackPlayback();
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  const video = container.querySelector("video") as HTMLVideoElement;
  video.dispatchEvent(new Event("play"));
  let resolve!: (value: unknown) => void;
  vi.mocked(client.POST).mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }) as never,
  );
  const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  video.dispatchEvent(new Event("pause"));
  resolve({
    data: playbackWithTrackChoices({ audioStreamIndex: 3 }, "changed"),
    response: new Response(),
  });
  await tick();
  expect(audio.value).toBe("3");
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
});

it("keeps the old source during preparation and commits at the latest playback position", async () => {
  vi.mocked(client.POST).mockResolvedValue({
    data: { ...playbackWithTrackChoices({}, "old"), url: "/old.mp4" },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  const video = container.querySelector("video") as HTMLVideoElement;
  video.currentTime = 37;
  video.dispatchEvent(new Event("play"));
  let resolve!: (value: unknown) => void;
  vi.mocked(client.POST).mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }) as never,
  );
  const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  expect(video.src).toContain("/old.mp4");
  expect(container.textContent).toContain("Preparing playback");
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({ waitForReady: true, startMs: 37000 }),
    }),
  );
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop"),
  ).toHaveLength(0);
  video.currentTime = 41;
  resolve({
    data: {
      ...playbackWithTrackChoices({ audioStreamIndex: 3 }, "new"),
      startMs: 37000,
      url: "/new.mp4",
    },
    response: new Response(),
  });
  await tick();
  expect(video.src).toContain("/new.mp4");
  expect(video.currentTime).toBe(41);
  expect(client.POST).toHaveBeenCalledWith("/api/v1/playback/{session}/stop", {
    params: { path: { session: "old" } },
    body: { positionMs: 41000 },
    signal: expect.any(AbortSignal),
  });
});
it("aborts a pending replacement on close and releases a candidate returned despite cancellation", async () => {
  mockTrackPlayback();
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
  await tick();
  let resolve!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.POST).mockImplementationOnce((_path, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return new Promise((done) => {
      resolve = done;
    }) as never;
  });
  const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  dispose();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
  resolve({
    data: playbackWithTrackChoices({ audioStreamIndex: 3 }, "late"),
    response: new Response(),
  });
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.objectContaining({ params: { path: { session: "late" } } }),
  );
});

it("keeps paused and buffering sessions alive without confirming an unplayed source", async () => {
  vi.useFakeTimers();
  try {
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await vi.advanceTimersByTimeAsync(0);
    const video = container.querySelector("video") as HTMLVideoElement;
    vi.mocked(client.POST).mockClear();
    await vi.advanceTimersByTimeAsync(5000);
    expect(client.POST).toHaveBeenLastCalledWith(
      "/api/v1/playback/{session}/progress",
      expect.objectContaining({ body: { positionMs: 0, state: "buffering" } }),
    );
    video.currentTime = 82;
    video.dispatchEvent(new Event("play"));
    video.dispatchEvent(new Event("playing"));
    await vi.advanceTimersByTimeAsync(0);
    video.dispatchEvent(new Event("pause"));
    await vi.advanceTimersByTimeAsync(0);
    vi.mocked(client.POST).mockClear();
    await vi.advanceTimersByTimeAsync(65000);
    const calls = vi.mocked(client.POST).mock.calls as unknown as Array<
      [string, { body: { state: string; positionMs: number } }]
    >;
    const beats = calls.filter(([path]) => path === "/api/v1/playback/{session}/progress");
    expect(beats).toHaveLength(13);
    expect(
      beats.every(
        ([, options]) => options?.body?.state === "paused" && options.body.positionMs === 82000,
      ),
    ).toBe(true);
    vi.mocked(client.POST).mockClear();
    document.dispatchEvent(new Event("visibilitychange"));
    await vi.advanceTimersByTimeAsync(0);
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback/{session}/progress",
      expect.objectContaining({ body: { positionMs: 82000, state: "paused" } }),
    );
    dispose?.();
    dispose = undefined;
    await vi.advanceTimersByTimeAsync(0);
    vi.mocked(client.POST).mockClear();
    await vi.advanceTimersByTimeAsync(10000);
    document.dispatchEvent(new Event("visibilitychange"));
    expect(client.POST).not.toHaveBeenCalled();
  } finally {
    vi.useRealTimers();
  }
});

it("offers a paused restart at the same position after an expired session", async () => {
  vi.useFakeTimers();
  try {
    vi.mocked(client.POST).mockImplementation(
      (path) =>
        Promise.resolve({
          data: path === "/api/v1/playback" ? playback : undefined,
          response: new Response(),
        }) as never,
    );
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await vi.advanceTimersByTimeAsync(0);
    const video = container.querySelector("video") as HTMLVideoElement;
    video.currentTime = 123;
    video.dispatchEvent(new Event("playing"));
    await vi.advanceTimersByTimeAsync(0);
    video.dispatchEvent(new Event("pause"));
    await vi.advanceTimersByTimeAsync(0);
    vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
    vi.mocked(client.POST).mockImplementation(
      (path) =>
        Promise.resolve(
          path === "/api/v1/playback/{session}/progress"
            ? {
                error: { error: { message: "Session not found" } },
                response: new Response(null, { status: 404 }),
              }
            : {
                data: { ...playback, sessionId: "resumed", startMs: 123000 },
                response: new Response(),
              },
        ) as never,
    );
    await vi.advanceTimersByTimeAsync(5000);
    expect(container.textContent).toContain("Playback session expired");
    const calls = vi.mocked(client.POST).mock.calls.length;
    await vi.advanceTimersByTimeAsync(15000);
    expect(vi.mocked(client.POST).mock.calls).toHaveLength(calls);
    const retry = Array.from(container.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("Resume playback"),
    );
    retry?.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({ startMs: 123000, waitForReady: true }),
      }),
    );
    expect(video.currentTime).toBe(123);
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
    expect(container.textContent).not.toContain("Playback session expired");
  } finally {
    vi.useRealTimers();
  }
});

it("ignores an expired heartbeat from a stream that has already been replaced", async () => {
  let expireOld!: (value: unknown) => void;
  vi.useFakeTimers();
  try {
    mockTrackPlayback();
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await vi.advanceTimersByTimeAsync(0);
    const post = vi.mocked(client.POST);
    const original = post.getMockImplementation() as unknown as
      | ((path: unknown, options: unknown) => unknown)
      | undefined;
    post.mockImplementation((path, options) => {
      if (path === "/api/v1/playback/{session}/progress")
        return new Promise((resolve) => {
          expireOld = resolve;
        }) as never;
      return original?.(path, options) as never;
    });
    await vi.advanceTimersByTimeAsync(5000);
    const audio = container.querySelector("#player-audio-track") as HTMLSelectElement;
    audio.value = "3";
    audio.dispatchEvent(new Event("change", { bubbles: true }));
    await vi.advanceTimersByTimeAsync(0);
    expireOld({ error: {}, response: new Response(null, { status: 404 }) });
    await vi.advanceTimersByTimeAsync(0);
    expect(audio.value).toBe("3");
    expect(container.textContent).not.toContain("Playback session expired");
  } finally {
    vi.useRealTimers();
  }
});

it("times out a stuck heartbeat so it can renew again and aborts progress on close", async () => {
  vi.useFakeTimers();
  try {
    const signals: AbortSignal[] = [];
    vi.mocked(client.POST).mockImplementation((path, options) => {
      if (path === "/api/v1/playback/{session}/progress") {
        const signal = (options as unknown as { signal: AbortSignal }).signal;
        signals.push(signal);
        return new Promise((_resolve, reject) => {
          signal.addEventListener("abort", () => reject(new Error("aborted")), { once: true });
        }) as never;
      }
      return Promise.resolve({ data: playback, response: new Response() }) as never;
    });
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await vi.advanceTimersByTimeAsync(20000);
    expect(signals).toHaveLength(2);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
    dispose();
    dispose = undefined;
    expect(signals[1].aborted).toBe(true);
    await vi.advanceTimersByTimeAsync(15000);
    expect(signals).toHaveLength(2);
  } finally {
    vi.useRealTimers();
  }
});
