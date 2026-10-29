import Hls from "hls.js";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { getPreferredProfile } from "../api/deviceProfile";
import { findNextEpisode } from "../api/nextEpisode";
import type { components } from "../api/schema";
import type { PlayRequest } from "../context/PlayerContext";
import { useTheme } from "../context/ThemeContext";
import { castManager } from "../utils/cast";
import { createHlsRecovery, hlsLoadPolicy } from "../utils/hlsRecovery";
import TrickplayPreview from "./TrickplayPreview";
import { Button } from "./ui";

type PlaybackResponse = components["schemas"]["PlaybackResponse"];
type PlaybackSubtitleTrack = components["schemas"]["PlaybackSubtitleTrack"];

function playbackFailureMessage(error: unknown, fallback: string): string {
  if (typeof error === "string") return error;
  if (error && typeof error === "object") {
    const problem = error as { error?: { message?: unknown }; detail?: unknown; message?: unknown };
    const message = problem.error?.message ?? problem.detail ?? problem.message;
    if (typeof message === "string") return message;
  }
  return fallback;
}

export interface PlayerProps {
  itemId: number;
  partId?: number;
  initialStartMs?: number;
  audioStreamIndex?: number;
  subtitleStreamIndex?: number | null;
  title?: string;
  subtitle?: string;
  onClose: () => void;
  autoplayNext?: boolean;
  onPlayNext?: (request: PlayRequest) => void;
}

export function Player(props: PlayerProps) {
  let videoRef: HTMLVideoElement | undefined;
  let playerContainerRef: HTMLDivElement | undefined;
  let hlsInstance: Hls | null = null;
  let hlsRecovery: ReturnType<typeof createHlsRecovery> | null = null;
  let localAutoplay = false;
  let lastAdvancingTime = 0;
  const [recoveryNotice, setRecoveryNotice] = createSignal("");
  const [retryingPlayback, setRetryingPlayback] = createSignal(false);
  const [failedPositionMs, setFailedPositionMs] = createSignal<number | null>(null);
  let progressInterval: number | null = null;
  let disposed = false;
  let stoppingSession: Promise<void> | null = null;
  let stoppedSession: string | null = null;
  let requestedStartMs = 0;
  let confirmedSessionId: string | null = null;
  let localBuffering = true;
  let reportingProgress = false;
  let progressAbort: AbortController | null = null;
  const [sessionExpired, setSessionExpired] = createSignal(false);
  const pendingStarts = new Set<AbortController>();
  let hideControlsTimer: number | null = null;

  // WebAudio nodes for Night Mode / Smart Dialogue Boost
  let audioCtx: AudioContext | null = null;
  let audioSource: MediaElementAudioSourceNode | null = null;
  let dialogueBoostFilter: BiquadFilterNode | null = null;
  let compressorNode: DynamicsCompressorNode | null = null;

  const { subtitleStyle, nightMode, setNightMode, audioOffsetMs, setAudioOffsetMs } = useTheme();
  const [audioSyncMs, setAudioSyncMs] = createSignal(
    Math.min(10000, Math.max(-10000, audioOffsetMs?.() ?? 0)),
  );
  const [previewTime, setPreviewTime] = createSignal<number | null>(null);
  const [selectedAudioIndex, setSelectedAudioIndex] = createSignal<number | undefined>(
    props.audioStreamIndex,
  );
  const [quality, setQuality] = createSignal(0);
  const [changingQuality, setChangingQuality] = createSignal(false);
  const [controlNotice, setControlNotice] = createSignal("");

  const [mediaItemId, setMediaItemId] = createSignal(props.itemId);
  const [mediaTitle, setMediaTitle] = createSignal(props.title);
  const [mediaPartId, setMediaPartId] = createSignal(props.partId);
  const [castStatus, setCastStatus] = createSignal<string | null>(null);
  const [nextEpisode, setNextEpisode] = createSignal<{ id: number; title: string } | null>(null);
  const [nextError, setNextError] = createSignal("");
  const [ended, setEnded] = createSignal(false);
  const [startingNext, setStartingNext] = createSignal(false);
  const [autoplayNext, setAutoplayNext] = createSignal(props.autoplayNext ?? false);
  const [receiverFinish, setReceiverFinish] = createSignal<{
    loadId: number;
    itemId: number | null;
  } | null>(null);
  const [receiverAutoplaySupported, setReceiverAutoplaySupported] = createSignal(false);
  let lastFinishedLoad = -1;
  let lastAutoplayLoad = -1;
  let nextVersion = 0;
  let nextAbort: AbortController | null = null;
  const loadNext = async (id: number) => {
    nextAbort?.abort();
    nextAbort = new AbortController();
    const version = ++nextVersion;
    setNextEpisode(null);
    setNextError("");
    try {
      const episode = await findNextEpisode(id, nextAbort.signal);
      if (!disposed && version === nextVersion) setNextEpisode(episode);
    } catch {
      if (!disposed && version === nextVersion) setNextError("Could not load next episode.");
    }
  };
  createEffect(() => {
    const id = mediaItemId();
    if (props.onPlayNext) {
      setEnded(false);
      void loadNext(id);
    }
  });
  const [playbackData, setPlaybackData] = createSignal<PlaybackResponse | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [error, setError] = createSignal<string | null>(null);
  const [isPlaying, setIsPlaying] = createSignal(false);
  const [currentTime, setCurrentTime] = createSignal(0);
  const [duration, setDuration] = createSignal(0);
  const [volume, setVolume] = createSignal(1);
  const [isMuted, setIsMuted] = createSignal(false);
  const [showControls, setShowControls] = createSignal(true);
  const [showResumePrompt, setShowResumePrompt] = createSignal(false);
  const [selectedSubtitleIndex, setSelectedSubtitleIndex] = createSignal<number>(-1);
  const [showSubtitlesMenu, setShowSubtitlesMenu] = createSignal(false);
  const [showSpeedMenu, setShowSpeedMenu] = createSignal(false);
  const [playbackRate, setPlaybackRate] = createSignal(1.0);
  const [showStats, setShowStats] = createSignal(false);
  const [droppedFrames, setDroppedFrames] = createSignal<number | null>(null);
  const [bufferAheadSecs, setBufferAheadSecs] = createSignal<number | null>(null);
  const [videoSize, setVideoSize] = createSignal("Not measured");
  const [diagnostics, setDiagnostics] = createSignal<
    components["schemas"]["PlaybackDiagnostics"] | null
  >(null);
  const [diagnosticsError, setDiagnosticsError] = createSignal("");

  // Chromecast & Remote Playback state (M12)
  const [isCasting, setIsCasting] = createSignal(false);
  const [castDevice, setCastDevice] = createSignal<string | null>(null);
  const [isCastAvailable, setIsCastAvailable] = createSignal(false);
  const [castConnecting, setCastConnecting] = createSignal(false);

  createEffect(() => {
    const finish = receiverFinish();
    const next = nextEpisode();
    const enabled = autoplayNext();
    const busy = castConnecting();
    if (!isCasting() || !receiverAutoplaySupported() || !finish || finish.itemId !== mediaItemId())
      return;
    setEnded(true);
    setShowControls(true);
    if (lastFinishedLoad !== finish.loadId) {
      lastFinishedLoad = finish.loadId;
      void stopPlayback();
    }
    if (enabled && next && !busy && lastAutoplayLoad !== finish.loadId) {
      lastAutoplayLoad = finish.loadId;
      queueMicrotask(() => {
        if (!disposed && isCasting() && autoplayNext() && mediaItemId() === finish.itemId)
          void playNextEpisode();
      });
    }
  });

  const collectLocalStats = () => {
    if (!videoRef || disposed || isCasting()) return;
    const quality = videoRef.getVideoPlaybackQuality?.();
    setDroppedFrames(quality?.droppedVideoFrames ?? null);
    setVideoSize(
      videoRef.videoWidth && videoRef.videoHeight
        ? `${videoRef.videoWidth}x${videoRef.videoHeight}`
        : "Not measured",
    );
    let end = videoRef.currentTime;
    for (let index = 0; index < videoRef.buffered.length; index++) {
      if (
        videoRef.buffered.start(index) <= videoRef.currentTime &&
        videoRef.buffered.end(index) >= videoRef.currentTime
      )
        end = videoRef.buffered.end(index);
    }
    setBufferAheadSecs(Math.max(0, Math.round(end - videoRef.currentTime)));
  };
  createEffect(() => {
    const id = playbackData()?.sessionId;
    const enabled = showStats();
    if (enabled && !isCasting()) collectLocalStats();
    setDiagnostics(null);
    setDiagnosticsError("");
    if (!enabled || !id) return;
    const abort = new AbortController();
    let closed = false,
      busy = false;
    const refresh = async () => {
      if (busy || closed) return;
      busy = true;
      try {
        const result = await client.GET("/api/v1/playback/{session}/diagnostics", {
          params: { path: { session: id } },
          signal: abort.signal,
        });
        if (closed) return;
        if (result.error || !result.data)
          throw new Error("Server diagnostics unavailable. Retrying while statistics are open.");
        setDiagnostics(result.data);
        setDiagnosticsError("");
      } catch (reason) {
        if (!closed) {
          setDiagnostics(null);
          setDiagnosticsError(
            reason instanceof Error ? reason.message : "Server diagnostics unavailable.",
          );
        }
      } finally {
        busy = false;
      }
    };
    void refresh();
    const timer = window.setInterval(() => {
      collectLocalStats();
      void refresh();
    }, 5000);
    onCleanup(() => {
      closed = true;
      abort.abort();
      window.clearInterval(timer);
    });
  });

  const formatTime = (secs: number) => {
    if (!Number.isFinite(secs) || secs < 0) return "00:00";
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    const s = Math.floor(secs % 60);
    if (h > 0) {
      return `${h}:${m < 10 ? `0${m}` : m}:${s < 10 ? `0${s}` : s}`;
    }
    return `${m < 10 ? `0${m}` : m}:${s < 10 ? `0${s}` : s}`;
  };

  const resetHideControlsTimer = () => {
    setShowControls(true);
    if (hideControlsTimer) clearTimeout(hideControlsTimer);
    hideControlsTimer = window.setTimeout(() => {
      if (isPlaying() && !showStats()) {
        setShowControls(false);
        setShowSubtitlesMenu(false);
        setShowSpeedMenu(false);
      }
    }, 3500);
  };

  // WebAudio setup for Night Mode
  const initWebAudio = () => {
    if (!videoRef || audioCtx) return;
    try {
      const AudioCtxClass =
        window.AudioContext ||
        (window as Window & { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!AudioCtxClass) return;
      audioCtx = new AudioCtxClass();

      // Center vocal boost peaking filter (~2 kHz, +6 dB)
      dialogueBoostFilter = audioCtx.createBiquadFilter();
      dialogueBoostFilter.type = "peaking";
      dialogueBoostFilter.frequency.value = 2000;
      dialogueBoostFilter.Q.value = 1.4;
      dialogueBoostFilter.gain.value = 6;

      // Dynamics compressor to tame sudden explosions and lift whisper dialogue
      compressorNode = audioCtx.createDynamicsCompressor();
      compressorNode.threshold.value = -24;
      compressorNode.knee.value = 30;
      compressorNode.ratio.value = 12;
      compressorNode.attack.value = 0.003;
      compressorNode.release.value = 0.25;

      audioSource = audioCtx.createMediaElementSource(videoRef);
      connectAudioNodes();
    } catch {
      if (!audioSource) {
        void audioCtx?.close().catch(() => {});
        audioCtx = null;
      }
      // Browser WebAudio policy or already connected
    }
  };

  const connectAudioNodes = () => {
    if (!audioCtx || !audioSource) return;
    audioSource.disconnect();
    if (dialogueBoostFilter) dialogueBoostFilter.disconnect();
    if (compressorNode) compressorNode.disconnect();
    const output = audioCtx.destination;

    if (nightMode() && dialogueBoostFilter && compressorNode) {
      audioSource.connect(dialogueBoostFilter);
      dialogueBoostFilter.connect(compressorNode);
      compressorNode.connect(output);
    } else {
      audioSource.connect(output);
    }
  };

  const toggleNightMode = () => {
    if (isCasting()) return;
    initWebAudio();
    if (audioCtx && audioCtx.state === "suspended") {
      audioCtx.resume();
    }
    const next = !nightMode();
    setNightMode(next);
    connectAudioNodes();
  };

  const negotiatePlayback = async (body: components["schemas"]["PlaybackRequest"]) => {
    const abort = new AbortController();
    pendingStarts.add(abort);
    try {
      return await client.POST("/api/v1/playback", {
        body: { ...body, waitForReady: true },
        signal: abort.signal,
      });
    } finally {
      pendingStarts.delete(abort);
    }
  };

  const changeAudioSync = async (milliseconds: number) => {
    if (!Number.isInteger(milliseconds) || Math.abs(milliseconds) > 10000) return;
    if (isCasting()) {
      if (!receiverAutoplaySupported()) return;
      const success = await startReceiverPlayback(
        mediaItemId(),
        currentTime() * 1000,
        isPlaying(),
        selectedAudioIndex(),
        selectedSubtitleIndex() >= 0 ? selectedSubtitleIndex() : null,
        milliseconds,
        true,
      );
      if (success) setAudioOffsetMs?.(audioSyncMs());
    } else {
      await changeQuality(quality(), milliseconds);
    }
  };

  const changeQuality = async (
    maxBitrate: number,
    offset = audioSyncMs(),
    audioIndex = selectedAudioIndex(),
    subtitle = selectedSubtitleIndex(),
    retryPositionMs?: number,
  ) => {
    const previous = playbackData();
    if (!previous || loading() || changingQuality() || isCasting() || castConnecting())
      return false;
    setChangingQuality(true);
    setControlNotice("");
    const position =
      retryPositionMs ??
      failedPositionMs() ??
      Math.floor((videoRef?.currentTime ?? currentTime()) * 1000);
    try {
      if (failedPositionMs() !== null) {
        await stopPlayback();
        if (disposed) return false;
      }
      const result = await negotiatePlayback({
        itemId: mediaItemId(),
        partId: mediaPartId(),
        profile: getPreferredProfile() as unknown as Record<string, never>,
        startMs: position,
        maxBitrate: maxBitrate || undefined,
        audioOffsetMs: offset,
        audioStreamIndex: audioIndex,
        subtitleStreamIndex: subtitle >= 0 ? subtitle : undefined,
      });
      if (disposed) {
        if (result.data) void stopSession(result.data.sessionId, result.data.startMs);
        return false;
      }
      if (result.error || !result.data)
        throw new Error(
          `${playbackFailureMessage(result.error, "Playback adjustment failed.")} Previous settings are still selected.`,
        );
      const commitPosition =
        retryPositionMs ??
        failedPositionMs() ??
        Math.floor((videoRef?.currentTime ?? currentTime()) * 1000);
      setError(null);
      setPlaybackData(result.data);
      setMediaPartId(result.data.partId);
      setQuality(maxBitrate);
      setSelectedAudioIndex(
        result.data.audioStreamIndex !== undefined
          ? (result.data.audioStreamIndex ?? undefined)
          : audioIndex,
      );
      setAudioSyncMs(result.data.audioOffsetMs ?? offset);
      setAudioOffsetMs?.(result.data.audioOffsetMs ?? offset);
      const previousReleased = stoppedSession === previous.sessionId;
      const previousStop = previousReleased ? stoppingSession : null;
      initPlayer(result.data, commitPosition, isPlaying(), subtitle < 0 ? null : subtitle);
      setSelectedSubtitleIndex(subtitle);
      if (previousReleased) await previousStop;
      else await stopSession(previous.sessionId, commitPosition);
      return true;
    } catch (reason) {
      if (!disposed)
        setControlNotice(reason instanceof Error ? reason.message : "Playback adjustment failed.");
      return false;
    } finally {
      if (!disposed) setChangingQuality(false);
    }
  };

  const failLocalPlayback = (message: string, expired = false) => {
    if (disposed || ended() || error()) return;
    const seconds = videoRef?.currentTime;
    setFailedPositionMs(
      Math.floor(
        (typeof seconds === "number" && Number.isFinite(seconds) && seconds > 0
          ? seconds
          : Math.max(currentTime(), lastAdvancingTime)) * 1000,
      ),
    );
    setSessionExpired(expired);
    hlsRecovery?.dispose();
    hlsInstance?.stopLoad();
    setError(message);
    setLoading(false);
    localAutoplay = false;
    videoRef?.pause();
    setIsPlaying(false);
    // Release failed transcoding capacity before a manual replacement, including on Pi's one-slot limit.
    void stopPlayback();
  };

  const expirePlayback = () => {
    if (disposed || ended() || sessionExpired()) return;
    if (!isCasting()) {
      failLocalPlayback(
        "Playback session expired. Resume to prepare a new stream at this position.",
        true,
      );
    } else {
      setSessionExpired(true);
      setCastStatus("Playback session expired. Disconnect and reconnect the receiver to resume.");
    }
  };

  const localProgressState = (): "playing" | "paused" | "buffering" => {
    if (confirmedSessionId !== playbackData()?.sessionId) return "buffering";
    return !isPlaying() ? "paused" : localBuffering ? "buffering" : "playing";
  };

  const reportProgress = async (state: "playing" | "paused" | "buffering") => {
    const data = playbackData();
    const vid = videoRef;
    if (!data || !vid || disposed || ended() || error() || sessionExpired() || reportingProgress)
      return;

    const positionMs = Math.floor((isCasting() ? currentTime() : vid.currentTime) * 1000);
    reportingProgress = true;
    const abort = new AbortController();
    progressAbort = abort;
    const timeout = window.setTimeout(() => abort.abort(), 10000);
    abort.signal.addEventListener("abort", () => window.clearTimeout(timeout), { once: true });
    try {
      const result = await client.POST("/api/v1/playback/{session}/progress", {
        signal: abort.signal,
        params: { path: { session: data.sessionId } },
        body: { positionMs, state },
      });
      if (
        !disposed &&
        playbackData()?.sessionId === data.sessionId &&
        (result.response.status === 404 || result.response.status === 410)
      )
        expirePlayback();
    } catch {
    } finally {
      window.clearTimeout(timeout);
      if (progressAbort === abort) progressAbort = null;
      reportingProgress = false;
    }
  };

  const heartbeat = () => {
    if (disposed || ended() || error() || sessionExpired()) return;
    if (isCasting()) {
      const state = castManager.getState().playerState;
      void reportProgress(
        state === "playing" ? "playing" : state === "buffering" ? "buffering" : "paused",
      );
    } else {
      void reportProgress(localProgressState());
      if (isPlaying()) collectLocalStats();
    }
  };

  const stopSession = async (sessionId: string, positionMs: number) => {
    const abort = new AbortController();
    const timeout = window.setTimeout(() => abort.abort(), 10000);
    try {
      await client.POST("/api/v1/playback/{session}/stop", {
        signal: abort.signal,
        params: { path: { session: sessionId } },
        body: { positionMs },
      });
    } catch {
    } finally {
      window.clearTimeout(timeout);
    }
  };

  const stopPlayback = async () => {
    const data = playbackData();
    if (!data) return;
    if (stoppedSession === data.sessionId) {
      await stoppingSession;
      return;
    }
    stoppedSession = data.sessionId;
    stoppingSession = stopSession(
      data.sessionId,
      failedPositionMs() ??
        Math.floor((isCasting() ? currentTime() : (videoRef?.currentTime ?? 0)) * 1000),
    );
    await stoppingSession;
  };

  const retryPlayback = async () => {
    if (retryingPlayback() || changingQuality() || loading() || isCasting() || castConnecting())
      return;
    setRetryingPlayback(true);
    try {
      const data = playbackData();
      if (!data) {
        await startPlaybackSession(requestedStartMs);
        return;
      }
      const position = failedPositionMs() ?? Math.floor(currentTime() * 1000);
      await stopPlayback();
      if (disposed || playbackData()?.sessionId !== data.sessionId) return;
      if (
        await changeQuality(
          quality(),
          audioSyncMs(),
          selectedAudioIndex(),
          selectedSubtitleIndex(),
          position,
        )
      ) {
        if (!error()) setControlNotice("Stream prepared. Press Play to continue.");
      }
    } finally {
      if (!disposed) setRetryingPlayback(false);
    }
  };

  const startPlaybackSession = async (startMs = 0, offset = audioSyncMs()) => {
    requestedStartMs = startMs;
    setLoading(true);
    setError(null);

    try {
      const { data, error: err } = await negotiatePlayback({
        itemId: mediaItemId(),
        partId: mediaPartId(),
        profile: getPreferredProfile() as unknown as Record<string, never>,
        startMs,
        audioOffsetMs: offset,
        audioStreamIndex: selectedAudioIndex(),
        subtitleStreamIndex: props.subtitleStreamIndex ?? undefined,
      });

      if (disposed) {
        if (data) void stopSession(data.sessionId, data.startMs);
        return false;
      }
      if (err || !data) {
        setError(playbackFailureMessage(err, "Failed to start playback session"));
        setLoading(false);
        return false;
      }

      setPlaybackData(data as PlaybackResponse);
      setMediaPartId(data.partId);
      setAudioSyncMs(data.audioOffsetMs ?? offset);
      setSelectedAudioIndex(
        data.audioStreamIndex !== undefined
          ? (data.audioStreamIndex ?? undefined)
          : selectedAudioIndex(),
      );
      initPlayer(data as PlaybackResponse, data.startMs, true, props.subtitleStreamIndex);
      return true;
    } catch (e) {
      if (disposed) return false;
      setError(e instanceof Error ? e.message : "Playback initiation failed");
      setLoading(false);
      return false;
    }
  };

  const initPlayer = (
    data: PlaybackResponse,
    startMs: number,
    autoplay = true,
    subtitleSelection?: number | null,
  ) => {
    const vid = videoRef;
    if (!vid) return;

    hlsRecovery?.dispose();
    hlsRecovery = null;
    if (hlsInstance) {
      hlsInstance.destroy();
      hlsInstance = null;
    }

    confirmedSessionId = null;
    localBuffering = true;
    localAutoplay = autoplay;
    lastAdvancingTime = startMs / 1000;
    setFailedPositionMs(null);
    setSessionExpired(false);
    const streamUrl = data.url;
    const isHls = new URL(streamUrl, window.location.href).pathname.endsWith(".m3u8");

    if (isHls && Hls.isSupported()) {
      const hls = new Hls({
        enableWorker: true,
        startPosition: startMs > 0 ? startMs / 1000 : -1,
        manifestLoadPolicy: hlsLoadPolicy(),
        playlistLoadPolicy: hlsLoadPolicy(),
        fragLoadPolicy: hlsLoadPolicy(),
      });
      const active = () =>
        !disposed &&
        hlsInstance === hls &&
        playbackData()?.sessionId === data.sessionId &&
        !isCasting();
      const recovery = createHlsRecovery({
        active,
        stop: () => hls.stopLoad(),
        notice: setRecoveryNotice,
        retry: (reloadManifest) => {
          const position = Math.max(
            0,
            vid.readyState > 0 ? vid.currentTime : currentTime() || startMs / 1000,
          );
          // PlaylistLoader.startLoad is a no-op: a failed master needs loadSource again.
          if (reloadManifest) {
            hls.config.startPosition = position;
            hls.loadSource(streamUrl);
          }
          if (active() && !error()) hls.startLoad(position);
        },
        fail: (reason) => {
          if (reason === "expired") return expirePlayback();
          const message =
            reason === "access"
              ? "Access to this stream was denied. Sign in again or retry playback."
              : reason === "media"
                ? "The stream could not be decoded. Retry playback or choose another quality."
                : reason === "unsupported"
                  ? "This stream could not be played. Retry playback or choose another quality."
                  : "The stream could not be loaded. Check the connection and retry playback.";
          failLocalPlayback(message);
        },
      });
      hlsInstance = hls;
      hlsRecovery = recovery;
      // Register before loading: loaders and attach validation may emit errors synchronously.
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        if (!active() || error()) return;
        setLoading(false);
        if (localAutoplay && !castConnecting()) vid.play().catch(() => {});
      });
      hls.on(Hls.Events.ERROR, (_event, data) => recovery.error(data));
      hls.on(Hls.Events.FRAG_BUFFERED, () => {
        if (active()) recovery.progress();
      });
      hls.loadSource(streamUrl);
      if (!error()) hls.attachMedia(vid);
    } else {
      vid.src = streamUrl;
      if (startMs > 0) {
        vid.currentTime = startMs / 1000;
      }
      setLoading(false);
      if (autoplay) vid.play().catch(() => {});
    }

    const selectedSub = data.burnedSubtitleStreamIndex ?? subtitleSelection;
    const defaultSub = data.subtitles.find((s) => s.isDefault);
    setSelectedSubtitleIndex(selectedSub === null ? -1 : (selectedSub ?? defaultSub?.index ?? -1));
  };

  onMount(() => {
    const initialStart = props.initialStartMs ?? 0;
    if (initialStart > 10000) {
      setShowResumePrompt(true);
    } else {
      startPlaybackSession(initialStart);
    }

    progressInterval = window.setInterval(heartbeat, 5000);
    const onVisible = () => {
      if (document.visibilityState === "visible") heartbeat();
    };
    document.addEventListener("visibilitychange", onVisible);

    const handleKeyDown = (e: KeyboardEvent) => {
      const vid = videoRef;
      if (!vid) return;
      if (
        e.key !== "Escape" &&
        e.target instanceof HTMLElement &&
        e.target.closest("input, select, textarea, button, a, [contenteditable='true']")
      )
        return;

      resetHideControlsTimer();

      switch (e.key) {
        case " ":
        case "k":
          e.preventDefault();
          togglePlay();
          break;
        case "ArrowLeft":
        case "j":
          e.preventDefault();
          seekTo(Math.max(0, currentTime() - 10));
          break;
        case "ArrowRight":
        case "l":
          e.preventDefault();
          seekTo(Math.min(duration() || 0, currentTime() + 10));
          break;
        case "ArrowUp":
          e.preventDefault();
          setPlaybackVolume(Math.min(1, volume() + 0.1));
          break;
        case "ArrowDown":
          e.preventDefault();
          setPlaybackVolume(Math.max(0, volume() - 0.1));
          break;
        case "m":
          e.preventDefault();
          toggleMute();
          break;
        case "f":
          e.preventDefault();
          toggleFullscreen();
          break;
        case "s":
          e.preventDefault();
          setShowStats((prev) => !prev);
          break;
        case "n":
          e.preventDefault();
          toggleNightMode();
          break;
        case "Escape":
          e.preventDefault();
          handleClose();
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    // Subscribe to CastManager updates (M12)
    const unsubscribeCommands = castManager.setCommandHandler(async (command) => {
      if (command.action === "play" && command.params?.itemId) {
        await startReceiverPlayback(command.params.itemId, command.params.startMs ?? 0, true);
      } else if (command.action === "setTracks") {
        await startReceiverPlayback(
          mediaItemId(),
          currentTime() * 1000,
          isPlaying(),
          command.params?.audioStreamIndex ?? selectedAudioIndex(),
          command.params?.subtitleStreamIndex !== undefined
            ? command.params.subtitleStreamIndex
            : selectedSubtitleIndex() < 0
              ? null
              : selectedSubtitleIndex(),
          audioSyncMs(),
          true,
        );
      } else if (command.action === "stop") {
        await stopPlayback();
        castManager.disposeSession();
        videoRef?.pause();
        setIsPlaying(false);
        setError(command.params?.message || "Playback stopped remotely");
      }
    });
    const unsubCast = castManager.subscribe((cState) => {
      const wasCasting = isCasting();
      const lastPosition = currentTime();
      setIsCastAvailable(cState.isAvailable);
      setIsCasting(cState.isCasting);
      setReceiverAutoplaySupported(cState.supportsAutoplayNext === true);
      if (!cState.isCasting) setReceiverFinish(null);
      if (wasCasting && !cState.isCasting && !disposed && videoRef) {
        const data = playbackData();
        if (data)
          initPlayer(
            data,
            lastPosition * 1000,
            false,
            selectedSubtitleIndex() < 0 ? null : selectedSubtitleIndex(),
          );
        setIsPlaying(false);
        setVolume(videoRef.volume);
        setIsMuted(videoRef.muted);
      }
      setCastDevice(cState.deviceName);
      if (cState.isCasting) {
        if (!wasCasting) {
          hlsRecovery?.dispose();
          hlsInstance?.stopLoad();
        }
        setIsPlaying(cState.playerState === "playing");
        setCurrentTime(cState.currentTime);
        if (cState.duration > 0) setDuration(cState.duration);
        setVolume(cState.volume);
        setIsMuted(cState.isMuted);
        if (cState.playbackEnded)
          setReceiverFinish({ loadId: cState.mediaLoadId, itemId: cState.itemId });
      }
    });

    onCleanup(() => {
      disposed = true;
      progressAbort?.abort();
      hlsRecovery?.dispose();
      hlsRecovery = null;
      for (const request of pendingStarts) request.abort();
      pendingStarts.clear();
      nextAbort?.abort();
      unsubscribeCommands();
      unsubCast();
      window.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("visibilitychange", onVisible);
      if (progressInterval) clearInterval(progressInterval);
      if (hideControlsTimer) clearTimeout(hideControlsTimer);
      void stopPlayback();
      castManager.disposeSession();
      if (hlsInstance) {
        hlsInstance.destroy();
        hlsInstance = null;
      }
      if (audioCtx) {
        audioCtx.close().catch(() => {});
      }
    });
  });

  const startReceiverPlayback = async (
    itemId: number,
    startMs: number,
    autoplay: boolean,
    audioIndex = itemId === mediaItemId() ? selectedAudioIndex() : undefined,
    subtitleIndex: number | null | undefined = itemId === mediaItemId()
      ? selectedSubtitleIndex() < 0
        ? null
        : selectedSubtitleIndex()
      : undefined,
    offset = audioSyncMs(),
    preservePosition = false,
  ) => {
    if (castConnecting() || changingQuality()) return false;
    if (isCasting() && !receiverAutoplaySupported()) {
      setCastStatus(
        "This receiver uses the current source. Disconnect before changing media or audio tracks.",
      );
      return false;
    }
    const previous = playbackData();
    if (!previous) return false;
    setShowSpeedMenu(false);
    setShowSubtitlesMenu(false);
    setCastConnecting(true);
    setCastStatus(null);
    let oldPosition = currentTime();
    let negotiatedStartMs = startMs;
    const preserveReceiverState =
      isCasting() && itemId === mediaItemId() && autoplay === isPlaying();
    let negotiated: PlaybackResponse | null = null;
    let negotiatedTitle = itemId === mediaItemId() ? mediaTitle() || "MBL" : "MBL";
    videoRef?.pause();
    try {
      const success = await castManager.requestCastSession(
        {
          contentId: previous.url,
          title: mediaTitle(),
          subtitle: itemId === props.itemId ? props.subtitle : undefined,
          currentTime: startMs / 1000,
          duration: duration(),
          itemId,
          autoplay,
        },
        videoRef,
        async () => {
          const result = await negotiatePlayback({
            itemId,
            partId: itemId === mediaItemId() ? mediaPartId() : undefined,
            profile: "chromecast" as unknown as Record<string, never>,
            startMs,
            audioOffsetMs: offset,
            audioStreamIndex: audioIndex,
            subtitleStreamIndex: subtitleIndex ?? undefined,
          });
          if (result.error || !result.data)
            throw new Error(
              playbackFailureMessage(
                result.error,
                "Could not negotiate playback for the Cast receiver.",
              ),
            );
          negotiated = result.data;
          try {
            const detail = await client.GET("/api/v1/items/{id}", {
              params: { path: { id: itemId } },
            });
            negotiatedTitle = detail.data?.title || negotiatedTitle;
          } catch {
            /* Optional title lookup must not reject valid receiver playback. */
          }
          oldPosition = isCasting() ? currentTime() : (videoRef?.currentTime ?? currentTime());
          negotiatedStartMs = preservePosition
            ? Math.floor(oldPosition * 1000)
            : result.data.startMs;
          const title = negotiatedTitle;
          return {
            contentId: result.data.url,
            contentType: result.data.contentType,
            title,
            currentTime: negotiatedStartMs / 1000,
            duration: result.data.durationMs / 1000,
            itemId,
            autoplay: preserveReceiverState ? isPlaying() : autoplay,
            subtitles: result.data.subtitles,
            subtitleIndex: result.data.burnedSubtitleStreamIndex != null ? null : subtitleIndex,
          };
        },
      );
      // Native Remote Playback uses the browser's existing source; Google Cast uses
      // the server-negotiated receiver profile, MIME and externally reachable URLs.
      const accepted = negotiated as PlaybackResponse | null;
      if (success && accepted && !disposed) {
        if (stoppedSession === previous.sessionId) await stoppingSession;
        else await stopSession(previous.sessionId, Math.floor(oldPosition * 1000));
        setPlaybackData(accepted);
        setFailedPositionMs(null);
        setSessionExpired(false);
        setError(null);
        setQuality(0);
        setAudioSyncMs(accepted.audioOffsetMs ?? offset);
        setMediaItemId(itemId);
        setMediaPartId(accepted.partId);
        setSelectedAudioIndex(
          accepted.audioStreamIndex !== undefined
            ? (accepted.audioStreamIndex ?? undefined)
            : audioIndex,
        );
        setCurrentTime(negotiatedStartMs / 1000);
        setDuration(accepted.durationMs / 1000);
        setSelectedSubtitleIndex(
          accepted.burnedSubtitleStreamIndex ??
            (subtitleIndex === null
              ? -1
              : (subtitleIndex ?? accepted.subtitles.find((s) => s.isDefault)?.index ?? -1)),
        );
        setMediaTitle(negotiatedTitle);
        void reportProgress(isPlaying() ? "playing" : "paused");
      } else if (accepted) {
        await stopSession(accepted.sessionId, accepted.startMs);
      }
      if (!success) {
        setCastStatus(castManager.getLastError());
        if (autoplay && !disposed && !isCasting()) void videoRef?.play().catch(() => {});
      }
      return success && !!accepted && !disposed;
    } finally {
      if (!disposed) setCastConnecting(false);
    }
  };

  const handleCast = async () => {
    if (isCasting()) {
      castManager.stopCasting();
      return;
    }
    await startReceiverPlayback(
      mediaItemId(),
      currentTime() * 1000,
      isPlaying(),
      selectedAudioIndex(),
      selectedSubtitleIndex() < 0 ? null : selectedSubtitleIndex(),
      audioSyncMs(),
      true,
    );
  };

  const playNextEpisode = async () => {
    const next = nextEpisode();
    if (
      !next ||
      startingNext() ||
      castConnecting() ||
      changingQuality() ||
      (isCasting() && !receiverAutoplaySupported())
    )
      return;
    setStartingNext(true);
    setControlNotice("");
    try {
      if (isCasting()) {
        if (ended()) await stopPlayback();
        if (!disposed) await startReceiverPlayback(next.id, 0, true);
      } else if (props.onPlayNext) {
        await stopPlayback();
        if (!disposed)
          props.onPlayNext({
            itemId: next.id,
            title: next.title,
            initialStartMs: 0,
            autoplayNext: autoplayNext(),
          });
      }
    } catch {
      if (!disposed) setControlNotice("Could not start next episode. Try again.");
    } finally {
      if (!disposed) setStartingNext(false);
    }
  };

  const togglePlay = () => {
    if (isCasting()) {
      if (isPlaying()) {
        castManager.pause();
      } else {
        castManager.play();
      }
      return;
    }
    const vid = videoRef;
    if (!vid || error()) return;
    localAutoplay = vid.paused;
    if (vid.paused) {
      void vid.play().catch(() => {});
    } else {
      vid.pause();
    }
  };

  const toggleMute = () => {
    if (isCasting()) {
      castManager.toggleMute();
      return;
    }
    const vid = videoRef;
    if (!vid) return;
    vid.muted = !vid.muted;
    setIsMuted(vid.muted);
  };

  const toggleFullscreen = () => {
    const container = playerContainerRef;
    if (!container) return;

    if (!document.fullscreenElement) {
      container.requestFullscreen().catch(() => {});
    } else {
      document.exitFullscreen().catch(() => {});
    }
  };

  const seekTo = (targetTime: number) => {
    setCurrentTime(targetTime);
    if (isCasting()) {
      castManager.seek(targetTime);
      return;
    }
    lastAdvancingTime = targetTime;
    if (videoRef) videoRef.currentTime = targetTime;
  };
  const handleSeek = (e: Event & { currentTarget: HTMLInputElement }) =>
    seekTo(Number(e.currentTarget.value));

  const setPlaybackVolume = (v: number) => {
    setVolume(v);
    if (isCasting()) {
      castManager.setVolume(v);
      return;
    }
    if (videoRef) {
      videoRef.volume = v;
      videoRef.muted = false;
    }
    setIsMuted(false);
  };
  const handleVolumeChange = (e: Event & { currentTarget: HTMLInputElement }) =>
    setPlaybackVolume(Number(e.currentTarget.value));

  const handleSetSpeed = (rate: number) => {
    if (videoRef) {
      videoRef.playbackRate = rate;
      setPlaybackRate(rate);
    }
    setShowSpeedMenu(false);
  };

  const handleClose = () => {
    props.onClose();
  };

  const subtitleChoices = () =>
    playbackData()?.subtitleOptions ??
    (playbackData()?.subtitles ?? []).map((sub) => ({ ...sub, requiresTranscode: false }));
  const changeAudioTrack = async (index: number) => {
    if (index === selectedAudioIndex()) return;
    if (isCasting()) {
      if (!receiverAutoplaySupported()) return;
      await startReceiverPlayback(
        mediaItemId(),
        currentTime() * 1000,
        isPlaying(),
        index,
        selectedSubtitleIndex() < 0 ? null : selectedSubtitleIndex(),
        audioSyncMs(),
        true,
      );
    } else {
      await changeQuality(quality(), audioSyncMs(), index);
    }
  };
  const selectSubtitle = async (index: number) => {
    if (loading() || changingQuality() || castConnecting()) return;
    const requiresStream =
      playbackData()?.burnedSubtitleStreamIndex != null ||
      subtitleChoices().find((s) => s.index === index)?.requiresTranscode;
    if (requiresStream) {
      if (isCasting()) {
        if (!receiverAutoplaySupported()) {
          setControlNotice("This receiver cannot renegotiate image subtitles.");
          return;
        }
        if (
          !(await startReceiverPlayback(
            mediaItemId(),
            currentTime() * 1000,
            isPlaying(),
            selectedAudioIndex(),
            index < 0 ? null : index,
            audioSyncMs(),
            true,
          ))
        )
          return;
      } else if (!(await changeQuality(quality(), audioSyncMs(), selectedAudioIndex(), index)))
        return;
    } else {
      if (isCasting() && !(await castManager.selectSubtitle(index < 0 ? null : index))) {
        setCastStatus("The receiver could not change subtitles.");
        return;
      }
      setSelectedSubtitleIndex(index);
    }
    setShowSubtitlesMenu(false);
  };

  const activeSubtitleTrack = (): PlaybackSubtitleTrack | undefined => {
    const subs = playbackData()?.subtitles || [];
    return subs.find((s) => s.index === selectedSubtitleIndex());
  };

  // Markers: intro, credits, chapters
  const currentMarkers = () => playbackData()?.markers || [];
  const activeIntroMarker = () => {
    const curMs = currentTime() * 1000;
    return currentMarkers().find(
      (m) => m.kind === "intro" && curMs >= m.startMs && curMs <= m.endMs,
    );
  };
  const activeCreditsMarker = () => {
    const curMs = currentTime() * 1000;
    return currentMarkers().find(
      (m) => m.kind === "credits" && curMs >= m.startMs && curMs <= m.endMs,
    );
  };

  // Subtitle cue styling CSS injection
  const subCueStyle = () => {
    const s = subtitleStyle();
    const fontSize =
      s.size === "sm" ? "14px" : s.size === "lg" ? "24px" : s.size === "xl" ? "32px" : "18px";
    const bg =
      s.background === "none"
        ? "transparent"
        : s.background === "solid"
          ? "#000000"
          : "rgba(0, 0, 0, 0.75)";
    const color = s.color === "yellow" ? "#facc15" : s.color === "cyan" ? "#06b6d4" : "#ffffff";
    return `
      video::cue {
        font-size: ${fontSize};
        background-color: ${bg};
        color: ${color};
        text-shadow: 0 2px 4px rgba(0, 0, 0, 0.8);
      }
    `;
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: mouse movement resets auto-hide controls timer
    <div
      ref={playerContainerRef}
      onMouseMove={resetHideControlsTimer}
      style={{
        position: "fixed",
        top: 0,
        left: 0,
        width: "100vw",
        height: "100vh",
        background: "#000",
        "z-index": 9999,
        display: "flex",
        "align-items": "center",
        "justify-content": "center",
        cursor: showControls() ? "default" : "none",
        "user-select": "none",
      }}
    >
      <style>{subCueStyle()}</style>

      {/* Resume Prompt Modal */}
      <Show when={showResumePrompt()}>
        <div
          style={{
            position: "absolute",
            background: "rgba(20, 20, 20, 0.95)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-lg)",
            padding: "28px 36px",
            "text-align": "center",
            "z-index": 10001,
            "max-width": "420px",
            "box-shadow": "var(--shadow-xl)",
          }}
        >
          <h3 style={{ margin: "0 0 10px", "font-size": "20px", "font-weight": "700" }}>
            Resume Playback?
          </h3>
          <p style={{ margin: "0 0 24px", color: "var(--text-secondary)", "font-size": "14px" }}>
            You previously stopped watching at{" "}
            <strong>{formatTime((props.initialStartMs ?? 0) / 1000)}</strong>.
          </p>
          <div style={{ display: "flex", gap: "12px", "justify-content": "center" }}>
            <Button
              variant="primary"
              size="md"
              onClick={() => {
                setShowResumePrompt(false);
                startPlaybackSession(props.initialStartMs ?? 0);
              }}
            >
              Resume ({formatTime((props.initialStartMs ?? 0) / 1000)})
            </Button>
            <Button
              variant="secondary"
              size="md"
              onClick={() => {
                setShowResumePrompt(false);
                startPlaybackSession(0);
              }}
            >
              Start Over
            </Button>
          </div>
        </div>
      </Show>

      {/* Loading Overlay */}
      <Show when={loading()}>
        <div
          style={{
            position: "absolute",
            inset: "0",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            background: "rgba(0, 0, 0, 0.5)",
            "z-index": "15",
            "pointer-events": "none",
          }}
        >
          <div
            style={{
              width: "48px",
              height: "48px",
              border: "4px solid var(--accent)",
              "border-right-color": "transparent",
              "border-radius": "var(--radius-full)",
              animation: "spin 0.8s linear infinite",
            }}
          />
        </div>
      </Show>

      {/* Error Overlay */}
      <Show when={error()}>
        {(err) => (
          <div
            style={{
              position: "absolute",
              inset: "0",
              display: "flex",
              "flex-direction": "column",
              "align-items": "center",
              "justify-content": "center",
              background: "rgba(0, 0, 0, 0.85)",
              "z-index": "25",
              padding: "var(--space-6)",
              "text-align": "center",
            }}
          >
            <p
              role="alert"
              style={{
                color: "var(--danger)",
                "font-size": "var(--font-size-lg)",
                "margin-bottom": "var(--space-4)",
              }}
            >
              {err()}
            </p>
            <Show when={!isCasting()}>
              <Button
                variant="secondary"
                disabled={retryingPlayback() || changingQuality() || loading()}
                aria-busy={retryingPlayback()}
                onClick={retryPlayback}
              >
                {retryingPlayback()
                  ? "Preparing playback…"
                  : sessionExpired()
                    ? "Resume playback"
                    : "Retry playback"}
              </Button>
            </Show>
            <Show when={!playbackData() && audioSyncMs() !== 0}>
              <Button
                variant="secondary"
                onClick={async () => {
                  if (await startPlaybackSession(requestedStartMs, 0)) setAudioOffsetMs?.(0);
                }}
              >
                Retry without audio sync
              </Button>
            </Show>
            <Button variant="secondary" onClick={props.onClose}>
              Close
            </Button>
          </div>
        )}
      </Show>

      <Show when={castStatus()}>
        <div
          role="alert"
          style={{
            position: "absolute",
            top: "70px",
            padding: "12px",
            background: "var(--bg-surface)",
            "z-index": 30,
          }}
        >
          {castStatus()}
        </div>
      </Show>
      <Show when={retryingPlayback() || changingQuality() || castConnecting()}>
        <p
          role="status"
          style={{
            position: "absolute",
            top: "100px",
            left: "30px",
            "z-index": 10010,
            background: "#222",
            color: "white",
            padding: "12px",
          }}
        >
          Preparing playback…
        </p>
      </Show>
      <Show when={recoveryNotice()}>
        <p
          role="status"
          style={{
            position: "absolute",
            top: "145px",
            left: "30px",
            "z-index": 10010,
            background: "#222",
            padding: "12px",
          }}
        >
          {recoveryNotice()}
        </p>
      </Show>
      <Show when={controlNotice()}>
        <p
          role="alert"
          style={{
            position: "absolute",
            top: "100px",
            left: "30px",
            "z-index": 10010,
            background: "#222",
            color: "white",
            padding: "12px",
          }}
        >
          {controlNotice()}
        </p>
      </Show>
      {/* Main Video Element */}
      {/* biome-ignore lint/a11y/useMediaCaption: dynamic subtitle tracks loaded conditionally */}
      <video
        ref={videoRef}
        playsinline
        crossorigin="anonymous"
        style={{
          width: "100%",
          height: "100%",
          "object-fit": "contain",
        }}
        onTimeUpdate={(e) => {
          if (!isCasting()) {
            const video = e.currentTarget;
            if (
              !localBuffering &&
              !video.paused &&
              !video.seeking &&
              video.currentTime > lastAdvancingTime + 0.25
            )
              hlsRecovery?.progress();
            lastAdvancingTime = video.currentTime;
            setCurrentTime(video.currentTime);
            setDuration(e.currentTarget.duration || 0);
          }
        }}
        onPlay={() => {
          if (!isCasting() && !error()) {
            localAutoplay = true;
            setIsPlaying(true);
            if (nightMode()) {
              initWebAudio();
              if (audioCtx?.state === "suspended") void audioCtx.resume().catch(() => {});
            }
          }
        }}
        onPlaying={() => {
          if (!isCasting() && !error()) {
            confirmedSessionId = playbackData()?.sessionId ?? null;
            localBuffering = false;
            setIsPlaying(true);
            void reportProgress("playing");
          }
        }}
        onPause={() => {
          if (!isCasting()) {
            localAutoplay = false;
            setIsPlaying(false);
            void reportProgress(localProgressState());
          }
        }}
        onLoadedMetadata={(e) => {
          if (!isCasting()) {
            setDuration(e.currentTarget.duration || 0);
            collectLocalStats();
          }
        }}
        onEnded={() => {
          if (isCasting()) return;
          setIsPlaying(false);
          setEnded(true);
          setShowControls(true);
          if (autoplayNext() && nextEpisode()) void playNextEpisode();
          else void stopPlayback();
        }}
        onError={() => {
          if (isCasting()) return;
          failLocalPlayback(
            "The media could not be played. Retry playback at the current position.",
          );
        }}
        onWaiting={() => {
          if (!isCasting()) {
            localBuffering = true;
            void reportProgress("buffering");
          }
        }}
        onClick={togglePlay}
      >
        <Show when={activeSubtitleTrack()}>
          {(sub) => (
            <track
              kind="subtitles"
              src={sub().url}
              srclang={sub().language || "en"}
              label={sub().title || sub().language || "Subtitles"}
              default
              onLoad={(e) => {
                e.currentTarget.track.mode = "showing";
              }}
            />
          )}
        </Show>
      </video>

      {/* Chromecast Active Overlay (M12) */}
      <Show when={isCasting()}>
        <div
          style={{
            position: "absolute",
            inset: 0,
            background: "radial-gradient(circle at center, #1b2838 0%, #0d1117 100%)",
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            "justify-content": "center",
            gap: "16px",
            "z-index": 10,
          }}
        >
          <div style={{ "font-size": "56px" }}>📺</div>
          <div style={{ "font-size": "22px", "font-weight": "bold", color: "#fff" }}>
            Casting to {castDevice() || "Chromecast"}
          </div>
          <div style={{ "font-size": "14px", color: "var(--text-secondary)" }}>
            {mediaTitle()}{" "}
            {mediaItemId() === props.itemId && props.subtitle ? `• ${props.subtitle}` : ""}
          </div>
          <Button variant="secondary" onClick={() => castManager.stopCasting()}>
            Disconnect Cast
          </Button>
        </div>
      </Show>

      {/* Floating Skip Intro / Credits Button */}
      <Show when={activeIntroMarker()}>
        {(intro) => (
          <div style={{ position: "absolute", bottom: "100px", right: "40px", "z-index": 10000 }}>
            <Button
              variant="primary"
              size="lg"
              onClick={() => {
                seekTo(intro().endMs / 1000);
              }}
            >
              Skip Intro ›
            </Button>
          </div>
        )}
      </Show>
      <Show when={activeCreditsMarker()}>
        {(credits) => (
          <div style={{ position: "absolute", bottom: "100px", right: "40px", "z-index": 10000 }}>
            <Button
              variant="primary"
              size="lg"
              onClick={() => {
                seekTo(credits().endMs / 1000);
              }}
            >
              Skip Credits ›
            </Button>
          </div>
        )}
      </Show>

      {/* Stats for Nerds Overlay (M3.8) */}
      <Show when={showStats()}>
        <div
          style={{
            position: "absolute",
            top: "80px",
            left: "32px",
            background: "rgba(10, 12, 16, 0.88)",
            "backdrop-filter": "blur(8px)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            padding: "16px 20px",
            color: "#fff",
            "font-family": "var(--font-mono)",
            "font-size": "12px",
            "z-index": 10005,
            "max-width": "380px",
            "box-shadow": "var(--shadow-xl)",
          }}
        >
          <div
            style={{
              display: "flex",
              "justify-content": "space-between",
              "margin-bottom": "8px",
              "border-bottom": "1px solid var(--border)",
              "padding-bottom": "6px",
            }}
          >
            <span style={{ "font-weight": "bold", color: "var(--accent)" }}>STATS FOR NERDS</span>
            <button
              type="button"
              onClick={() => setShowStats(false)}
              style={{ background: "none", border: "none", color: "#aaa", cursor: "pointer" }}
            >
              ✕
            </button>
          </div>
          <div style={{ display: "flex", "flex-direction": "column", gap: "4px" }}>
            <div>
              Method:{" "}
              <strong style={{ color: "var(--accent)" }}>
                {playbackData()?.method || "Unknown"}
              </strong>
            </div>
            <div>
              Reasons:{" "}
              <span>{playbackData()?.reasons?.join("; ") || "No server reason reported"}</span>
            </div>
            <Show
              when={!isCasting()}
              fallback={
                <p>
                  Receiver video dimensions, decoded frames and buffer are not reported by this
                  integration.
                </p>
              }
            >
              <div>Local resolution: {videoSize()}</div>
              <div>
                Local buffer:{" "}
                {bufferAheadSecs() === null ? "Not measured" : `${bufferAheadSecs()}s`}
              </div>
              <div>Local dropped frames: {droppedFrames() ?? "Not reported"}</div>
              <div>
                Night Mode EQ:{" "}
                {nightMode() && audioCtx
                  ? "Active"
                  : nightMode()
                    ? "Requested; processing unavailable"
                    : "Off"}
              </div>
              <div>Local speed: {playbackRate()}x</div>
            </Show>
            <div>Selected audio stream: {selectedAudioIndex() ?? "None"}</div>
            <div>Server audio sync: {playbackData()?.audioOffsetMs ?? 0} ms</div>
            <div>
              Requested quality limit: {quality() ? `${quality() / 1000000} Mbps` : "Automatic"}
            </div>
            <Show when={diagnosticsError()}>
              <p role="alert">{diagnosticsError()}</p>
            </Show>
            <Show when={diagnostics()}>
              {(server) => (
                <>
                  <div>
                    Session request network:{" "}
                    {server().network === "tailnetDirect"
                      ? "Tailscale (route not measured)"
                      : server().network}
                  </div>
                  <div>Server profile: {server().profileName}</div>
                  <Show when={server().isHls} fallback={<div>Server transcoding: not used</div>}>
                    <Show
                      when={server().transcode}
                      fallback={<div>Transcode runtime: unavailable</div>}
                    >
                      {(runtime) => (
                        <>
                          <div>
                            {runtime().processRunning ? "Active backend" : "Last backend"}:{" "}
                            {runtime().backend}
                          </div>
                          <div>
                            FFmpeg:{" "}
                            {runtime().processRunning
                              ? runtime().throttled
                                ? "Paused ahead of playback"
                                : "Running"
                              : runtime().lastExitFailed
                                ? "Exited with an error"
                                : "Not running; buffered segments may remain"}
                          </div>
                          <div>Transcode restarts: {runtime().restarts}</div>
                        </>
                      )}
                    </Show>
                  </Show>
                </>
              )}
            </Show>
          </div>
        </div>
      </Show>

      <Show
        when={
          props.onPlayNext &&
          (nextEpisode() || nextError()) &&
          (ended() || (duration() > 0 && duration() - currentTime() <= 30))
        }
      >
        <aside
          aria-label="Up Next"
          style={{
            position: "absolute",
            right: "24px",
            bottom: "160px",
            padding: "16px",
            background: "#17191f",
            color: "white",
            "z-index": 10006,
            "max-width": "min(360px, 90vw)",
          }}
        >
          <Show when={nextError()}>
            <p role="alert">{nextError()}</p>
            <Button onClick={() => loadNext(mediaItemId())}>Retry next episode</Button>
          </Show>
          <Show when={nextEpisode()}>
            {(episode) => (
              <>
                <h2>Up Next</h2>
                <p>{episode().title}</p>
                <Button
                  disabled={
                    startingNext() ||
                    castConnecting() ||
                    changingQuality() ||
                    (isCasting() && !receiverAutoplaySupported())
                  }
                  onClick={playNextEpisode}
                >
                  Play next episode
                </Button>
                <label style={{ display: "block" }}>
                  <input
                    type="checkbox"
                    disabled={isCasting() && !receiverAutoplaySupported()}
                    checked={autoplayNext()}
                    onChange={(event) => {
                      setAutoplayNext(event.currentTarget.checked);
                    }}
                  />
                  Automatically play next episode
                </label>
              </>
            )}
          </Show>
        </aside>
      </Show>

      {/* Player Header Overlay */}
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          padding: "24px 32px",
          "z-index": 20,
          background: "linear-gradient(to bottom, rgba(0, 0, 0, 0.85) 0%, transparent 100%)",
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          opacity: showControls() ? 1 : 0,
          transition: "opacity 0.3s ease",
          "pointer-events": showControls() ? "auto" : "none",
        }}
      >
        <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
          <button
            type="button"
            onClick={handleClose}
            title="Back / Close (Esc)"
            style={{
              background: "rgba(255, 255, 255, 0.1)",
              border: "none",
              color: "#fff",
              width: "40px",
              height: "40px",
              "border-radius": "var(--radius-full)",
              cursor: "pointer",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-size": "20px",
            }}
          >
            &larr;
          </button>
          <div>
            <div style={{ "font-size": "18px", "font-weight": "700", color: "#fff" }}>
              {mediaTitle() || "Now Playing"}
            </div>
            {mediaItemId() === props.itemId && props.subtitle && (
              <div style={{ "font-size": "13px", color: "var(--text-muted)" }}>
                {props.subtitle}
              </div>
            )}
          </div>
        </div>

        <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
          <button
            type="button"
            disabled={isCasting()}
            onClick={toggleNightMode}
            title="Toggle Night Mode (n)"
            style={{
              background: nightMode() ? "var(--accent)" : "rgba(255, 255, 255, 0.15)",
              color: "#fff",
              border: "none",
              padding: "6px 12px",
              "border-radius": "var(--radius-sm)",
              "font-size": "12px",
              "font-weight": "600",
              cursor: "pointer",
            }}
          >
            🌙 Night Mode
          </button>
          <button
            type="button"
            onClick={() => setShowStats(!showStats())}
            title="Stats for Nerds (s)"
            style={{
              background: showStats() ? "var(--accent)" : "rgba(255, 255, 255, 0.15)",
              color: "#fff",
              border: "none",
              padding: "6px 10px",
              "border-radius": "var(--radius-sm)",
              "font-size": "12px",
              cursor: "pointer",
            }}
          >
            📊
          </button>
        </div>
      </div>

      {/* Bottom Controls Overlay */}
      <div
        style={{
          position: "absolute",
          bottom: 0,
          left: 0,
          right: 0,
          "z-index": 20,
          padding: "24px 32px 28px",
          background: "linear-gradient(to top, rgba(0, 0, 0, 0.95) 0%, transparent 100%)",
          display: "flex",
          "flex-direction": "column",
          gap: "12px",
          opacity: showControls() ? 1 : 0,
          transition: "opacity 0.3s ease",
          "pointer-events": showControls() ? "auto" : "none",
        }}
      >
        {/* Scrubber timeline with chapter markers */}
        <div
          style={{ position: "relative", display: "flex", "align-items": "center", gap: "12px" }}
        >
          <span
            style={{ "font-size": "12px", color: "#fff", "font-variant-numeric": "tabular-nums" }}
          >
            {formatTime(currentTime())}
          </span>
          <div style={{ position: "relative", flex: 1, display: "flex", "align-items": "center" }}>
            <input
              type="range"
              min={0}
              max={duration() || 100}
              step={0.5}
              value={currentTime()}
              aria-label="Seek video"
              onPointerMove={(event) => {
                const rect = event.currentTarget.getBoundingClientRect();
                if (rect.width > 0 && duration() > 0)
                  setPreviewTime(
                    Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width)) *
                      duration() *
                      1000,
                  );
              }}
              onPointerLeave={() => setPreviewTime(null)}
              onFocus={(event) => setPreviewTime(Number(event.currentTarget.value) * 1000)}
              onBlur={() => setPreviewTime(null)}
              onInput={(event) => {
                handleSeek(event);
                setPreviewTime(Number(event.currentTarget.value) * 1000);
              }}
              style={{
                width: "100%",
                height: "6px",
                cursor: "pointer",
                "accent-color": "var(--accent)",
              }}
            />
            <Show when={previewTime() !== null && playbackData()?.partId}>
              <div
                style={{
                  position: "absolute",
                  bottom: "24px",
                  "z-index": 2,
                  left: `clamp(85px, ${Math.max(0, Math.min(100, ((previewTime() ?? 0) / ((duration() || 1) * 1000)) * 100))}%, calc(100% - 85px))`,
                  transform: "translateX(-50%)",
                }}
              >
                <TrickplayPreview
                  partId={playbackData()?.partId ?? 0}
                  timeMs={previewTime() ?? 0}
                />
              </div>
            </Show>
            {/* Chapter ticks */}
            <Show when={duration() > 0}>
              <For each={currentMarkers().filter((m) => m.kind === "chapter")}>
                {(chap) => {
                  const pct = (chap.startMs / 1000 / duration()) * 100;
                  return (
                    <div
                      title={chap.title || "Chapter"}
                      style={{
                        position: "absolute",
                        left: `${pct}%`,
                        width: "2px",
                        height: "10px",
                        background: "rgba(255, 255, 255, 0.7)",
                        "pointer-events": "none",
                      }}
                    />
                  );
                }}
              </For>
            </Show>
          </div>
          <span
            style={{
              "font-size": "12px",
              color: "var(--text-muted)",
              "font-variant-numeric": "tabular-nums",
            }}
          >
            {formatTime(duration())}
          </span>
        </div>

        {/* Buttons Row */}
        <div
          style={{ display: "flex", "align-items": "center", "justify-content": "space-between" }}
        >
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <button
              type="button"
              onClick={togglePlay}
              title={isPlaying() ? "Pause (Space)" : "Play (Space)"}
              style={{
                background: "transparent",
                border: "none",
                color: "#fff",
                "font-size": "24px",
                cursor: "pointer",
                padding: "4px",
              }}
            >
              {isPlaying() ? "⏸" : "▶"}
            </button>
            <button
              type="button"
              onClick={() => {
                seekTo(Math.max(0, currentTime() - 10));
              }}
              style={{
                background: "transparent",
                border: "none",
                color: "#ccc",
                "font-size": "14px",
                cursor: "pointer",
              }}
            >
              ↺ 10s
            </button>
            <button
              type="button"
              onClick={() => {
                seekTo(Math.min(duration() || 0, currentTime() + 10));
              }}
              style={{
                background: "transparent",
                border: "none",
                color: "#ccc",
                "font-size": "14px",
                cursor: "pointer",
              }}
            >
              10s ↻
            </button>

            {/* Volume */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "8px",
                "margin-left": "10px",
              }}
            >
              <button
                type="button"
                onClick={toggleMute}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "#ccc",
                  cursor: "pointer",
                }}
              >
                {isMuted() || volume() === 0 ? "🔇" : "🔊"}
              </button>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={isMuted() ? 0 : volume()}
                onInput={handleVolumeChange}
                style={{ width: "70px", height: "4px", cursor: "pointer", "accent-color": "#fff" }}
              />
            </div>
          </div>

          <div
            style={{ display: "flex", "align-items": "center", gap: "14px", position: "relative" }}
          >
            <div style={{ display: "grid", gap: "4px" }}>
              <Show when={(playbackData()?.audioTracks?.length ?? 0) > 0}>
                <label for="player-audio-track">Audio track</label>
                <select
                  id="player-audio-track"
                  value={selectedAudioIndex()}
                  disabled={
                    loading() ||
                    changingQuality() ||
                    castConnecting() ||
                    (isCasting() && !receiverAutoplaySupported())
                  }
                  onChange={(event) => {
                    const select = event.currentTarget;
                    void changeAudioTrack(Number(select.value)).finally(() => {
                      select.value = String(selectedAudioIndex() ?? "");
                    });
                  }}
                >
                  <For each={playbackData()?.audioTracks}>
                    {(track) => (
                      <option value={track.index}>
                        {track.title || track.language || `Track ${track.index}`} · {track.codec}
                        {track.channels ? ` · ${track.channels}ch` : ""}
                      </option>
                    )}
                  </For>
                </select>
              </Show>
              <label for="player-quality">Quality limit</label>
              <select
                id="player-quality"
                value={quality()}
                disabled={isCasting() || loading() || changingQuality() || castConnecting()}
                onChange={(event) => {
                  const select = event.currentTarget;
                  void changeQuality(Number(select.value)).finally(() => {
                    select.value = String(quality());
                  });
                }}
              >
                <option value={0}>Automatic</option>
                <option value={2000000}>2 Mbps</option>
                <option value={4000000}>4 Mbps</option>
                <option value={8000000}>8 Mbps</option>
                <option value={20000000}>20 Mbps</option>
              </select>
              <label for="player-audio-delay">Audio sync (− earlier, + later)</label>
              <select
                id="player-audio-delay"
                value={audioSyncMs()}
                disabled={
                  loading() ||
                  changingQuality() ||
                  castConnecting() ||
                  (isCasting() && !receiverAutoplaySupported())
                }
                onChange={(event) => {
                  const select = event.currentTarget;
                  void changeAudioSync(Number(select.value)).finally(() => {
                    select.value = String(audioSyncMs());
                  });
                }}
              >
                <For
                  each={Array.from(
                    new Set([
                      -10000,
                      -5000,
                      -2000,
                      -1000,
                      -500,
                      -250,
                      -100,
                      0,
                      100,
                      250,
                      500,
                      1000,
                      2000,
                      5000,
                      10000,
                      audioSyncMs(),
                    ]),
                  )}
                >
                  {(milliseconds) => <option value={milliseconds}>{milliseconds} ms</option>}
                </For>
              </select>
            </div>
            {/* Speed Selector */}
            <div style={{ position: "relative" }}>
              <button
                type="button"
                disabled={isCasting()}
                onClick={() => setShowSpeedMenu(!showSpeedMenu())}
                style={{
                  background: "rgba(255, 255, 255, 0.1)",
                  border: "none",
                  color: "#fff",
                  padding: "6px 10px",
                  "border-radius": "var(--radius-sm)",
                  cursor: "pointer",
                  "font-size": "12px",
                  "font-weight": "bold",
                }}
              >
                {playbackRate()}x
              </button>
              <Show when={showSpeedMenu()}>
                <div
                  style={{
                    position: "absolute",
                    bottom: "40px",
                    right: 0,
                    background: "rgba(20, 20, 20, 0.95)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-md)",
                    padding: "6px 0",
                    width: "80px",
                    "z-index": 10002,
                  }}
                >
                  <For each={[0.5, 0.75, 1.0, 1.25, 1.5, 2.0]}>
                    {(rate) => (
                      <button
                        type="button"
                        onClick={() => handleSetSpeed(rate)}
                        style={{
                          display: "block",
                          width: "100%",
                          padding: "6px 12px",
                          "text-align": "center",
                          background: playbackRate() === rate ? "var(--accent)" : "transparent",
                          color: "#fff",
                          border: "none",
                          cursor: "pointer",
                          "font-size": "12px",
                        }}
                      >
                        {rate}x
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            {/* Subtitles Menu */}
            <Show when={subtitleChoices().length > 0}>
              <div style={{ position: "relative" }}>
                <button
                  type="button"
                  aria-label="Subtitle tracks"
                  disabled={loading() || changingQuality() || castConnecting()}
                  onClick={() => setShowSubtitlesMenu(!showSubtitlesMenu())}
                  style={{
                    background:
                      selectedSubtitleIndex() >= 0 ? "var(--accent)" : "rgba(255,255,255,0.1)",
                    border: "none",
                    color: "#fff",
                    padding: "6px 12px",
                    "border-radius": "var(--radius-sm)",
                    cursor: "pointer",
                    "font-size": "13px",
                    "font-weight": "600",
                  }}
                >
                  💬 CC
                </button>
                <Show when={showSubtitlesMenu()}>
                  <div
                    style={{
                      position: "absolute",
                      bottom: "40px",
                      right: 0,
                      background: "rgba(20, 20, 20, 0.95)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-md)",
                      padding: "8px 0",
                      width: "200px",
                      "z-index": 10002,
                    }}
                  >
                    <button
                      type="button"
                      onClick={() => selectSubtitle(-1)}
                      style={{
                        display: "block",
                        width: "100%",
                        padding: "8px 14px",
                        "text-align": "left",
                        background:
                          selectedSubtitleIndex() === -1 ? "var(--accent-light)" : "transparent",
                        color: selectedSubtitleIndex() === -1 ? "var(--accent)" : "#fff",
                        border: "none",
                        cursor: "pointer",
                        "font-size": "13px",
                      }}
                    >
                      Off
                    </button>
                    <For each={subtitleChoices()}>
                      {(sub) => (
                        <button
                          type="button"
                          onClick={() => selectSubtitle(sub.index)}
                          style={{
                            display: "block",
                            width: "100%",
                            padding: "8px 14px",
                            "text-align": "left",
                            background:
                              selectedSubtitleIndex() === sub.index
                                ? "var(--accent-light)"
                                : "transparent",
                            color: selectedSubtitleIndex() === sub.index ? "var(--accent)" : "#fff",
                            border: "none",
                            cursor: "pointer",
                            "font-size": "13px",
                          }}
                        >
                          {sub.title || sub.language || `Track ${sub.index}`}
                          {sub.isForced ? " (Forced)" : ""}
                          {sub.requiresTranscode ? " (video conversion)" : ""}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
            </Show>

            {/* Chromecast / Remote Playback Button (M12) */}
            <Show when={isCastAvailable()}>
              <button
                type="button"
                onClick={handleCast}
                disabled={castConnecting() || changingQuality()}
                title={
                  isCasting()
                    ? `Casting to ${castDevice() || "Chromecast"}`
                    : "Cast to TV (Chromecast)"
                }
                style={{
                  background: isCasting() ? "var(--accent)" : "transparent",
                  border: "none",
                  color: "#fff",
                  "font-size": "18px",
                  cursor: "pointer",
                  padding: "4px 8px",
                  "border-radius": "var(--radius-sm)",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                }}
              >
                ⎚
              </button>
            </Show>

            <button
              type="button"
              onClick={toggleFullscreen}
              title="Fullscreen (f)"
              style={{
                background: "transparent",
                border: "none",
                color: "#fff",
                "font-size": "18px",
                cursor: "pointer",
                padding: "4px",
              }}
            >
              ⛶
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

export default Player;
