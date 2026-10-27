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
  contentType?: string; // e.g. "application/x-mpegurl" or "video/mp4"
  title?: string;
  subtitle?: string;
  imageUrl?: string;
  currentTime?: number;
  duration?: number;
  itemId?: number;
}

export type CastStateListener = (state: {
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
        media?: {
          DEFAULT_MEDIA_RECEIVER_APP_ID?: string;
          MediaInfo: new (
            contentId: string,
            contentType: string,
          ) => {
            metadata?: {
              title?: string;
              subtitle?: string;
              images?: Array<{ url: string }>;
            };
          };
          GenericMediaMetadata: new () => Record<string, unknown>;
          Image: new (url: string) => { url: string };
          LoadRequest: new (
            mediaInfo: unknown,
          ) => {
            autoplay?: boolean;
            currentTime?: number;
          };
        };
      };
    };
  }
}

class CastManager {
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

  constructor() {
    this.initCastApi();
  }

  public subscribe(listener: CastStateListener): () => void {
    this.listeners.add(listener);
    this.notify();
    return () => this.listeners.delete(listener);
  }

  public getState() {
    return {
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

      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.IS_CONNECTED_CHANGED,
        () => this.onConnectedChanged(),
      );
      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.PLAYER_STATE_CHANGED,
        () => this.onPlayerStateChanged(),
      );
      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.CURRENT_TIME_CHANGED,
        () => {
          this.currentTime = this.remotePlayer?.currentTime || 0;
          this.notify();
        },
      );
      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.DURATION_CHANGED,
        () => {
          this.duration = this.remotePlayer?.duration || 0;
          this.notify();
        },
      );
      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.VOLUME_LEVEL_CHANGED,
        () => {
          this.volume = this.remotePlayer?.volumeLevel || 1;
          this.notify();
        },
      );
      this.remotePlayerController.addEventListener(
        framework.RemotePlayerEventType.IS_MUTED_CHANGED,
        () => {
          this.isMuted = !!this.remotePlayer?.isMuted;
          this.notify();
        },
      );

      this.isAvailable = true;
      this.onConnectedChanged();
    } catch {
      // Framework init may fail if run in non-standard context
    }
  }

  private onConnectedChanged() {
    if (this.remotePlayer?.isConnected) {
      this.isCasting = true;
      this.deviceName = this.remotePlayer.displayName || "Google Cast";
      this.castSessionId =
        window.cast?.framework?.CastContext.getInstance().getCurrentSession()?.getSessionId() ||
        `cast-${Date.now()}`;
      this.startHeartbeat();
    } else {
      this.isCasting = false;
      this.deviceName = null;
      this.playerState = "idle";
      this.stopHeartbeat();
    }
    this.notify();
  }

  private onPlayerStateChanged() {
    if (!this.remotePlayer) return;
    const raw = (this.remotePlayer.playerState || "").toUpperCase();
    if (raw === "PLAYING") this.playerState = "playing";
    else if (raw === "PAUSED") this.playerState = "paused";
    else if (raw === "BUFFERING") this.playerState = "buffering";
    else this.playerState = "idle";
    this.notify();
    this.syncPlayerRegistry();
  }

  /**
   * Prompts user to connect to a Cast device and loads media.
   */
  public async requestCastSession(
    media: CastMediaOptions,
    videoElement?: HTMLVideoElement,
  ): Promise<boolean> {
    this.activeItemId = media.itemId ?? null;

    // 1. Try Google Cast Web SDK
    const framework = window.cast?.framework;
    const chromeCastMedia = window.chrome?.cast?.media;
    if (framework && chromeCastMedia) {
      try {
        const castContext = framework.CastContext.getInstance();
        await castContext.requestSession();
        const session = castContext.getCurrentSession();
        if (!session) return false;

        const url = this.toAbsoluteUrl(media.contentId);
        const contentType =
          media.contentType || (url.includes(".m3u8") ? "application/x-mpegurl" : "video/mp4");

        const mediaInfo = new chromeCastMedia.MediaInfo(url, contentType);
        mediaInfo.metadata = new chromeCastMedia.GenericMediaMetadata();
        if (mediaInfo.metadata) {
          mediaInfo.metadata.title = media.title || "MBL Video";
          mediaInfo.metadata.subtitle = media.subtitle || "";
          if (media.imageUrl) {
            mediaInfo.metadata.images = [
              new chromeCastMedia.Image(this.toAbsoluteUrl(media.imageUrl)),
            ];
          }
        }

        const request = new chromeCastMedia.LoadRequest(mediaInfo);
        request.autoplay = true;
        request.currentTime = media.currentTime || 0;

        await session.loadMedia(request);
        this.isCasting = true;
        this.deviceName = session.getCastDevice()?.friendlyName || "Google Cast";
        this.currentTime = media.currentTime || 0;
        this.playerState = "playing";
        this.notify();
        this.syncPlayerRegistry();
        return true;
      } catch (err: unknown) {
        if (err !== "cancel") {
          console.warn("Cast request failed, checking fallback:", err);
        }
      }
    }

    // 2. Fallback to HTMLMediaElement Remote Playback API
    const remoteEl = videoElement as HTMLMediaElementWithRemote | undefined;
    if (remoteEl?.remote) {
      try {
        await remoteEl.remote.prompt();
        this.isCasting = true;
        this.deviceName = "Wireless Display / AirPlay";
        this.notify();
        return true;
      } catch {
        return false;
      }
    }

    return false;
  }

  public play() {
    this.remotePlayerController?.playOrPause();
  }

  public pause() {
    this.remotePlayerController?.playOrPause();
  }

  public seek(seconds: number) {
    if (this.remotePlayer && this.remotePlayerController) {
      this.remotePlayer.currentTime = seconds;
      this.remotePlayerController.seek();
    }
  }

  public setVolume(level: number) {
    if (this.remotePlayer && this.remotePlayerController) {
      this.remotePlayer.volumeLevel = Math.max(0, Math.min(1, level));
      this.remotePlayerController.setVolumeLevel();
    }
  }

  public stopCasting() {
    try {
      window.cast?.framework?.CastContext.getInstance().endCurrentSession(true);
    } catch {
      // ignore
    }
    this.isCasting = false;
    this.deviceName = null;
    this.playerState = "idle";
    this.stopHeartbeat();
    this.notify();
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
    client
      .POST("/api/v1/players", {
        body: {
          id: this.castSessionId,
          lastSeenAt: Date.now(),
          name: this.deviceName || "Chromecast",
          deviceType: "chromecast",
          state: this.playerState,
          nowPlayingItemId: this.activeItemId,
          positionMs: Math.round(this.currentTime * 1000),
          durationMs: Math.round(this.duration * 1000),
          volume: Math.round(this.volume * 100),
        },
      })
      .catch(() => {
        // ignore network error for player reporting
      });
  }

  private toAbsoluteUrl(url: string): string {
    if (url.startsWith("http://") || url.startsWith("https://")) {
      return url;
    }
    return new URL(url, window.location.origin).href;
  }
}

export const castManager = new CastManager();
