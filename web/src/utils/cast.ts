/**
 * Chromecast & Remote Playback Management (M12).
 *
 * Implements the Google Cast Web Sender SDK integration with fallback
 * to the W3C Remote Playback API for browsers without Google Cast Framework.
 * Also synchronizes cast sessions with the MBL server PlayerRegistry.
 */

import client from "../api/client";

export interface CastMediaOptions {
  contentId: string; // Absolute or relative stream URL
  autoplay?: boolean;
  contentType?: string; // e.g. "application/x-mpegurl" or "video/mp4"
  title?: string;
  subtitle?: string;
  imageUrl?: string;
  currentTime?: number;
  duration?: number;
  itemId?: number;
  subtitles?: Array<{
    index: number;
    url: string;
    language?: string | null;
    title?: string | null;
    isDefault?: boolean;
    format?: string;
  }>;
  subtitleIndex?: number | null;
}

export type RemoteCommand = {
  action: string;
  params?: {
    itemId?: number;
    startMs?: number;
    positionMs?: number;
    volume?: number;
    audioStreamIndex?: number | null;
    subtitleStreamIndex?: number | null;
    message?: string | null;
  };
};

interface MediaSessionLike {
  playerState?: string;
  idleReason?: string | null;
  media?: { contentId?: string };
  addUpdateListener?: (listener: (alive: boolean) => void) => void;
  removeUpdateListener?: (listener: (alive: boolean) => void) => void;
  editTracksInfo: (request: unknown, success: () => void, error: () => void) => void;
}

export type CastStateListener = (state: {
  playbackEnded: boolean;
  mediaLoadId: number;
  itemId: number | null;
  supportsAutoplayNext: boolean;
  isAvailable: boolean;
  isCasting: boolean;
  deviceName: string | null;
  playerState: "idle" | "playing" | "paused" | "buffering";
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
}) => void;

interface RemotePlayerLike {
  isConnected?: boolean;
  isPaused?: boolean;
  displayName?: string;
  playerState?: string;
  currentTime?: number;
  duration?: number;
  volumeLevel?: number;
  isMuted?: boolean;
}

interface RemotePlayerControllerLike {
  addEventListener: (event: string, callback: () => void) => void;
  playOrPause: () => void;
  seek: () => void;
  setVolumeLevel: () => void;
  stop: () => void;
  muteOrUnmute: () => void;
}

type HTMLMediaElementWithRemote = HTMLMediaElement;

// Declare Google Cast global typings
declare global {
  interface Window {
    __onGCastApiAvailable?: (isAvailable: boolean) => void;
    cast?: {
      framework?: {
        CastContext: {
          getInstance: () => {
            setOptions: (opts: Record<string, unknown>) => void;
            getCurrentSession: () => {
              getSessionId: () => string;
              getMediaSession: () => MediaSessionLike | null;
              getCastDevice: () => { friendlyName?: string };
              loadMedia: (req: unknown) => Promise<unknown>;
            } | null;
            requestSession: () => Promise<void>;
            endCurrentSession: (stopCasting: boolean) => void;
          };
        };
        RemotePlayer: new () => RemotePlayerLike;
        RemotePlayerController: new (player: RemotePlayerLike) => RemotePlayerControllerLike;
        RemotePlayerEventType: Record<string, string>;
      };
    };
    chrome?: {
      cast?: {
        AutoJoinPolicy?: Record<string, string>;
        Image: new (url: string) => { url: string };
        media?: {
          DEFAULT_MEDIA_RECEIVER_APP_ID?: string;
          MediaInfo: new (
            contentId: string,
            contentType: string,
          ) => {
            tracks?: Array<Record<string, unknown>>;
            metadata?: {
              title?: string;
              subtitle?: string;
              images?: Array<{ url: string }>;
            };
          };
          Track?: new (id: number, type: string) => Record<string, unknown>;
          TrackType?: { TEXT: string };
          EditTracksInfoRequest?: new (ids: number[]) => unknown;
          GenericMediaMetadata: new () => Record<string, unknown>;
          LoadRequest: new (
            mediaInfo: unknown,
          ) => {
            autoplay?: boolean;
            currentTime?: number;
            activeTrackIds?: number[];
          };
        };
      };
    };
  }
}

class CastManager {
  private playbackEnded = false;
  private mediaHasPlayed = false;
  private mediaLoadId = 0;
  private mediaContentId: string | null = null;
  private observedMedia: MediaSessionLike | null = null;
  private detachMedia: (() => void) | null = null;
  private isAvailable = false;
  private isCasting = false;
  private deviceName: string | null = null;
  private playerState: "idle" | "playing" | "paused" | "buffering" = "idle";
  private currentTime = 0;
  private duration = 0;
  private volume = 1;
  private isMuted = false;
  private activeItemId: number | null = null;
  private castSessionId: string | null = null;
  private listeners: Set<CastStateListener> = new Set();
  private remotePlayer: RemotePlayerLike | null = null;
  private remotePlayerController: RemotePlayerControllerLike | null = null;
  private heartbeatInterval: number | null = null;
  private mode: "google" | "remote" | null = null;
  private nativeVideo: HTMLVideoElement | null = null;
  private detachNative: (() => void) | null = null;
  private registryQueue: Promise<void> = Promise.resolve();
  private lastError: string | null = null;
  private requesting = false;
  private generation = 0;
  private socket: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private commandHandler: ((command: RemoteCommand) => Promise<void>) | null = null;

  constructor() {
    this.initCastApi();
  }

  public subscribe(listener: CastStateListener): () => void {
    this.listeners.add(listener);
    this.notify();
    return () => this.listeners.delete(listener);
  }

  public setCommandHandler(handler: (command: RemoteCommand) => Promise<void>) {
    this.commandHandler = handler;
    return () => {
      if (this.commandHandler === handler) this.commandHandler = null;
    };
  }

  private async dispatchCommand(command: RemoteCommand) {
    switch (command.action) {
      case "pause":
        this.pause();
        break;
      case "resume":
        this.play();
        break;
      case "seek":
        if (typeof command.params?.positionMs === "number")
          this.seek(command.params.positionMs / 1000);
        break;
      case "setVolume":
        if (typeof command.params?.volume === "number") this.setVolume(command.params.volume / 100);
        break;
      default:
        if (this.commandHandler) await this.commandHandler(command);
        else if (command.action === "stop") this.disposeSession();
    }
  }

  private connectCommands(id: string) {
    if (this.socket || typeof WebSocket === "undefined" || this.castSessionId !== id) return;
    const url = new URL(
      `/api/v1/players/${encodeURIComponent(id)}/connect`,
      window.location.origin,
    );
    url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
    const socket = new WebSocket(url);
    this.socket = socket;
    socket.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      try {
        const command = JSON.parse(event.data) as RemoteCommand;
        void this.dispatchCommand(command).catch(() => {});
      } catch {}
    };
    socket.onclose = () => {
      if (this.socket === socket) this.socket = null;
      if (this.castSessionId === id && this.isCasting) {
        this.reconnectTimer = setTimeout(() => {
          this.reconnectTimer = null;
          this.syncPlayerRegistry();
        }, 2000);
      }
    };
  }

  public selectSubtitle(index: number | null): Promise<boolean> {
    if (this.mode === "remote" && this.nativeVideo) {
      for (let i = 0; i < this.nativeVideo.textTracks.length; i++)
        this.nativeVideo.textTracks[i].mode = index === i ? "showing" : "disabled";
      return Promise.resolve(true);
    }
    const media = window.cast?.framework?.CastContext.getInstance()
      .getCurrentSession()
      ?.getMediaSession();
    const Request = window.chrome?.cast?.media?.EditTracksInfoRequest;
    if (!media || !Request) return Promise.resolve(false);
    return new Promise((resolve) =>
      media.editTracksInfo(
        new Request(index === null ? [] : [index + 1]),
        () => resolve(true),
        () => resolve(false),
      ),
    );
  }

  public getLastError() {
    return this.lastError;
  }

  public getState() {
    return {
      playbackEnded: this.playbackEnded && !this.requesting,
      mediaLoadId: this.mediaLoadId,
      itemId: this.activeItemId,
      supportsAutoplayNext: this.mode === "google",
      isAvailable: this.isAvailable,
      isCasting: this.isCasting,
      deviceName: this.deviceName,
      playerState: this.playerState,
      currentTime: this.currentTime,
      duration: this.duration,
      volume: this.volume,
      isMuted: this.isMuted,
    };
  }

  private notify() {
    const s = this.getState();
    for (const listener of this.listeners) {
      listener(s);
    }
  }

  /**
   * Initializes the Google Cast Web Sender SDK.
   */
  public initCastApi() {
    if (typeof window === "undefined") return;

    // Check if W3C Remote Playback API is supported as fallback
    if ("HTMLMediaElement" in window && "remote" in HTMLMediaElement.prototype) {
      this.isAvailable = true;
      this.notify();
    }

    if (window.cast?.framework) {
      this.setupCastFramework();
      return;
    }

    window.__onGCastApiAvailable = (isAvailable: boolean) => {
      if (isAvailable) {
        this.setupCastFramework();
      }
    };

    // Dynamically inject Google Cast SDK script if not already present
    if (!document.querySelector('script[src*="cast_sender.js"]')) {
      const script = document.createElement("script");
      script.src = "https://www.gstatic.com/cv/js/sender/v1/cast_sender.js?loadCastFramework=1";
      script.async = true;
      document.head.appendChild(script);
    }
  }

  private setupCastFramework() {
    try {
      const framework = window.cast?.framework;
      if (!framework) return;

      const castContext = framework.CastContext.getInstance();
      const defaultAppId = window.chrome?.cast?.media?.DEFAULT_MEDIA_RECEIVER_APP_ID || "CC1AD845";

      castContext.setOptions({
        receiverApplicationId: defaultAppId,
        autoJoinPolicy: window.chrome?.cast?.AutoJoinPolicy?.ORIGIN_SCOPED || "origin_scoped",
      });

      this.remotePlayer = new framework.RemotePlayer();
      this.remotePlayerController = new framework.RemotePlayerController(this.remotePlayer);

      // ANY_CHANGE is a documented SDK event; PLAYER_STATE_CHANGED is not.
      this.remotePlayerController.addEventListener(framework.RemotePlayerEventType.ANY_CHANGE, () =>
        this.updateRemoteState(),
      );

      this.isAvailable = true;
      this.notify();
    } catch {
      // Framework init may fail if run in non-standard context
    }
  }

  private observeMedia() {
    if (this.requesting) return;
    const media = window.cast?.framework?.CastContext.getInstance()
      .getCurrentSession()
      ?.getMediaSession?.();
    if (!media || media === this.observedMedia || !this.mediaContentId) return;
    if (media.media?.contentId !== this.mediaContentId) return;
    this.detachMedia?.();
    this.observedMedia = media;
    const loadId = this.mediaLoadId;
    const update = () => {
      if (
        !this.isCasting ||
        this.requesting ||
        this.mode !== "google" ||
        loadId !== this.mediaLoadId ||
        this.observedMedia !== media ||
        this.playbackEnded
      )
        return;
      if (media.playerState === "PLAYING") this.mediaHasPlayed = true;
      if (this.mediaHasPlayed && media.playerState === "IDLE" && media.idleReason === "FINISHED") {
        this.playbackEnded = true;
        this.playerState = "idle";
        if (this.duration > 0) this.currentTime = this.duration;
        this.notify();
      }
    };
    media.addUpdateListener?.(update);
    this.detachMedia = () => {
      media.removeUpdateListener?.(update);
      this.observedMedia = null;
    };
    update();
  }

  private updateRemoteState() {
    if (this.mode !== "google" || !this.remotePlayer) return;
    if (!this.remotePlayer.isConnected) {
      if (this.isCasting) this.clearSession();
      return;
    }
    this.observeMedia();
    const previousState = this.playerState;
    if (!this.playbackEnded) this.currentTime = this.remotePlayer.currentTime ?? this.currentTime;
    this.duration = this.remotePlayer.duration ?? this.duration;
    this.volume = this.remotePlayer.volumeLevel ?? this.volume;
    this.isMuted = this.remotePlayer.isMuted ?? false;
    const raw = this.playbackEnded ? "IDLE" : this.remotePlayer.playerState?.toUpperCase();
    this.playerState =
      raw === "PLAYING"
        ? "playing"
        : raw === "PAUSED"
          ? "paused"
          : raw === "BUFFERING"
            ? "buffering"
            : "idle";
    this.notify();
    if (previousState !== this.playerState) this.syncPlayerRegistry();
  }

  private clearSession() {
    this.detachMedia?.();
    this.detachMedia = null;
    this.playbackEnded = false;
    this.mediaHasPlayed = false;
    this.mediaContentId = null;
    this.mediaLoadId++;
    this.generation++;
    const id = this.castSessionId;
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      socket.onclose = null;
      socket.close();
    }
    this.stopHeartbeat();
    this.detachNative?.();
    this.detachNative = null;
    this.nativeVideo = null;
    this.mode = null;
    this.castSessionId = null;
    this.activeItemId = null;
    this.isCasting = false;
    this.deviceName = null;
    this.playerState = "idle";
    this.currentTime = 0;
    this.duration = 0;
    this.volume = 1;
    this.isMuted = false;
    if (id) {
      // Serialize teardown after already queued heartbeats so a late POST cannot resurrect it.
      this.registryQueue = this.registryQueue
        .then(async () => {
          await client.DELETE("/api/v1/players/{id}", { params: { path: { id } } });
        })
        .catch(() => {});
    }
    this.notify();
  }

  /**
   * Prompts user to connect to a Cast device and loads media.
   */
  public async requestCastSession(
    media: CastMediaOptions,
    videoElement?: HTMLVideoElement,
    prepare?: () => Promise<CastMediaOptions>,
  ): Promise<boolean> {
    if (this.requesting) return false;
    this.requesting = true;
    this.lastError = null;
    const generation = ++this.generation;
    const previousItemId = this.activeItemId;
    const hadActiveGoogleSession = this.isCasting && this.mode === "google";
    let loadStarted = false;
    try {
      this.activeItemId = media.itemId ?? null;

      // 1. Try Google Cast Web SDK
      const framework = window.cast?.framework;
      const chromeCastMedia = window.chrome?.cast?.media;
      if (framework && chromeCastMedia) {
        try {
          const castContext = framework.CastContext.getInstance();
          this.mode = "google";
          if (!this.isCasting || !castContext.getCurrentSession())
            await castContext.requestSession();
          if (generation !== this.generation) {
            castContext.endCurrentSession(true);
            return false;
          }
          const session = castContext.getCurrentSession();
          if (!session) {
            this.clearSession();
            return false;
          }

          if (prepare) media = await prepare();
          if (generation !== this.generation) return false;
          this.activeItemId = media.itemId ?? null;
          const url = this.toAbsoluteUrl(media.contentId);
          if (["localhost", "127.0.0.1", "[::1]"].includes(new URL(url).hostname)) {
            throw new Error(
              "Open MBL using a LAN address or configure a receiver-reachable public URL before casting.",
            );
          }
          const contentType =
            media.contentType ||
            (new URL(url).pathname.toLowerCase().endsWith(".m3u8")
              ? "application/x-mpegurl"
              : "video/mp4");

          const mediaInfo = new chromeCastMedia.MediaInfo(url, contentType);
          mediaInfo.metadata = new chromeCastMedia.GenericMediaMetadata();
          if (mediaInfo.metadata) {
            mediaInfo.metadata.title = media.title || "MBL Video";
            mediaInfo.metadata.subtitle = media.subtitle || "";
            if (media.imageUrl) {
              mediaInfo.metadata.images = [
                new window.chrome!.cast!.Image(this.toAbsoluteUrl(media.imageUrl)),
              ];
            }
          }

          if (media.subtitles?.length && chromeCastMedia.Track) {
            mediaInfo.tracks = media.subtitles.map((subtitle) =>
              Object.assign(
                new chromeCastMedia.Track!(
                  subtitle.index + 1,
                  chromeCastMedia.TrackType?.TEXT ?? "TEXT",
                ),
                {
                  trackContentId: this.toAbsoluteUrl(subtitle.url),
                  trackContentType: "text/vtt",
                  subtype: "SUBTITLES",
                  name: subtitle.title || subtitle.language || "Subtitles",
                  language: subtitle.language || "und",
                },
              ),
            );
          }
          const request = new chromeCastMedia.LoadRequest(mediaInfo);
          const selected =
            media.subtitleIndex === undefined
              ? media.subtitles?.find((s) => s.isDefault)?.index
              : media.subtitleIndex;
          request.activeTrackIds =
            selected === null || selected === undefined ? [] : [selected + 1];
          request.autoplay = media.autoplay ?? true;
          request.currentTime = media.currentTime || 0;

          this.detachMedia?.();
          this.detachMedia = null;
          this.playbackEnded = false;
          this.mediaHasPlayed = false;
          this.mediaLoadId++;
          this.mediaContentId = url;
          loadStarted = true;
          await session.loadMedia(request);
          if (generation !== this.generation) {
            castContext.endCurrentSession(true);
            return false;
          }
          this.isCasting = true;
          this.deviceName = session.getCastDevice()?.friendlyName || "Google Cast";
          this.castSessionId ??= `cast-${crypto.randomUUID()}`;
          this.currentTime = media.currentTime ?? 0;
          this.duration = media.duration ?? 0;
          this.playerState = request.autoplay ? "playing" : "paused";
          this.startHeartbeat();
          this.notify();
          this.syncPlayerRegistry();
          return true;
        } catch (err: unknown) {
          this.lastError =
            err === "cancel"
              ? null
              : err instanceof Error
                ? err.message
                : "The receiver could not load this media.";
          // A rejected server negotiation has not touched the receiver source.
          // Keep the existing media listener, session registration and controls.
          if (hadActiveGoogleSession && !loadStarted && generation === this.generation) {
            this.activeItemId = previousItemId;
            this.notify();
          } else {
            this.clearSession();
          }
          // Cancellation must not open a second native chooser.
          if (err !== "cancel") console.warn("Cast request failed:", err);
          return false;
        }
      }

      // 2. Fallback to HTMLMediaElement Remote Playback API
      const remoteEl = videoElement as HTMLMediaElementWithRemote | undefined;
      if (remoteEl?.remote) {
        try {
          await remoteEl.remote.prompt();
          if (generation !== this.generation) return false;
          if (remoteEl.remote.state !== "connected") return false;
          this.mode = "remote";
          this.nativeVideo = videoElement ?? null;
          this.castSessionId = `remote-${crypto.randomUUID()}`;
          this.deviceName = "Remote display";
          const update = () => {
            if (remoteEl.remote.state === "disconnected") {
              this.clearSession();
              return;
            }
            this.isCasting = remoteEl.remote.state === "connected";
            this.currentTime = remoteEl.currentTime;
            this.duration = Number.isFinite(remoteEl.duration)
              ? remoteEl.duration
              : (media.duration ?? 0);
            this.volume = remoteEl.volume;
            this.isMuted = remoteEl.muted;
            this.playerState = remoteEl.paused ? "paused" : "playing";
            this.notify();
          };
          const events = ["timeupdate", "play", "pause", "volumechange", "durationchange"];
          for (const event of events) remoteEl.addEventListener(event, update);
          remoteEl.remote.addEventListener("connect", update);
          remoteEl.remote.addEventListener("disconnect", update);
          this.detachNative = () => {
            for (const event of events) remoteEl.removeEventListener(event, update);
            remoteEl.remote.removeEventListener("connect", update);
            remoteEl.remote.removeEventListener("disconnect", update);
          };
          if (media.autoplay !== false) await remoteEl.play();
          if (generation !== this.generation) {
            remoteEl.pause();
            return false;
          }
          update();
          this.startHeartbeat();
          this.syncPlayerRegistry();
          return true;
        } catch {
          this.clearSession();
          return false;
        }
      }

      this.clearSession();
      return false;
    } finally {
      this.requesting = false;
      if (this.mode === "google" && this.isCasting) this.observeMedia();
    }
  }

  public play() {
    if (this.mode === "remote") {
      void this.nativeVideo?.play().catch(() => {});
      return;
    }
    if (this.remotePlayer?.isPaused) this.remotePlayerController?.playOrPause();
  }

  public pause() {
    if (this.mode === "remote") {
      this.nativeVideo?.pause();
      return;
    }
    if (this.remotePlayer && !this.remotePlayer.isPaused)
      this.remotePlayerController?.playOrPause();
  }

  public seek(seconds: number) {
    if (this.mode === "remote" && this.nativeVideo) {
      this.nativeVideo.currentTime = seconds;
      return;
    }
    if (this.remotePlayer && this.remotePlayerController) {
      this.remotePlayer.currentTime = seconds;
      this.remotePlayerController.seek();
    }
  }

  public setVolume(level: number) {
    if (this.mode === "remote" && this.nativeVideo) {
      this.nativeVideo.volume = Math.max(0, Math.min(1, level));
      return;
    }
    if (this.remotePlayer && this.remotePlayerController) {
      this.remotePlayer.volumeLevel = Math.max(0, Math.min(1, level));
      this.remotePlayerController.setVolumeLevel();
    }
  }

  public toggleMute() {
    if (this.mode === "remote" && this.nativeVideo)
      this.nativeVideo.muted = !this.nativeVideo.muted;
    else this.remotePlayerController?.muteOrUnmute();
  }

  public stopCasting() {
    if (this.mode === "remote" && this.nativeVideo) {
      // Remote Playback has no programmatic disconnect; the browser owns this chooser.
      void this.nativeVideo.remote.prompt().catch(() => {});
      return;
    }
    if (this.mode === "google") {
      try {
        window.cast?.framework?.CastContext.getInstance().endCurrentSession(true);
      } catch {}
    }
    this.clearSession();
  }

  public disposeSession() {
    this.generation++;
    if (this.mode === "remote") {
      this.nativeVideo?.pause();
      this.clearSession();
    } else this.stopCasting();
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatInterval = window.setInterval(() => {
      this.syncPlayerRegistry();
    }, 10_000);
  }

  private stopHeartbeat() {
    if (this.heartbeatInterval) {
      clearInterval(this.heartbeatInterval);
      this.heartbeatInterval = null;
    }
  }

  private syncPlayerRegistry() {
    if (!this.isCasting || !this.castSessionId) return;

    // Report Chromecast presence to MBL Server PlayerRegistry
    const body = {
      id: this.castSessionId,
      lastSeenAt: Date.now(),
      name: this.deviceName || "Remote display",
      deviceType: this.mode === "remote" ? "remote" : "chromecast",
      state: this.playerState,
      nowPlayingItemId: this.activeItemId,
      positionMs: Math.round(this.currentTime * 1000),
      durationMs: Math.round(this.duration * 1000),
      volume: Math.round(this.volume * 100),
    };
    this.registryQueue = this.registryQueue
      .then(async () => {
        const result = await client.POST("/api/v1/players", {
          body,
          signal: AbortSignal.timeout(5000),
        });
        if (!result.error && this.castSessionId === body.id) this.connectCommands(body.id);
      })
      .catch(() => {});
  }

  private toAbsoluteUrl(url: string): string {
    if (url.startsWith("http://") || url.startsWith("https://")) {
      return url;
    }
    return new URL(url, window.location.origin).href;
  }
}

export const castManager = new CastManager();
