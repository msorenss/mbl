/**
 * Chromecast & Remote Playback Management (M12).
 *
 * Implements the Google Cast Web Sender SDK integration with fallback
 * to the W3C Remote Playback API for browsers without Google Cast Framework.
 * Also synchronizes cast sessions with the MBL server PlayerRegistry.
 */

import client from "../api/client";
import { castDiagnostics } from "./castDiagnostics";

export interface CastMediaOptions {
  contentId: string; // Absolute or relative stream URL
  autoplay?: boolean;
  hlsFmp4?: boolean; // MBL-produced HLS uses fragmented MP4 segments.
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
  isMediaLoaded?: boolean;
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
            addEventListener?: (event: string, callback: (event: unknown) => void) => void;
          };
        };
        CastContextEventType?: Record<string, string>;
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
          HlsSegmentFormat?: { FMP4: string };
          HlsVideoSegmentFormat?: { FMP4: string };
          StreamType?: { BUFFERED: string; LIVE: string; OTHER: string };
          MetadataType?: { GENERIC: number; MOVIE: number; TV_SHOW: number; MUSIC_TRACK: number };
          MediaInfo: new (
            contentId: string,
            contentType: string,
          ) => {
            streamType?: string;
            duration?: number;
            hlsSegmentFormat?: string;
            hlsVideoSegmentFormat?: string;
            tracks?: Array<Record<string, unknown>>;
            metadata?: {
              metadataType?: number;
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
  private diagnosticState = "";
  private generation = 0;
  private socket: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private commandHandler: ((command: RemoteCommand) => Promise<void>) | null = null;
  private frameworkReadyPromise: Promise<boolean> | null = null;
  private frameworkReadyResolver: (() => void) | null = null;

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
    castDiagnostics.record("command.received", { action: command.action });
    switch (command.action) {
      case "pause":
        this.pause();
        break;
      case "resume":
        this.play();
        break;
      case "seek":
        if (this.commandHandler) await this.commandHandler(command);
        else if (typeof command.params?.positionMs === "number")
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
    socket.onopen = () => castDiagnostics.record("commands.connected");
    socket.onerror = () => castDiagnostics.record("commands.error");
    socket.onmessage = (event) => {
      if (typeof event.data !== "string") return;
      try {
        const command = JSON.parse(event.data) as RemoteCommand;
        void this.dispatchCommand(command).catch(() => {});
      } catch {}
    };
    socket.onclose = (event) => {
      castDiagnostics.record("commands.closed", { code: event?.code, clean: event?.wasClean });
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
    if (!media || !Request) {
      castDiagnostics.record("subtitles.error", { reason: "no_media_or_request" });
      return Promise.resolve(false);
    }
    const activeTrackIds = index === null ? [] : [index + 1];
    const request = new Request(activeTrackIds) as Record<string, unknown>;
    request.activeTrackIds = activeTrackIds;
    castDiagnostics.record("subtitles.edit", { index, activeTrackIds });
    return new Promise((resolve) =>
      media.editTracksInfo(
        request,
        () => {
          castDiagnostics.record("subtitles.success", { index, activeTrackIds });
          resolve(true);
        },
        () => {
          castDiagnostics.record("subtitles.error", { index, activeTrackIds });
          console.error("[MBL Cast] editTracksInfo error for track:", index);
          resolve(false);
        },
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

    this.frameworkReadyPromise = new Promise<boolean>((resolve) => {
      if (window.cast?.framework) {
        this.setupCastFramework();
        resolve(true);
        return;
      }
      this.frameworkReadyResolver = () => {
        resolve(!!window.cast?.framework);
      };
      window.setTimeout(() => resolve(!!window.cast?.framework), 2500);
    });

    const previousCallback = window.__onGCastApiAvailable;
    window.__onGCastApiAvailable = (isAvailable: boolean) => {
      castDiagnostics.record("sdk.callback", { isAvailable });
      try {
        if (typeof previousCallback === "function") {
          previousCallback(isAvailable);
        }
      } catch {}
      if (isAvailable) {
        this.setupCastFramework();
      }
      if (this.frameworkReadyResolver) {
        this.frameworkReadyResolver();
        this.frameworkReadyResolver = null;
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
        resumeSavedSession: true,
      });

      this.remotePlayer = new framework.RemotePlayer();
      this.remotePlayerController = new framework.RemotePlayerController(this.remotePlayer);

      // Listen to specific documented RemotePlayerController events (Google Cast Web Sender Guide)
      const eventTypes = [
        framework.RemotePlayerEventType?.ANY_CHANGE,
        framework.RemotePlayerEventType?.IS_CONNECTED_CHANGED,
        framework.RemotePlayerEventType?.IS_MEDIA_LOADED_CHANGED,
        framework.RemotePlayerEventType?.IS_PAUSED_CHANGED,
        framework.RemotePlayerEventType?.CURRENT_TIME_CHANGED,
        framework.RemotePlayerEventType?.DURATION_CHANGED,
        framework.RemotePlayerEventType?.VOLUME_LEVEL_CHANGED,
        framework.RemotePlayerEventType?.IS_MUTED_CHANGED,
        framework.RemotePlayerEventType?.PLAYER_STATE_CHANGED,
      ].filter((ev): ev is string => typeof ev === "string" && ev.length > 0);

      for (const ev of new Set(eventTypes)) {
        try {
          this.remotePlayerController.addEventListener(ev, () => this.updateRemoteState());
        } catch {
          // Keep resilient across mock or minimal SDK environments
        }
      }

      const sessionStateEvent = framework.CastContextEventType?.SESSION_STATE_CHANGED;
      if (sessionStateEvent && typeof castContext.addEventListener === "function") {
        try {
          castContext.addEventListener(sessionStateEvent, (event: unknown) => {
            const ev = event as { sessionState?: string; errorCode?: number };
            castDiagnostics.record("session.state_changed", {
              sessionState: ev?.sessionState,
              errorCode: ev?.errorCode,
            });
            this.updateRemoteState();
          });
        } catch {
          // resilient
        }
      }

      const castStateEvent = framework.CastContextEventType?.CAST_STATE_CHANGED;
      if (castStateEvent && typeof castContext.addEventListener === "function") {
        try {
          castContext.addEventListener(castStateEvent, (event: unknown) => {
            const ev = event as { castState?: string };
            castDiagnostics.record("cast.state_changed", {
              castState: ev?.castState,
            });
          });
        } catch {
          // resilient
        }
      }

      this.isAvailable = true;
      this.notify();
      castDiagnostics.record("sdk.ready");
    } catch (err: unknown) {
      castDiagnostics.error("setup.error", err);
    }
  }

  private observeMedia() {
    if (this.requesting) return;
    const session = window.cast?.framework?.CastContext.getInstance().getCurrentSession();
    const media = session?.getMediaSession?.();
    if (media !== this.observedMedia && this.isCasting) {
      castDiagnostics.record("observe.probe", {
        hasSession: !!session,
        hasMedia: !!media,
        playerState: media?.playerState,
        idleReason: media?.idleReason,
        contentId: media?.media?.contentId,
      });
    }
    if (!media || media === this.observedMedia) return;
    if (
      this.mediaContentId &&
      media.media?.contentId &&
      media.media.contentId !== this.mediaContentId
    )
      return;
    this.detachMedia?.();
    this.observedMedia = media;
    const loadId = this.mediaLoadId;
    const update = (alive?: boolean) => {
      castDiagnostics.record("media.status", {
        alive,
        state: media.playerState,
        idleReason: media.idleReason,
      });
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
    const mediaSession =
      typeof window !== "undefined"
        ? window.cast?.framework?.CastContext.getInstance().getCurrentSession()?.getMediaSession()
        : null;
    const idleReason =
      (this.remotePlayer as unknown as { idleReason?: string }).idleReason ||
      (mediaSession as unknown as { idleReason?: string })?.idleReason ||
      null;
    const snapshot = {
      connected: this.remotePlayer.isConnected,
      loaded: this.remotePlayer.isMediaLoaded,
      state: this.remotePlayer.playerState,
      idleReason,
      paused: this.remotePlayer.isPaused,
      position: Math.floor(this.remotePlayer.currentTime ?? 0),
      duration: this.remotePlayer.duration,
      requesting: this.requesting,
    };
    const key = JSON.stringify(snapshot);
    if (key !== this.diagnosticState) {
      this.diagnosticState = key;
      castDiagnostics.record("receiver.status", snapshot);
    }
    if (!this.remotePlayer.isConnected) {
      if (this.isCasting) this.clearSession("sdk-disconnected");
      return;
    }
    this.observeMedia();
    const previousState = this.playerState;
    const position = this.remotePlayer.currentTime;
    const duration = this.remotePlayer.duration;
    const volume = this.remotePlayer.volumeLevel;
    if (
      !this.playbackEnded &&
      typeof position === "number" &&
      Number.isFinite(position) &&
      position >= 0
    )
      this.currentTime = position;
    // Cast uses -1 (and sometimes 0) while a growing HLS duration is unknown.
    // Keep the known source duration instead of sending invalid registry data.
    if (typeof duration === "number" && Number.isFinite(duration) && duration > 0)
      this.duration = Math.max(this.duration, duration);
    if (typeof volume === "number" && Number.isFinite(volume))
      this.volume = Math.max(0, Math.min(1, volume));
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

  private clearSession(reason = "reset") {
    castDiagnostics.record("session.clear", {
      reason,
      wasCasting: this.isCasting,
      requesting: this.requesting,
    });
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
    castDiagnostics.start();
    const hasCastScript =
      typeof document !== "undefined" && !!document.querySelector('script[src*="cast_sender.js"]');
    if (
      !window.cast?.framework &&
      typeof window !== "undefined" &&
      window.chrome &&
      hasCastScript &&
      this.frameworkReadyPromise
    ) {
      castDiagnostics.record("framework.waiting");
      await this.frameworkReadyPromise;
    }
    castDiagnostics.record("sdk.available", {
      google: !!window.cast?.framework,
      remote: !!videoElement?.remote,
      localPaused: videoElement?.paused,
      localPosition: videoElement?.currentTime,
      isSecureContext: typeof window !== "undefined" ? window.isSecureContext : undefined,
      hasPresentation:
        typeof navigator !== "undefined"
          ? !!(navigator as unknown as { presentation?: unknown }).presentation
          : undefined,
      hasChrome: typeof window !== "undefined" ? !!window.chrome : undefined,
      hasCast: typeof window !== "undefined" ? !!window.cast : undefined,
    });
    this.requesting = true;
    this.lastError = null;
    const generation = ++this.generation;
    const previousItemId = this.activeItemId;
    const hadActiveGoogleSession = this.isCasting && this.mode === "google";
    let loadStarted = false;
    const diagnosticWait = window.setTimeout(() => {
      if (this.requesting && generation === this.generation) {
        castDiagnostics.record("attempt.waiting", {
          mode: this.mode,
          loadStarted,
          elapsedSeconds: 20,
        });
      }
    }, 20000);
    try {
      this.activeItemId = media.itemId ?? null;

      // 1. Try Google Cast Web SDK
      const framework = window.cast?.framework;
      const chromeCastMedia = window.chrome?.cast?.media;
      if (framework && chromeCastMedia) {
        try {
          const castContext = framework.CastContext.getInstance();
          this.mode = "google";
          if (!this.isCasting || !castContext.getCurrentSession()) {
            castDiagnostics.record("chooser.open");
            await castContext.requestSession();
            castDiagnostics.record("chooser.accepted");
          }
          if (generation !== this.generation) {
            castContext.endCurrentSession(true);
            return false;
          }
          const session = castContext.getCurrentSession();
          if (!session) {
            this.clearSession();
            return false;
          }

          if (prepare) {
            castDiagnostics.record("prepare.begin");
            media = await prepare();
            castDiagnostics.record("prepare.ready", {
              contentType: media.contentType,
              url: media.contentId,
              hlsFmp4: media.hlsFmp4,
            });
          }
          if (generation !== this.generation) return false;
          this.activeItemId = media.itemId ?? null;
          const url = this.toAbsoluteUrl(media.contentId);
          if (["localhost", "127.0.0.1", "[::1]"].includes(new URL(url).hostname)) {
            throw new Error(
              "Open MBL using a LAN address or configure a receiver-reachable public URL before casting.",
            );
          }
          const contentType = new URL(url).pathname.toLowerCase().endsWith(".m3u8")
            ? "application/x-mpegurl"
            : media.contentType || "video/mp4";

          const mediaInfo = new chromeCastMedia.MediaInfo(url, contentType);
          mediaInfo.streamType = chromeCastMedia.StreamType?.BUFFERED || "BUFFERED";
          if (
            typeof media.duration === "number" &&
            Number.isFinite(media.duration) &&
            media.duration > 0
          ) {
            mediaInfo.duration = media.duration;
          }
          // Do not force hlsSegmentFormat / hlsVideoSegmentFormat on muxed fMP4 streams;
          // explicit hints invoke legacy Cast MPL 'Unable to derive timescale' errors on Chromecast.
          mediaInfo.metadata = new chromeCastMedia.GenericMediaMetadata();
          if (mediaInfo.metadata) {
            mediaInfo.metadata.metadataType = chromeCastMedia.MetadataType?.GENERIC ?? 0;
            mediaInfo.metadata.title = media.title || "MBL Video";
            mediaInfo.metadata.subtitle = media.subtitle || "";
            if (media.imageUrl) {
              mediaInfo.metadata.images = [
                new window.chrome!.cast!.Image(this.toAbsoluteUrl(media.imageUrl)),
              ];
            }
          }

          const selected =
            media.subtitleIndex === undefined
              ? media.subtitles?.find((s) => s.isDefault)?.index
              : media.subtitleIndex;
          const hasSelectedTrack = selected !== null && selected !== undefined && selected >= 0;
          // Always populate tracks for progressive MP4 so user can toggle subtitles on/off or change languages dynamically without reconnecting.
          // For HLS streams, only attach when a track is actively selected to avoid Shaka Player receiver stalls.
          const allowTracks = !media.hlsFmp4 || hasSelectedTrack;

          if (media.subtitles?.length && chromeCastMedia.Track && allowTracks) {
            let trackSubtitles = media.subtitles;
            if (trackSubtitles.length > 8) {
              const mustInclude = new Set<number>();
              if (hasSelectedTrack) mustInclude.add(selected);
              const def = media.subtitles.find((s) => s.isDefault);
              if (def) mustInclude.add(def.index);
              const priority = trackSubtitles.filter((s) => mustInclude.has(s.index));
              const others = trackSubtitles
                .filter((s) => !mustInclude.has(s.index))
                .slice(0, Math.max(0, 8 - priority.length));
              trackSubtitles = [...priority, ...others];
            }
            mediaInfo.tracks = trackSubtitles.map((subtitle) =>
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
          request.activeTrackIds = hasSelectedTrack ? [selected + 1] : [];
          request.autoplay = media.autoplay ?? true;
          request.currentTime =
            typeof media.currentTime === "number" && media.currentTime >= 5 ? media.currentTime : 0;

          this.detachMedia?.();
          this.detachMedia = null;
          this.playbackEnded = false;
          this.mediaHasPlayed = false;
          this.mediaLoadId++;
          this.mediaContentId = url;
          loadStarted = true;
          castDiagnostics.record("load.send", {
            url,
            contentType,
            streamType: mediaInfo.streamType,
            duration: mediaInfo.duration,
            audioFormat: mediaInfo.hlsSegmentFormat,
            videoFormat: mediaInfo.hlsVideoSegmentFormat,
            autoplay: request.autoplay,
            position: request.currentTime,
            tracks: mediaInfo.tracks?.length ?? 0,
            activeTrackIds: request.activeTrackIds,
          });
          const loadResult = await session.loadMedia(request);
          castDiagnostics.record("load.accepted", { result: loadResult ?? null });
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
          castDiagnostics.error("load.failed", err);
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
          // ignore prompt cancel or rejection
        }
      }

      if (!this.lastError) {
        if (typeof window !== "undefined" && !window.isSecureContext) {
          this.lastError =
            "Google Cast kräver en säker anslutning (HTTPS) eller localhost. Om du öppnar webbläsaren via http://192.168... blockeras Cast av webbläsaren. Öppna via http://127.0.0.1:18421 eller aktivera chrome://flags/#unsafely-treat-insecure-origin-as-secure.";
        } else if (typeof window !== "undefined" && !window.chrome) {
          this.lastError =
            "Google Cast stöds endast i Chromium-baserade webbläsare (Chrome, Edge, Brave med Cast).";
        } else if (
          typeof navigator !== "undefined" &&
          !(navigator as unknown as { presentation?: unknown }).presentation
        ) {
          this.lastError =
            "Webbläsarens Media Router / Presentation API är inaktiverad. Kontrollera startflaggor i webbläsaren.";
        } else {
          this.lastError =
            "Ingen Cast-mottagare kunde öppnas. Kontrollera att webbläsaren stöder Google Cast och att enheter finns på nätverket.";
        }
      }

      this.clearSession();
      return false;
    } finally {
      window.clearTimeout(diagnosticWait);
      this.requesting = false;
      if (this.mode === "google" && this.isCasting) this.observeMedia();
    }
  }

  public play() {
    castDiagnostics.record("control.play", {
      mode: this.mode,
      paused: this.remotePlayer?.isPaused,
      playerState: this.remotePlayer?.playerState,
    });
    if (this.mode === "remote") {
      void this.nativeVideo?.play().catch(() => {});
      return;
    }
    if (this.remotePlayerController) {
      if (this.remotePlayer?.isPaused || this.playerState !== "playing") {
        this.remotePlayerController.playOrPause();
      }
    }
  }

  public pause() {
    castDiagnostics.record("control.pause", {
      mode: this.mode,
      paused: this.remotePlayer?.isPaused,
    });
    if (this.mode === "remote") {
      this.nativeVideo?.pause();
      return;
    }
    if (this.remotePlayer && !this.remotePlayer.isPaused)
      this.remotePlayerController?.playOrPause();
  }

  public seek(seconds: number) {
    castDiagnostics.record("control.seek", { position: seconds, mode: this.mode });
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
    castDiagnostics.record("player.dispose");
    this.generation++;
    if (this.mode === "remote") {
      this.nativeVideo?.pause();
      this.clearSession();
    } else this.stopCasting();
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatInterval = window.setInterval(() => {
      castDiagnostics.record("receiver.heartbeat", {
        mode: this.mode,
        state: this.playerState,
        position: this.currentTime,
        sdkConnected: this.remotePlayer?.isConnected,
        mediaLoaded: this.remotePlayer?.isMediaLoaded,
      });
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

    // Report only bounded finite values; SDK unknown values are not API numbers.
    const milliseconds = (seconds: number) =>
      Math.round(
        Math.min(
          Number.MAX_SAFE_INTEGER,
          Math.max(0, Number.isFinite(seconds) ? seconds * 1000 : 0),
        ),
      );
    const body = {
      id: this.castSessionId,
      lastSeenAt: Date.now(),
      name: this.deviceName || "Remote display",
      deviceType: this.mode === "remote" ? "remote" : "chromecast",
      state: this.playerState,
      nowPlayingItemId: this.activeItemId,
      positionMs: milliseconds(this.currentTime),
      durationMs: milliseconds(this.duration),
      volume: Math.max(
        0,
        Math.min(100, Math.round(Number.isFinite(this.volume) ? this.volume * 100 : 100)),
      ),
    };
    this.registryQueue = this.registryQueue
      .then(async () => {
        const result = await client.POST("/api/v1/players", {
          body,
          signal: AbortSignal.timeout(5000),
        });
        if (result.response?.ok === false) {
          const failure = result.error as
            | { error?: { code?: unknown; message?: unknown }; message?: unknown }
            | undefined;
          castDiagnostics.record("registry.failed", {
            status: result.response.status,
            code: failure?.error?.code,
            message: failure?.error?.message ?? failure?.message,
          });
        }
        if (!result.error && this.castSessionId === body.id) this.connectCommands(body.id);
      })
      .catch((error) => castDiagnostics.error("registry.error", error));
  }

  private toAbsoluteUrl(url: string): string {
    if (url.startsWith("http://") || url.startsWith("https://")) {
      return url;
    }
    return new URL(url, window.location.origin).href;
  }
}

export const castManager = new CastManager();
