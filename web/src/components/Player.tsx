import Hls from "hls.js";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { getPreferredProfile } from "../api/deviceProfile";
import type { components } from "../api/schema";
import { useTheme } from "../context/ThemeContext";
import { castManager } from "../utils/cast";
import { Button } from "./ui";

type PlaybackResponse = components["schemas"]["PlaybackResponse"];
type PlaybackSubtitleTrack = components["schemas"]["PlaybackSubtitleTrack"];

export interface PlayerProps {
  itemId: number;
  partId?: number;
  initialStartMs?: number;
  audioStreamIndex?: number;
  subtitleStreamIndex?: number | null;
  title?: string;
  subtitle?: string;
  onClose: () => void;
}

export function Player(props: PlayerProps) {
  let videoRef: HTMLVideoElement | undefined;
  let playerContainerRef: HTMLDivElement | undefined;
  let hlsInstance: Hls | null = null;
  let progressInterval: number | null = null;
  let disposed = false;
  let stoppedSession: string | null = null;
  let hideControlsTimer: number | null = null;

  // WebAudio nodes for Night Mode / Smart Dialogue Boost
  let audioCtx: AudioContext | null = null;
  let audioSource: MediaElementAudioSourceNode | null = null;
  let dialogueBoostFilter: BiquadFilterNode | null = null;
  let compressorNode: DynamicsCompressorNode | null = null;

  const { subtitleStyle, nightMode, setNightMode } = useTheme();

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
  const [droppedFrames, setDroppedFrames] = createSignal(0);
  const [bufferAheadSecs, setBufferAheadSecs] = createSignal(0);

  // Chromecast & Remote Playback state (M12)
  const [isCasting, setIsCasting] = createSignal(false);
  const [castDevice, setCastDevice] = createSignal<string | null>(null);
  const [isCastAvailable, setIsCastAvailable] = createSignal(true);

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
      audioSource = audioCtx.createMediaElementSource(videoRef);

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

      connectAudioNodes();
    } catch {
      // Browser WebAudio policy or already connected
    }
  };

  const connectAudioNodes = () => {
    if (!audioCtx || !audioSource) return;
    audioSource.disconnect();
    if (dialogueBoostFilter) dialogueBoostFilter.disconnect();
    if (compressorNode) compressorNode.disconnect();

    if (nightMode() && dialogueBoostFilter && compressorNode) {
      audioSource.connect(dialogueBoostFilter);
      dialogueBoostFilter.connect(compressorNode);
      compressorNode.connect(audioCtx.destination);
    } else {
      audioSource.connect(audioCtx.destination);
    }
  };

  const toggleNightMode = () => {
    initWebAudio();
    if (audioCtx && audioCtx.state === "suspended") {
      audioCtx.resume();
    }
    const next = !nightMode();
    setNightMode(next);
    connectAudioNodes();
  };

  const reportProgress = async (state: "playing" | "paused" | "buffering") => {
    const data = playbackData();
    const vid = videoRef;
    if (!data || !vid) return;

    const positionMs = Math.floor(vid.currentTime * 1000);
    try {
      await client.POST("/api/v1/playback/{session}/progress", {
        params: { path: { session: data.sessionId } },
        body: { positionMs, state },
      });
    } catch {}
  };

  const stopSession = async (sessionId: string, positionMs: number) => {
    try {
      await client.POST("/api/v1/playback/{session}/stop", {
        params: { path: { session: sessionId } },
        body: { positionMs },
      });
    } catch {}
  };

  const stopPlayback = async () => {
    const data = playbackData();
    if (!data || stoppedSession === data.sessionId) return;
    stoppedSession = data.sessionId;
    await stopSession(data.sessionId, Math.floor((videoRef?.currentTime ?? 0) * 1000));
  };

  const startPlaybackSession = async (startMs = 0) => {
    setLoading(true);
    setError(null);

    try {
      const { data, error: err } = await client.POST("/api/v1/playback", {
        body: {
          itemId: props.itemId,
          partId: props.partId,
          profile: getPreferredProfile() as unknown as Record<string, never>,
          startMs,
          audioStreamIndex: props.audioStreamIndex,
          subtitleStreamIndex: props.subtitleStreamIndex ?? undefined,
        },
      });

      if (disposed) {
        if (data) void stopSession(data.sessionId, data.startMs);
        return;
      }
      if (err || !data) {
        setError(err ? String(err) : "Failed to start playback session");
        setLoading(false);
        return;
      }

      setPlaybackData(data as PlaybackResponse);
      initPlayer(data as PlaybackResponse, data.startMs);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Playback initiation failed");
      setLoading(false);
    }
  };

  const initPlayer = (data: PlaybackResponse, startMs: number) => {
    const vid = videoRef;
    if (!vid) return;

    if (hlsInstance) {
      hlsInstance.destroy();
      hlsInstance = null;
    }

    const streamUrl = data.url;
    const isHls = new URL(streamUrl, window.location.href).pathname.endsWith(".m3u8");

    if (isHls && Hls.isSupported()) {
      const hls = new Hls({
        enableWorker: true,
        startPosition: startMs > 0 ? startMs / 1000 : -1,
      });
      hls.loadSource(streamUrl);
      hls.attachMedia(vid);
      hls.on(Hls.Events.MANIFEST_PARSED, () => {
        setLoading(false);
        vid.play().catch(() => {});
      });
      hls.on(Hls.Events.ERROR, (_event, errData) => {
        if (errData.fatal) {
          switch (errData.type) {
            case Hls.ErrorTypes.NETWORK_ERROR:
              hls.startLoad();
              break;
            case Hls.ErrorTypes.MEDIA_ERROR:
              hls.recoverMediaError();
              break;
            default:
              hls.destroy();
              setError("Fatal HLS streaming error");
              break;
          }
        }
      });
      hlsInstance = hls;
    } else {
      vid.src = streamUrl;
      if (startMs > 0) {
        vid.currentTime = startMs / 1000;
      }
      setLoading(false);
      vid.play().catch(() => {});
    }

    const selectedSub = props.subtitleStreamIndex;
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

    progressInterval = window.setInterval(() => {
      if (isPlaying()) {
        reportProgress("playing");
        // Update stats
        if (videoRef) {
          const qual = videoRef.getVideoPlaybackQuality?.();
          if (qual) setDroppedFrames(qual.droppedVideoFrames);
          const buffered = videoRef.buffered;
          if (buffered.length > 0) {
            const end = buffered.end(buffered.length - 1);
            setBufferAheadSecs(Math.max(0, Math.round(end - videoRef.currentTime)));
          }
        }
      }
    }, 5000);

    const handleKeyDown = (e: KeyboardEvent) => {
      const vid = videoRef;
      if (!vid) return;

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
          vid.currentTime = Math.max(0, vid.currentTime - 10);
          break;
        case "ArrowRight":
        case "l":
          e.preventDefault();
          vid.currentTime = Math.min(vid.duration || 0, vid.currentTime + 10);
          break;
        case "ArrowUp":
          e.preventDefault();
          setVolume((v) => {
            const next = Math.min(1, v + 0.1);
            vid.volume = next;
            return next;
          });
          break;
        case "ArrowDown":
          e.preventDefault();
          setVolume((v) => {
            const next = Math.max(0, v - 0.1);
            vid.volume = next;
            return next;
          });
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
    const unsubCast = castManager.subscribe((cState) => {
      setIsCastAvailable(cState.isAvailable);
      setIsCasting(cState.isCasting);
      setCastDevice(cState.deviceName);
      if (cState.isCasting) {
        setIsPlaying(cState.playerState === "playing");
        setCurrentTime(cState.currentTime);
        if (cState.duration > 0) setDuration(cState.duration);
        setVolume(cState.volume);
        setIsMuted(cState.isMuted);
      }
    });

    onCleanup(() => {
      disposed = true;
      unsubCast();
      window.removeEventListener("keydown", handleKeyDown);
      if (progressInterval) clearInterval(progressInterval);
      if (hideControlsTimer) clearTimeout(hideControlsTimer);
      stopPlayback();
      if (hlsInstance) {
        hlsInstance.destroy();
        hlsInstance = null;
      }
      if (audioCtx) {
        audioCtx.close().catch(() => {});
      }
    });
  });

  const handleCast = async () => {
    if (isCasting()) {
      castManager.stopCasting();
      return;
    }
    const data = playbackData();
    if (!data) return;
    if (videoRef) {
      videoRef.pause();
    }
    await castManager.requestCastSession(
      {
        contentId: data.url,
        title: props.title,
        subtitle: props.subtitle,
        currentTime: currentTime(),
        duration: duration(),
        itemId: props.itemId,
      },
      videoRef,
    );
  };

  const togglePlay = () => {
    if (isCasting()) {
      if (isPlaying()) {
        castManager.pause();
        setIsPlaying(false);
      } else {
        castManager.play();
        setIsPlaying(true);
      }
      return;
    }
    const vid = videoRef;
    if (!vid) return;
    if (vid.paused) {
      vid.play();
    } else {
      vid.pause();
    }
  };

  const toggleMute = () => {
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

  const handleSeek = (e: Event & { currentTarget: HTMLInputElement }) => {
    const targetTime = Number(e.currentTarget.value);
    setCurrentTime(targetTime);
    if (isCasting()) {
      castManager.seek(targetTime);
      return;
    }
    const vid = videoRef;
    if (!vid) return;
    vid.currentTime = targetTime;
  };

  const handleVolumeChange = (e: Event & { currentTarget: HTMLInputElement }) => {
    const v = Number(e.currentTarget.value);
    setVolume(v);
    setIsMuted(false);
    if (isCasting()) {
      castManager.setVolume(v);
      return;
    }
    const vid = videoRef;
    if (!vid) return;
    vid.volume = v;
    vid.muted = false;
  };

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

  const selectSubtitle = (index: number) => {
    setSelectedSubtitleIndex(index);
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
              style={{
                color: "var(--danger)",
                "font-size": "var(--font-size-lg)",
                "margin-bottom": "var(--space-4)",
              }}
            >
              {err()}
            </p>
            <Button variant="secondary" onClick={props.onClose}>
              Close
            </Button>
          </div>
        )}
      </Show>

      {/* Main Video Element */}
      {/* biome-ignore lint/a11y/useMediaCaption: dynamic subtitle tracks loaded conditionally */}
      <video
        ref={videoRef}
        playsinline
        style={{
          width: "100%",
          height: "100%",
          "object-fit": "contain",
        }}
        onTimeUpdate={(e) => {
          setCurrentTime(e.currentTarget.currentTime);
          setDuration(e.currentTarget.duration || 0);
        }}
        onPlay={() => {
          setIsPlaying(true);
          reportProgress("playing");
        }}
        onPause={() => {
          setIsPlaying(false);
          reportProgress("paused");
        }}
        onLoadedMetadata={(e) => {
          setDuration(e.currentTarget.duration || 0);
        }}
        onEnded={() => {
          setIsPlaying(false);
          void stopPlayback();
        }}
        onError={() => {
          setLoading(false);
          setError("The media could not be played.");
        }}
        onWaiting={() => reportProgress("buffering")}
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
            {props.title} {props.subtitle ? `• ${props.subtitle}` : ""}
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
                if (videoRef) videoRef.currentTime = intro().endMs / 1000;
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
                if (videoRef) videoRef.currentTime = credits().endMs / 1000;
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
              Reasons: <span>{playbackData()?.reasons?.join("; ") || "Direct stream capable"}</span>
            </div>
            <div>
              Resolution:{" "}
              <span>
                {videoRef?.videoWidth}x{videoRef?.videoHeight}
              </span>
            </div>
            <div>
              Buffer Health: <span>{bufferAheadSecs()}s</span>
            </div>
            <div>
              Dropped Frames: <span>{droppedFrames()}</span>
            </div>
            <div>
              Night Mode EQ: <span>{nightMode() ? "Active (WebAudio Boost & Comp)" : "Off"}</span>
            </div>
            <div>
              Speed: <span>{playbackRate()}x</span>
            </div>
          </div>
        </div>
      </Show>

      {/* Player Header Overlay */}
      <div
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          padding: "24px 32px",
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
              {props.title || "Now Playing"}
            </div>
            {props.subtitle && (
              <div style={{ "font-size": "13px", color: "var(--text-muted)" }}>
                {props.subtitle}
              </div>
            )}
          </div>
        </div>

        <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
          <button
            type="button"
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
              onInput={handleSeek}
              style={{
                width: "100%",
                height: "6px",
                cursor: "pointer",
                "accent-color": "var(--accent)",
              }}
            />
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
                if (videoRef) videoRef.currentTime = Math.max(0, videoRef.currentTime - 10);
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
                if (videoRef)
                  videoRef.currentTime = Math.min(
                    videoRef.duration || 0,
                    videoRef.currentTime + 10,
                  );
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
            {/* Speed Selector */}
            <div style={{ position: "relative" }}>
              <button
                type="button"
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
            <Show when={playbackData() && (playbackData()?.subtitles.length ?? 0) > 0}>
              <div style={{ position: "relative" }}>
                <button
                  type="button"
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
                    <For each={playbackData()?.subtitles}>
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
