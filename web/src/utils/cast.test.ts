import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import client from "../api/client";

const api = vi.hoisted(() => ({ POST: vi.fn(), DELETE: vi.fn() }));
vi.mock("../api/client", () => ({ default: api }));

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
let manager: typeof import("./cast")["castManager"] | undefined;
let cleanup: (() => void) | undefined;
beforeEach(() => {
  vi.resetModules();
  vi.stubGlobal(
    "WebSocket",
    class {
      onclose = null;
      close() {}
    },
  );
  vi.clearAllMocks();
  vi.spyOn(client, "POST").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
});
afterEach(async () => {
  manager?.disposeSession();
  await Promise.resolve();
  cleanup?.();
  cleanup = undefined;
  manager = undefined;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function google() {
  const listeners = new Map<string, () => void>();
  const player = {
    isConnected: false,
    isPaused: false,
    currentTime: 0,
    duration: 0,
    volumeLevel: 1,
    isMuted: false,
    playerState: "PLAYING",
  };
  const emit = () => listeners.get("any-change")?.();
  const controller = {
    addEventListener: vi.fn((event: string, callback: () => void) => {
      if (event !== "any-change") throw new Error("Unknown SDK event");
      listeners.set(event, callback);
    }),
    playOrPause: vi.fn(() => {
      player.isPaused = !player.isPaused;
      player.playerState = player.isPaused ? "PAUSED" : "PLAYING";
      emit();
    }),
    seek: vi.fn(emit),
    setVolumeLevel: vi.fn(emit),
    stop: vi.fn(),
    muteOrUnmute: vi.fn(() => {
      player.isMuted = !player.isMuted;
      emit();
    }),
  };
  const mediaListeners = new Set<(alive: boolean) => void>();
  const mediaSession = {
    playerState: "PLAYING",
    idleReason: null as string | null,
    media: { contentId: "" },
    addUpdateListener: vi.fn((listener: (alive: boolean) => void) => mediaListeners.add(listener)),
    removeUpdateListener: vi.fn((listener: (alive: boolean) => void) =>
      mediaListeners.delete(listener),
    ),
    editTracksInfo: vi.fn(),
  };
  const emitMedia = () => {
    for (const listener of mediaListeners) listener(true);
  };
  const session = {
    getSessionId: () => "cast-test",
    getMediaSession: () => mediaSession,
    getCastDevice: () => ({ friendlyName: "Living room" }),
    loadMedia: vi.fn(async (request: unknown) => {
      const value = request as { mediaInfo: { contentId: string } };
      mediaSession.media.contentId = value.mediaInfo.contentId;
      mediaSession.playerState = "PLAYING";
      mediaSession.idleReason = null;
    }),
  };
  const context = {
    setOptions: vi.fn(),
    getCurrentSession: () => session,
    requestSession: vi.fn(async () => {
      player.isConnected = true;
    }),
    endCurrentSession: vi.fn(() => {
      player.isConnected = false;
      emit();
    }),
  };
  Object.defineProperty(window, "cast", {
    configurable: true,
    value: {
      framework: {
        CastContext: { getInstance: () => context },
        RemotePlayer: class {
          constructor() {
            // biome-ignore lint/correctness/noConstructorReturn: SDK double exposes a controllable shared state object.
            return player;
          }
        },
        RemotePlayerController: class {
          constructor() {
            // biome-ignore lint/correctness/noConstructorReturn: SDK double exposes a controllable shared controller.
            return controller;
          }
        },
        RemotePlayerEventType: { ANY_CHANGE: "any-change" },
      },
    },
  });
  const image = vi.fn(
    class {
      constructor(url: string) {
        Object.assign(this, { url });
      }
    },
  );
  Object.defineProperty(window, "chrome", {
    configurable: true,
    value: {
      cast: {
        Image: image,
        media: {
          DEFAULT_MEDIA_RECEIVER_APP_ID: "CC1AD845",
          MediaInfo: class {
            constructor(contentId: string, contentType: string) {
              Object.assign(this, { contentId, contentType });
            }
          },
          GenericMediaMetadata: class {},
          TrackType: { TEXT: "TEXT" },
          Track: class {
            constructor(trackId: number, type: string) {
              Object.assign(this, { trackId, type });
            }
          },
          LoadRequest: class {
            constructor(mediaInfo: unknown) {
              Object.assign(this, { mediaInfo });
            }
          },
        },
      },
    },
  });
  cleanup = () => {
    delete window.cast;
    delete window.chrome;
  };
  manager = (await import("./cast")).castManager;
  return { player, controller, emit, session, context, image, mediaSession, emitMedia };
}
const media = {
  contentId: "http://192.168.1.20/api/v1/stream/direct/7?token=signed",
  itemId: 7,
  currentTime: 45,
  duration: 600,
};

describe("Google Cast completion", () => {
  it("recognizes FINISHED only and keeps the final position despite idle SDK time resetting", async () => {
    const sdk = await google();
    await manager?.requestCastSession(media);
    for (const reason of [null, "CANCELLED", "INTERRUPTED", "ERROR"]) {
      sdk.mediaSession.playerState = "IDLE";
      sdk.mediaSession.idleReason = reason;
      sdk.emitMedia();
      expect(manager?.getState().playbackEnded).toBe(false);
    }
    sdk.mediaSession.idleReason = "FINISHED";
    sdk.emitMedia();
    const load = manager?.getState().mediaLoadId;
    sdk.player.currentTime = 0;
    sdk.player.playerState = "IDLE";
    sdk.emit();
    sdk.emitMedia();
    expect(manager?.getState()).toMatchObject({
      playbackEnded: true,
      currentTime: 600,
      mediaLoadId: load,
      itemId: 7,
    });
  });
  it("removes old media listeners and resets completion for the next load", async () => {
    const sdk = await google();
    await manager?.requestCastSession(media);
    const oldListener = sdk.mediaSession.addUpdateListener.mock.calls[0][0];
    sdk.mediaSession.playerState = "IDLE";
    sdk.mediaSession.idleReason = "FINISHED";
    sdk.emitMedia();
    await manager?.requestCastSession({ ...media, itemId: 8 }, undefined, async () => {
      sdk.emit();
      expect(manager?.getState().playbackEnded).toBe(false);
      return { ...media, itemId: 8, contentId: "http://192.168.1.20/next.mp4" };
    });
    expect(sdk.mediaSession.removeUpdateListener).toHaveBeenCalledWith(oldListener);
    sdk.mediaSession.playerState = "IDLE";
    sdk.mediaSession.idleReason = "FINISHED";
    oldListener(true);
    expect(manager?.getState()).toMatchObject({ playbackEnded: false, itemId: 8 });
    manager?.disposeSession();
    expect(sdk.mediaSession.removeUpdateListener).toHaveBeenCalledTimes(2);
  });
});

it("does not treat a stale finished state on initial load as a new completion", async () => {
  const sdk = await google();
  sdk.session.loadMedia.mockImplementationOnce(async (request) => {
    sdk.mediaSession.media.contentId = (
      request as { mediaInfo: { contentId: string } }
    ).mediaInfo.contentId;
    sdk.mediaSession.playerState = "IDLE";
    sdk.mediaSession.idleReason = "FINISHED";
  });
  await manager?.requestCastSession(media);
  expect(manager?.getState().playbackEnded).toBe(false);
  sdk.mediaSession.playerState = "PLAYING";
  sdk.mediaSession.idleReason = null;
  sdk.emitMedia();
  sdk.mediaSession.playerState = "IDLE";
  sdk.mediaSession.idleReason = "FINISHED";
  sdk.emitMedia();
  expect(manager?.getState().playbackEnded).toBe(true);
});

describe("Google Cast lifecycle", () => {
  it("loads media with the SDK image constructor and preserves zero receiver volume", async () => {
    const sdk = await google();
    expect(await manager?.requestCastSession({ ...media, imageUrl: "/poster.jpg" })).toBe(true);
    expect(sdk.image).toHaveBeenCalled();
    expect(sdk.session.loadMedia).toHaveBeenCalledWith(
      expect.objectContaining({ currentTime: 45, autoplay: true }),
    );
    sdk.player.volumeLevel = 0;
    sdk.player.currentTime = 80;
    sdk.player.duration = 600;
    sdk.emit();
    expect(manager?.getState()).toMatchObject({ isCasting: true, volume: 0, currentTime: 80 });
  });
  it("negotiates only after the receiver is chosen and loads explicit MIME and caption IDs", async () => {
    const sdk = await google();
    const prepare = vi.fn(async () => {
      expect(sdk.context.requestSession).toHaveBeenCalledTimes(1);
      return {
        ...media,
        contentType: "application/vnd.apple.mpegurl",
        subtitles: [
          {
            index: 4,
            url: "http://192.168.1.20/sub.vtt",
            language: "sv",
            title: "Svenska",
            isDefault: true,
            format: "vtt",
          },
        ],
        subtitleIndex: 4,
      };
    });
    expect(await manager?.requestCastSession(media, undefined, prepare)).toBe(true);
    expect(sdk.session.loadMedia).toHaveBeenCalledWith(
      expect.objectContaining({
        activeTrackIds: [5],
        mediaInfo: expect.objectContaining({
          contentType: "application/vnd.apple.mpegurl",
          tracks: [
            expect.objectContaining({ trackId: 5, trackContentType: "text/vtt", language: "sv" }),
          ],
        }),
      }),
    );
  });
  it("rejects loopback media URLs with an actionable error", async () => {
    const sdk = await google();
    expect(
      await manager?.requestCastSession({ ...media, contentId: "http://127.0.0.1/movie.mp4" }),
    ).toBe(false);
    expect(sdk.session.loadMedia).not.toHaveBeenCalled();
    expect(manager?.getLastError()).toContain("LAN address");
  });
  it("consumes remote commands through the authenticated player socket", async () => {
    let socket: { onmessage?: (event: { data: string }) => void } | undefined;
    vi.stubGlobal(
      "WebSocket",
      class {
        onmessage?: (event: { data: string }) => void;
        onclose = null;
        constructor() {
          socket = this;
        }
        close() {}
      },
    );
    const sdk = await google();
    await manager?.requestCastSession(media);
    await tick();
    socket?.onmessage?.({
      data: JSON.stringify({ action: "seek", params: { positionMs: 88000 } }),
    });
    await tick();
    expect(sdk.controller.seek).toHaveBeenCalledTimes(1);
    expect(sdk.player.currentTime).toBe(88);
  });
  it("uses idempotent play/pause commands and sends mute to the receiver", async () => {
    const sdk = await google();
    await manager?.requestCastSession(media);
    manager?.pause();
    manager?.pause();
    expect(sdk.controller.playOrPause).toHaveBeenCalledTimes(1);
    manager?.play();
    manager?.play();
    expect(sdk.controller.playOrPause).toHaveBeenCalledTimes(2);
    manager?.toggleMute();
    expect(manager?.getState().isMuted).toBe(true);
  });
  it("unregisters on receiver disconnect and resets the previous media state", async () => {
    const sdk = await google();
    await manager?.requestCastSession(media);
    await tick();
    sdk.player.isConnected = false;
    sdk.emit();
    await tick();
    expect(client.DELETE).toHaveBeenCalledWith("/api/v1/players/{id}", {
      params: { path: { id: expect.stringMatching(/^cast-/) } },
    });
    expect(manager?.getState()).toMatchObject({ isCasting: false, currentTime: 0, duration: 0 });
  });
  it("does not open a second chooser after Google Cast cancellation", async () => {
    const sdk = await google();
    sdk.context.requestSession.mockRejectedValueOnce("cancel");
    const video = document.createElement("video");
    const prompt = vi.fn();
    Object.defineProperty(video, "remote", { value: { prompt } });
    expect(await manager?.requestCastSession(media, video)).toBe(false);
    expect(prompt).not.toHaveBeenCalled();
  });
  it("ends a pending session when its player has already been closed", async () => {
    const sdk = await google();
    let finish!: () => void;
    sdk.session.loadMedia.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const request = manager!.requestCastSession(media);
    await tick();
    manager?.disposeSession();
    finish();
    expect(await request).toBe(false);
    await tick();
    expect(sdk.context.endCurrentSession).toHaveBeenCalled();
    expect(client.POST).not.toHaveBeenCalled();
  });
  it("queues unregistration after a pending heartbeat", async () => {
    const sdk = await google();
    let finish!: () => void;
    vi.mocked(client.POST).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = () => resolve({ response: new Response() } as never);
        }) as never,
    );
    await manager?.requestCastSession(media);
    await tick();
    manager?.stopCasting();
    expect(client.DELETE).not.toHaveBeenCalled();
    finish();
    await tick();
    expect(client.DELETE).toHaveBeenCalledTimes(1);
    expect(sdk.context.endCurrentSession).toHaveBeenCalledTimes(1);
  });
});

describe("native Remote Playback", () => {
  it("routes playback controls to the video and follows native disconnection", async () => {
    manager = (await import("./cast")).castManager;
    const video = document.createElement("video");
    class Remote extends EventTarget {
      state = "connected";
      prompt = vi.fn(async () => {});
    }
    const remote = new Remote();
    Object.defineProperty(video, "remote", { value: remote });
    const play = vi.spyOn(video, "play").mockResolvedValue();
    const pause = vi.spyOn(video, "pause").mockImplementation(() => {});
    expect(await manager.requestCastSession(media, video)).toBe(true);
    manager.seek(90);
    manager.setVolume(0);
    manager.pause();
    manager.play();
    manager.toggleMute();
    expect(video.currentTime).toBe(90);
    expect(video.volume).toBe(0);
    expect(video.muted).toBe(true);
    expect(play).toHaveBeenCalled();
    expect(pause).toHaveBeenCalled();
    remote.state = "disconnected";
    remote.dispatchEvent(new Event("disconnect"));
    await tick();
    expect(manager.getState().isCasting).toBe(false);
    expect(client.DELETE).toHaveBeenCalled();
  });
  it("does not revive native casting when play resolves after the player closes", async () => {
    manager = (await import("./cast")).castManager;
    const video = document.createElement("video");
    class Remote extends EventTarget {
      state = "connected";
      prompt = vi.fn(async () => {});
    }
    Object.defineProperty(video, "remote", { value: new Remote() });
    let finish!: () => void;
    vi.spyOn(video, "play").mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    vi.spyOn(video, "pause").mockImplementation(() => {});
    const request = manager.requestCastSession(media, video);
    await tick();
    manager.disposeSession();
    finish();
    expect(await request).toBe(false);
    expect(manager.getState().isCasting).toBe(false);
    expect(client.POST).not.toHaveBeenCalled();
  });
});

it("keeps the active receiver and completion listener when server renegotiation is rejected", async () => {
  const sdk = await google();
  await manager?.requestCastSession({ ...media, itemId: 7 });
  const oldLoad = manager?.getState().mediaLoadId;
  const loaded = sdk.session.loadMedia.mock.calls.length;
  const removed = sdk.mediaSession.removeUpdateListener.mock.calls.length;
  expect(
    await manager?.requestCastSession({ ...media, itemId: 8 }, undefined, async () => {
      throw new Error("No free HLS capacity");
    }),
  ).toBe(false);
  expect(manager?.getState()).toMatchObject({ isCasting: true, itemId: 7, mediaLoadId: oldLoad });
  expect(sdk.session.loadMedia).toHaveBeenCalledTimes(loaded);
  expect(sdk.mediaSession.removeUpdateListener).toHaveBeenCalledTimes(removed);
  sdk.mediaSession.playerState = "IDLE";
  sdk.mediaSession.idleReason = "FINISHED";
  sdk.emitMedia();
  expect(manager?.getState().playbackEnded).toBe(true);
});
