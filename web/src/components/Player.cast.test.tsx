import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { castManager } from "../utils/cast";
import { Player } from "./Player";

const cast = vi.hoisted(() => ({
  subscribe: vi.fn(),
  setCommandHandler: vi.fn(() => vi.fn()),
  getLastError: vi.fn(() => null),
  selectSubtitle: vi.fn(async () => true),
  requestCastSession: vi.fn(),
  disposeSession: vi.fn(),
  stopCasting: vi.fn(),
  getState: vi.fn(),
  pause: vi.fn(),
  play: vi.fn(),
  seek: vi.fn(),
  setVolume: vi.fn(),
  toggleMute: vi.fn(),
}));
vi.mock("../utils/cast", () => ({ castManager: cast }));
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
vi.mock("../api/deviceProfile", () => ({ getPreferredProfile: () => "chrome" }));
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
let dispose: (() => void) | undefined;
let emit: (value: unknown) => void;
let state: {
  playbackEnded?: boolean;
  supportsAutoplayNext?: boolean;
  mediaLoadId?: number;
  itemId?: number;
  isAvailable: boolean;
  isCasting: boolean;
  deviceName: string | null;
  playerState: string;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
};
beforeEach(() => {
  vi.clearAllMocks();
  preferences.offset = 0;
  state = {
    isAvailable: true,
    isCasting: false,
    deviceName: null,
    playerState: "idle",
    currentTime: 0,
    duration: 600,
    volume: 1,
    isMuted: false,
  };
  cast.subscribe.mockImplementation((callback) => {
    emit = callback;
    callback(state);
    return vi.fn();
  });
  cast.getState.mockImplementation(() => state);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(client, "POST").mockImplementation(async (path) =>
    path === "/api/v1/playback"
      ? ({
          data: {
            sessionId: "test",
            method: "directPlay",
            url: "/movie.mp4",
            subtitles: [],
            markers: [],
            startMs: 0,
            durationMs: 600000,
          },
          response: new Response(),
        } as never)
      : ({ response: new Response(null, { status: 204 }) } as never),
  );
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});
it("restores local playback when the Cast chooser is canceled", async () => {
  cast.requestCastSession.mockResolvedValue(false);
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  document.querySelector("video")?.dispatchEvent(new Event("play"));
  document.querySelector<HTMLButtonElement>('[title="Cast to TV (Chromecast)"]')?.click();
  await tick();
  expect(cast.requestCastSession).toHaveBeenCalled();
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(2);
});
it("uses receiver position for progress/stop and routes keyboard controls remotely", async () => {
  let reportTick: () => void = () => {
    throw new Error("Progress timer not registered");
  };
  vi.spyOn(window, "setInterval").mockImplementation((callback) => {
    reportTick = callback as () => void;
    return 999;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  state = { ...state, isCasting: true, deviceName: "TV", playerState: "playing", currentTime: 123 };
  emit(state);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight" }));
  expect(castManager.seek).toHaveBeenCalledWith(133);
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "m" }));
  expect(castManager.toggleMute).toHaveBeenCalled();
  // Local element events must not overwrite receiver time or pause state.
  document.querySelector("video")?.dispatchEvent(new Event("timeupdate"));
  document.querySelector("video")?.dispatchEvent(new Event("pause"));
  state = { ...state, currentTime: 140 };
  emit(state);
  reportTick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/progress",
    expect.objectContaining({ body: { positionMs: 140000, state: "playing" } }),
  );
  dispose();
  dispose = undefined;
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.objectContaining({ body: { positionMs: 140000 } }),
  );
  expect(cast.disposeSession).toHaveBeenCalled();
});

it("restores local volume and mute indicators after receiver disconnect", async () => {
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  const video = document.querySelector("video")!;
  video.volume = 0.4;
  video.muted = false;
  state = { ...state, isCasting: true, playerState: "playing", currentTime: 80, volume: 0 };
  emit(state);
  state = { ...state, isCasting: false, playerState: "idle", currentTime: 0, volume: 1 };
  emit(state);
  expect((document.querySelector('input[max="1"]') as HTMLInputElement).value).toBe("0.4");
  expect(video.currentTime).toBe(80);
  video.muted = true;
  state = { ...state, isCasting: true, playerState: "playing", currentTime: 90, isMuted: false };
  emit(state);
  state = { ...state, isCasting: false, currentTime: 0 };
  emit(state);
  const range = document.querySelector('input[max="1"]') as HTMLInputElement;
  expect(range.value).toBe("0");
  expect(range.parentElement?.querySelector("button")?.textContent).toBe("🔇");
});

it("continues a finished receiver item once, after stopping its server session", async () => {
  vi.spyOn(client, "GET").mockImplementation(async (path, options) => {
    const id = (options as { params: { path: { id: number } } }).params.path.id;
    const data = String(path).endsWith("children")
      ? id === 10
        ? [{ id: 8, kind: "episode", idx: 2, title: "Next episode" }]
        : []
      : id === 10
        ? { id: 10, kind: "season", idx: 1, parentId: 100 }
        : { id, kind: "episode", idx: id === 7 ? 1 : 2, parentId: 10, title: "Next episode" };
    return { data, response: new Response() } as never;
  });
  const post = vi.spyOn(client, "POST").mockImplementation(async (path, options) => {
    const body = (options as { body?: { itemId?: number } })?.body;
    return path === "/api/v1/playback"
      ? ({
          data: {
            sessionId: body?.itemId === 8 ? "next-session" : "test",
            partId: 8,
            contentType: "video/mp4",
            url: "http://192.168.1.20/movie.mp4",
            subtitles: [],
            markers: [],
            startMs: 0,
            durationMs: 600000,
          },
          response: new Response(),
        } as never)
      : ({ response: new Response(null, { status: 204 }) } as never);
  });
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    await prepare();
    return true;
  });
  const next = vi.fn();
  dispose = render(
    () =>
      createComponent(Player, {
        itemId: 7,
        onClose: vi.fn(),
        onPlayNext: next,
        autoplayNext: true,
      }),
    document.body,
  );
  await tick();
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: true,
    itemId: 7,
    mediaLoadId: 1,
    playbackEnded: false,
    playerState: "playing",
    currentTime: 599,
  };
  emit(state);
  post.mockClear();
  state = { ...state, playbackEnded: true, playerState: "idle", currentTime: 600 };
  emit(state);
  emit(state);
  await tick();
  await tick();
  expect(post).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({ itemId: 8, profile: "chromecast", startMs: 0 }),
    }),
  );
  expect(cast.requestCastSession).toHaveBeenCalledTimes(1);
  expect(next).not.toHaveBeenCalled();
  expect(post).toHaveBeenCalledWith("/api/v1/playback/{session}/stop", {
    params: { path: { session: "test" } },
    body: { positionMs: 600000 },
    signal: expect.any(AbortSignal),
  });
});

it("does not display stale local video metrics as receiver statistics", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      sessionId: "test",
      profileName: "chromecast",
      network: "lan",
      isHls: false,
      transcode: null,
    },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  state = { ...state, isCasting: true, playerState: "playing" };
  emit(state);
  document.querySelector<HTMLButtonElement>('[title="Stats for Nerds (s)"]')?.click();
  await tick();
  expect(document.body.textContent).toContain(
    "Receiver video dimensions, decoded frames and buffer are not reported",
  );
  expect(document.body.textContent).not.toContain("Local resolution:");
  expect(document.body.textContent).not.toContain("Local dropped frames:");
});

it("carries signed audio sync into Google Cast and renegotiates it at receiver time", async () => {
  preferences.offset = -250;
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { title: "Movie" },
    response: new Response(),
  } as never);
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    await prepare();
    return true;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  document.querySelector<HTMLButtonElement>('[title="Cast to TV (Chromecast)"]')?.click();
  await tick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        profile: "chromecast",
        audioOffsetMs: -250,
      }),
    }),
  );
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: true,
    playerState: "paused",
    currentTime: 123,
  };
  emit(state);
  const sync = document.querySelector("#player-audio-delay") as HTMLSelectElement;
  expect(sync.disabled).toBe(false);
  sync.value = "500";
  sync.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        profile: "chromecast",
        audioOffsetMs: 500,
        startMs: 123000,
      }),
    }),
  );
  expect(cast.requestCastSession).toHaveBeenLastCalledWith(
    expect.objectContaining({ autoplay: false }),
    expect.anything(),
    expect.anything(),
  );
  expect(preferences.saveOffset).toHaveBeenLastCalledWith(500);
});

it("preserves current audio on Cast changes and ignores an unavailable optional title", async () => {
  vi.spyOn(client, "GET").mockRejectedValue(new Error("Metadata offline"));
  const replies = (audio: number) => ({
    sessionId: `audio-${audio}`,
    partId: 22,
    url: "http://192.168.1.20/movie.mp4",
    contentType: "video/mp4",
    subtitles: [],
    markers: [],
    startMs: 0,
    durationMs: 600000,
    audioStreamIndex: audio,
    audioTracks: [
      { index: 1, title: "English", codec: "aac" },
      { index: 3, title: "Swedish", codec: "aac" },
    ],
  });
  vi.mocked(client.POST).mockImplementation(
    async (path, options) =>
      (path === "/api/v1/playback"
        ? {
            data: replies(
              (options as { body: { audioStreamIndex?: number } }).body.audioStreamIndex ?? 1,
            ),
            response: new Response(),
          }
        : { response: new Response(null, { status: 204 }) }) as never,
  );
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    await prepare();
    return true;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  document.querySelector<HTMLButtonElement>('[title="Cast to TV (Chromecast)"]')?.click();
  await tick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/stop",
    expect.objectContaining({ params: { path: { session: "audio-1" } } }),
  );
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: true,
    currentTime: 123,
    playerState: "paused",
  };
  emit(state);
  const audio = document.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({
        profile: "chromecast",
        audioStreamIndex: 3,
        startMs: 123000,
      }),
    }),
  );
  expect(audio.value).toBe("3");
  const sync = document.querySelector("#player-audio-delay") as HTMLSelectElement;
  sync.value = "250";
  sync.dispatchEvent(new Event("change", { bubbles: true }));
  await tick();
  await tick();
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback",
    expect.objectContaining({
      body: expect.objectContaining({ audioStreamIndex: 3, audioOffsetMs: 250 }),
    }),
  );
  expect(cast.requestCastSession).toHaveBeenLastCalledWith(
    expect.objectContaining({ autoplay: false }),
    expect.anything(),
    expect.anything(),
  );
});
it("does not send a burned subtitle as a nonexistent Cast caption track", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { title: "Movie" },
    response: new Response(),
  } as never);
  vi.mocked(client.POST).mockImplementation(
    async (path) =>
      (path === "/api/v1/playback"
        ? {
            data: {
              sessionId: "burned",
              partId: 22,
              url: "http://192.168.1.20/movie.m3u8",
              contentType: "application/vnd.apple.mpegurl",
              subtitles: [],
              markers: [],
              startMs: 0,
              durationMs: 600000,
              burnedSubtitleStreamIndex: 5,
              subtitleOptions: [{ index: 5, title: "PGS", requiresTranscode: true }],
            },
            response: new Response(),
          }
        : { response: new Response(null, { status: 204 }) }) as never,
  );
  let prepared: { subtitleIndex?: number | null } | undefined;
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    prepared = await prepare();
    return true;
  });
  dispose = render(
    () => createComponent(Player, { itemId: 7, subtitleStreamIndex: 5, onClose: vi.fn() }),
    document.body,
  );
  await tick();
  document.querySelector<HTMLButtonElement>('[title="Cast to TV (Chromecast)"]')?.click();
  await tick();
  await tick();
  expect(prepared?.subtitleIndex).toBeNull();
});

it("keeps the current session when native Remote Playback cannot load the next episode", async () => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: String(path).endsWith("children")
          ? [
              { id: 7, kind: "episode", idx: 1, title: "Current" },
              { id: 8, kind: "episode", idx: 2, title: "Next" },
            ]
          : { id: 7, kind: "episode", idx: 1, parentId: 10 },
        response: new Response(),
      }) as never,
  );
  dispose = render(
    () => createComponent(Player, { itemId: 7, onClose: vi.fn(), onPlayNext: vi.fn() }),
    document.body,
  );
  await tick();
  await tick();
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: false,
    currentTime: 599,
    playerState: "playing",
  };
  emit(state);
  const next = Array.from(document.querySelectorAll("button")).find((button) =>
    button.textContent?.includes("Play next episode"),
  );
  expect(next).toBeDefined();
  expect(next?.disabled).toBe(true);
  vi.mocked(client.POST).mockClear();
  next?.click();
  await tick();
  expect(client.POST).not.toHaveBeenCalled();
});

it("keeps a receiver pause made while its audio change is negotiating", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { title: "Movie" },
    response: new Response(),
  } as never);
  vi.mocked(client.POST).mockResolvedValue({
    data: {
      sessionId: "old",
      partId: 22,
      url: "http://192.168.1.20/movie.mp4",
      subtitles: [],
      markers: [],
      startMs: 0,
      durationMs: 600000,
      audioStreamIndex: 1,
      audioTracks: [
        { index: 1, title: "English", codec: "aac" },
        { index: 3, title: "Swedish", codec: "aac" },
      ],
    },
    response: new Response(),
  } as never);
  let resolve!: (value: unknown) => void;
  let prepared: { autoplay?: boolean } | undefined;
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    prepared = await prepare();
    return true;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: true,
    playerState: "playing",
    currentTime: 12,
  };
  emit(state);
  vi.mocked(client.POST).mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }) as never,
  );
  const audio = document.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  state = { ...state, playerState: "paused" };
  emit(state);
  resolve({
    data: {
      sessionId: "new",
      partId: 22,
      url: "http://192.168.1.20/new.mp4",
      subtitles: [],
      markers: [],
      startMs: 12000,
      durationMs: 600000,
      audioStreamIndex: 3,
    },
    response: new Response(),
  });
  await tick();
  await tick();
  expect(prepared?.autoplay).toBe(false);
});

it("commits a prepared Cast replacement at the latest receiver time", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: { title: "Movie" },
    response: new Response(),
  } as never);
  const media = {
    sessionId: "old",
    partId: 22,
    url: "http://192.168.1.20/movie.mp4",
    subtitles: [],
    markers: [],
    startMs: 0,
    durationMs: 600000,
    audioStreamIndex: 1,
    audioTracks: [
      { index: 1, title: "English", codec: "aac" },
      { index: 3, title: "Swedish", codec: "aac" },
    ],
  };
  vi.mocked(client.POST).mockResolvedValue({ data: media, response: new Response() } as never);
  let resolve!: (value: unknown) => void;
  let prepared: { currentTime?: number } | undefined;
  cast.requestCastSession.mockImplementation(async (_media, _video, prepare) => {
    prepared = await prepare();
    return true;
  });
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  state = {
    ...state,
    isCasting: true,
    supportsAutoplayNext: true,
    currentTime: 12,
    playerState: "playing",
  };
  emit(state);
  vi.mocked(client.POST).mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done;
      }) as never,
  );
  const audio = document.querySelector("#player-audio-track") as HTMLSelectElement;
  audio.value = "3";
  audio.dispatchEvent(new Event("change", { bubbles: true }));
  state = { ...state, currentTime: 18 };
  emit(state);
  resolve({
    data: { ...media, sessionId: "new", audioStreamIndex: 3, startMs: 12000 },
    response: new Response(),
  });
  await tick();
  await tick();
  expect(prepared?.currentTime).toBe(18);
  expect(client.POST).toHaveBeenCalledWith("/api/v1/playback/{session}/stop", {
    params: { path: { session: "old" } },
    body: { positionMs: 18000 },
    signal: expect.any(AbortSignal),
  });
});

it("renews a paused receiver session without using the hidden local video's time", async () => {
  dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), document.body);
  await tick();
  state = { ...state, isCasting: true, playerState: "paused", currentTime: 91 };
  emit(state);
  vi.useFakeTimers();
  // Visibility restoration uses the same heartbeat as the periodic timer.
  vi.mocked(client.POST).mockClear();
  document.dispatchEvent(new Event("visibilitychange"));
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/playback/{session}/progress",
    expect.objectContaining({ body: { positionMs: 91000, state: "paused" } }),
  );
});
