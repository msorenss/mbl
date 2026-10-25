import Hls from "hls.js";
import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { getPreferredProfile } from "../api/deviceProfile";
import type { components } from "../api/schema";

type PlaybackResponse = components["schemas"]["PlaybackResponse"];
type PlaybackSubtitleTrack = components["schemas"]["PlaybackSubtitleTrack"];

export interface PlayerProps {
  itemId: number;
  initialStartMs?: number;
  title?: string;
  subtitle?: string;
  onClose: () => void;
}

export function Player(props: PlayerProps) {
  let videoRef: HTMLVideoElement | undefined;
  let playerContainerRef: HTMLDivElement | undefined;
  let hlsInstance: Hls | null = null;
  let progressInterval: number | null = null;
  let hideControlsTimer: number | null = null;

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
      if (isPlaying()) {
        setShowControls(false);
        setShowSubtitlesMenu(false);
      }
    }, 3500);
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
    } catch {
      // Ignore background progress reporting errors
    }
  };

  const stopPlayback = async () => {
    const data = playbackData();
    const vid = videoRef;
    if (!data || !vid) return;

    const positionMs = Math.floor(vid.currentTime * 1000);
    try {
      await client.POST("/api/v1/playback/{session}/stop", {
        params: { path: { session: data.sessionId } },
        body: { positionMs },
      });
    } catch {
      // Ignore stop errors
    }
  };

  const startPlaybackSession = async (startMs = 0) => {
    setLoading(true);
    setError(null);

    try {
      const { data, error: err } = await client.POST("/api/v1/playback", {
        body: {
          itemId: props.itemId,
          profile: getPreferredProfile() as unknown as Record<string, never>,
          startMs: startMs > 0 ? startMs : undefined,
        },
      });

      if (err || !data) {
        setError(err ? String(err) : "Failed to start playback session");
        setLoading(false);
        return;
      }

      setPlaybackData(data);
      initPlayer(data, startMs);
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
    const isHls = streamUrl.endsWith(".m3u8");

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
      // Direct play or native HLS (e.g. Safari)
      vid.src = streamUrl;
      if (startMs > 0) {
        vid.currentTime = startMs / 1000;
      }
      setLoading(false);
      vid.play().catch(() => {});
    }

    // Set initial default subtitle if available
    const defaultSub = data.subtitles.find((s) => s.isDefault);
    if (defaultSub) {
      setSelectedSubtitleIndex(defaultSub.index);
    }
  };

  onMount(() => {
    const initialStart = props.initialStartMs ?? 0;
    if (initialStart > 10000) {
      setShowResumePrompt(true);
    } else {
      startPlaybackSession(0);
    }

    // Progress interval every 10 seconds
    progressInterval = window.setInterval(() => {
      if (isPlaying()) {
        reportProgress("playing");
      }
    }, 10000);

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
        case "Escape":
          e.preventDefault();
          handleClose();
          break;
      }
    };

    window.addEventListener("keydown", handleKeyDown);

    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      if (progressInterval) clearInterval(progressInterval);
      if (hideControlsTimer) clearTimeout(hideControlsTimer);
      stopPlayback();
      if (hlsInstance) {
        hlsInstance.destroy();
        hlsInstance = null;
      }
    });
  });

  const togglePlay = () => {
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
    const vid = videoRef;
    if (!vid) return;
    const targetTime = Number(e.currentTarget.value);
    vid.currentTime = targetTime;
    setCurrentTime(targetTime);
  };

  const handleVolumeChange = (e: Event & { currentTarget: HTMLInputElement }) => {
    const vid = videoRef;
    if (!vid) return;
    const v = Number(e.currentTarget.value);
    vid.volume = v;
    setVolume(v);
    setIsMuted(v === 0);
  };

  const handleClose = () => {
    stopPlayback();
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
            "box-shadow": "0 25px 50px -12px rgba(0, 0, 0, 0.8)",
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
            <button
              type="button"
              onClick={() => {
                setShowResumePrompt(false);
                startPlaybackSession(props.initialStartMs ?? 0);
              }}
              style={{
                padding: "10px 20px",
                background: "var(--accent)",
                color: "#fff",
                border: "none",
                "border-radius": "var(--radius-sm)",
                "font-weight": "600",
                cursor: "pointer",
              }}
            >
              Resume ({formatTime((props.initialStartMs ?? 0) / 1000)})
            </button>
            <button
              type="button"
              onClick={() => {
                setShowResumePrompt(false);
                startPlaybackSession(0);
              }}
              style={{
                padding: "10px 20px",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                color: "var(--text-primary)",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
              }}
            >
              Start Over
            </button>
          </div>
        </div>
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
            />
          )}
        </Show>
      </video>

      {/* Loading Spinner */}
      <Show when={loading()}>
        <div
          style={{
            position: "absolute",
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            gap: "14px",
            color: "var(--text-secondary)",
          }}
        >
          <div
            style={{
              width: "48px",
              height: "48px",
              border: "4px solid rgba(255, 255, 255, 0.2)",
              "border-top-color": "var(--accent)",
              "border-radius": "50%",
              animation: "spin 1s linear infinite",
            }}
          />
          <span style={{ "font-size": "14px", "font-weight": "500" }}>
            Negotiating playback stream...
          </span>
        </div>
      </Show>

      {/* Error Display */}
      <Show when={error()}>
        <div
          style={{
            position: "absolute",
            background: "rgba(30, 10, 10, 0.95)",
            border: "1px solid var(--danger)",
            "border-radius": "var(--radius-md)",
            padding: "24px 32px",
            "max-width": "460px",
            "text-align": "center",
          }}
        >
          <div style={{ "font-size": "24px", "margin-bottom": "8px" }}>⚠️</div>
          <h3 style={{ margin: "0 0 8px", color: "var(--danger)", "font-size": "18px" }}>
            Playback Error
          </h3>
          <p style={{ margin: "0 0 16px", color: "var(--text-secondary)", "font-size": "13px" }}>
            {error()}
          </p>
          <button
            type="button"
            onClick={handleClose}
            style={{
              padding: "8px 18px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              cursor: "pointer",
              "font-weight": "600",
            }}
          >
            Close Player
          </button>
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
          background: "linear-gradient(to bottom, rgba(0, 0, 0, 0.8) 0%, transparent 100%)",
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
              "border-radius": "50%",
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
            <Show when={props.subtitle}>
              <div style={{ "font-size": "13px", color: "var(--text-muted)" }}>
                {props.subtitle}
              </div>
            </Show>
          </div>
        </div>

        <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
          <Show when={playbackData()}>
            {(data) => (
              <span
                style={{
                  "font-size": "11px",
                  padding: "4px 8px",
                  "border-radius": "4px",
                  background: "rgba(255, 255, 255, 0.15)",
                  color: "#fff",
                  "text-transform": "uppercase",
                  "font-weight": "600",
                  "letter-spacing": "0.5px",
                }}
              >
                {data().method}
              </span>
            )}
          </Show>
        </div>
      </div>

      {/* Player Bottom Controls Overlay */}
      <div
        style={{
          position: "absolute",
          bottom: 0,
          left: 0,
          right: 0,
          padding: "24px 32px 28px",
          background: "linear-gradient(to top, rgba(0, 0, 0, 0.9) 0%, transparent 100%)",
          display: "flex",
          "flex-direction": "column",
          gap: "12px",
          opacity: showControls() ? 1 : 0,
          transition: "opacity 0.3s ease",
          "pointer-events": showControls() ? "auto" : "none",
        }}
      >
        {/* Scrubber timeline */}
        <div style={{ display: "flex", "align-items": "center", gap: "12px" }}>
          <span
            style={{ "font-size": "12px", color: "#fff", "font-variant-numeric": "tabular-nums" }}
          >
            {formatTime(currentTime())}
          </span>
          <input
            type="range"
            min={0}
            max={duration() || 100}
            step={0.5}
            value={currentTime()}
            onInput={handleSeek}
            style={{
              flex: 1,
              height: "5px",
              cursor: "pointer",
              "accent-color": "var(--accent)",
            }}
          />
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

        {/* Action Buttons Row */}
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

            {/* Skip -10s */}
            <button
              type="button"
              onClick={() => {
                if (videoRef) videoRef.currentTime = Math.max(0, videoRef.currentTime - 10);
              }}
              title="Back 10s (Left)"
              style={{
                background: "transparent",
                border: "none",
                color: "#ccc",
                "font-size": "15px",
                cursor: "pointer",
              }}
            >
              ↺ 10s
            </button>

            {/* Skip +10s */}
            <button
              type="button"
              onClick={() => {
                if (videoRef)
                  videoRef.currentTime = Math.min(
                    videoRef.duration || 0,
                    videoRef.currentTime + 10,
                  );
              }}
              title="Forward 10s (Right)"
              style={{
                background: "transparent",
                border: "none",
                color: "#ccc",
                "font-size": "15px",
                cursor: "pointer",
              }}
            >
              10s ↻
            </button>

            {/* Volume control */}
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
                title="Mute (m)"
                style={{
                  background: "transparent",
                  border: "none",
                  color: "#ccc",
                  "font-size": "16px",
                  cursor: "pointer",
                }}
              >
                {isMuted() || volume() === 0 ? "🔇" : volume() < 0.5 ? "🔉" : "🔊"}
              </button>
              <input
                type="range"
                min={0}
                max={1}
                step={0.05}
                value={isMuted() ? 0 : volume()}
                onInput={handleVolumeChange}
                style={{
                  width: "70px",
                  height: "4px",
                  cursor: "pointer",
                  "accent-color": "#fff",
                }}
              />
            </div>
          </div>

          <div
            style={{ display: "flex", "align-items": "center", gap: "14px", position: "relative" }}
          >
            {/* Subtitles Menu Button */}
            <Show when={playbackData() && (playbackData()?.subtitles.length ?? 0) > 0}>
              <div style={{ position: "relative" }}>
                <button
                  type="button"
                  onClick={() => setShowSubtitlesMenu(!showSubtitlesMenu())}
                  title="Subtitles"
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
                      "box-shadow": "0 10px 25px rgba(0,0,0,0.7)",
                      "z-index": 10002,
                    }}
                  >
                    <div
                      style={{
                        padding: "6px 14px",
                        "font-size": "11px",
                        "text-transform": "uppercase",
                        color: "var(--text-muted)",
                        "font-weight": "700",
                      }}
                    >
                      Subtitles
                    </div>
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

            {/* Fullscreen Button */}
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
