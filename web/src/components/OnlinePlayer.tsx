import type Hls from "hls.js";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import client, { getStoredToken } from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { castManager } from "../utils/cast";
import { Button } from "./ui/Button";

type Source = components["schemas"]["OnlineSource"];
type Session = components["schemas"]["OnlinePlayback"];
export function OnlinePlayer(props: { source: Source; onClose: () => void; onSaved?: () => void }) {
  const { t } = useI18n();
  const [error, setError] = createSignal("");
  const [ready, setReady] = createSignal(false);
  const [playing, setPlaying] = createSignal(false);
  const [isCasting, setIsCasting] = createSignal(castManager.getState().isCasting);
  const [isCastAvailable, setIsCastAvailable] = createSignal(castManager.getState().isAvailable);
  const [castDevice, setCastDevice] = createSignal(castManager.getState().deviceName);
  const [castConnecting, setCastConnecting] = createSignal(false);
  let media!: HTMLVideoElement;
  let section!: HTMLElement;
  let session: Session | undefined;
  let hls: Hls | undefined;
  let disposed = false;
  let closed = false;
  let pending: Promise<void> = Promise.resolve();
  let sending = false;
  let hasPlayed = false;
  let completed = false;
  let heartbeat: ReturnType<typeof setInterval> | undefined;
  let loadDeadline: ReturnType<typeof setTimeout> | undefined;
  let expiry: ReturnType<typeof setTimeout> | undefined;
  const abort = new AbortController();
  const deadline = setTimeout(() => abort.abort(), 70000);
  const snapshot = (ended = false) => ({
    position: media.currentTime,
    duration: media.duration,
    ended: ended || completed,
    heartbeatOnly: !hasPlayed,
  });
  const progress = (ended = false) => {
    if (!session || !ready() || closed || !Number.isFinite(media.duration)) return;
    const current = session;
    const body = snapshot(ended);
    sending = true;
    pending = pending
      .then(async () => {
        const res = await client.PATCH("/api/v1/online/playback/{id}", {
          params: { path: { id: current.id } },
          body,
          signal: AbortSignal.timeout(5000),
          keepalive: true,
        });
        if (res.error) throw new Error(apiErrorMessage(res.error, t("onlinePlayerError")));
      })
      .catch((cause) => {
        if (!disposed) fail(apiErrorMessage(cause, t("onlinePlayerError")));
      })
      .finally(() => {
        sending = false;
      });
  };
  const stop = () => {
    clearTimeout(loadDeadline);
    clearInterval(heartbeat);
    clearTimeout(expiry);
    hls?.destroy();
    hls = undefined;
    media.pause();
    media.removeAttribute("src");
    media.load();
  };
  const release = () => {
    if (closed) return;
    closed = true;
    const id = session?.id;
    if (id)
      void pending.finally(async () => {
        await client
          .DELETE("/api/v1/online/playback/{id}", {
            params: { path: { id } },
            signal: AbortSignal.timeout(5000),
            keepalive: true,
          })
          .catch(() => {});
        props.onSaved?.();
      });
  };
  const fail = (message: string) => {
    if (disposed || error()) return;
    console.warn(`[OnlinePlayer] Playback error: ${message}`);
    setError(message);
    setPlaying(false);
    release();
    stop();
  };
  onMount(async () => {
    console.info(
      `[OnlinePlayer] Initializing player for "${props.source.title}" (${props.source.url})`,
    );
    const unsubCast = castManager.subscribe((cState) => {
      setIsCasting(cState.isCasting);
      setIsCastAvailable(cState.isAvailable);
      setCastDevice(cState.deviceName);
    });
    onCleanup(unsubCast);
    section.scrollIntoView?.({ block: "nearest" });
    try {
      const res = await client.POST("/api/v1/online/playback", {
        body: { url: props.source.url },
        signal: abort.signal,
      });
      if (!res.data) throw new Error(apiErrorMessage(res.error, t("onlinePlayerError")));
      session = res.data;
      console.info(
        `[OnlinePlayer] Session started: id=${session.id}, HLS=${session.hls}, resumePos=${session.position}s`,
      );
      if (disposed) {
        void client
          .DELETE("/api/v1/online/playback/{id}", {
            params: { path: { id: session.id } },
            signal: AbortSignal.timeout(5000),
            keepalive: true,
          })
          .catch(() => {});
        return;
      }
      loadDeadline = setTimeout(() => fail(t("onlinePlayerError")), 20000);
      expiry = setTimeout(
        () => fail(t("onlinePlayerError")),
        Math.max(0, session.expiresAt - Date.now()),
      );
      heartbeat = setInterval(() => {
        if (!sending) progress();
      }, 10000);
      if (session.hls) {
        const url = new URL(session.streamUrl, location.href);
        if (url.origin !== location.origin || !url.pathname.startsWith("/api/v1/online/playback/"))
          throw new Error(t("onlinePlayerError"));
        if (media.canPlayType("application/vnd.apple.mpegurl")) media.src = session.streamUrl;
        else {
          const { default: HlsClass } = await import("hls.js");
          if (disposed) return;
          if (!HlsClass.isSupported()) throw new Error(t("onlinePlayerError"));
          hls = new HlsClass({
            maxBufferLength: 30,
            maxMaxBufferLength: 60,
            xhrSetup(xhr) {
              const token = getStoredToken();
              if (token) xhr.setRequestHeader("Authorization", `Bearer ${token}`);
            },
          });
          hls.on(HlsClass.Events.ERROR, (_event, data) => {
            if (data.fatal) fail(t("onlinePlayerError"));
          });
          hls.loadSource(session.streamUrl);
          hls.attachMedia(media);
        }
      } else media.src = session.streamUrl;
    } catch (cause) {
      if (!disposed) fail(apiErrorMessage(cause, t("onlinePlayerError")));
    } finally {
      clearTimeout(deadline);
    }
  });
  const handleCast = async () => {
    if (isCasting()) {
      castManager.stopCasting();
      return;
    }
    setCastConnecting(true);
    try {
      let streamUrl = session?.streamUrl || props.source.mediaUrl || props.source.url;
      const token = getStoredToken();
      if (session?.hls && token && !streamUrl.includes("token=")) {
        streamUrl = `${streamUrl}${streamUrl.includes("?") ? "&" : "?"}token=${encodeURIComponent(token)}`;
      }
      media?.pause?.();
      await castManager.requestCastSession(
        {
          contentId: streamUrl,
          title: props.source.title,
          subtitle: props.source.seriesTitle || undefined,
          currentTime: media && Number.isFinite(media.currentTime) ? media.currentTime : 0,
          duration: media && Number.isFinite(media.duration) ? media.duration : undefined,
          contentType: session?.hls ? "application/x-mpegurl" : "video/mp4",
        },
        media,
      );
    } catch {
      // Handled by castManager
    } finally {
      setCastConnecting(false);
    }
  };

  onCleanup(() => {
    disposed = true;
    abort.abort();
    clearTimeout(deadline);
    progress();
    release();
    stop();
  });
  return (
    <section
      ref={section}
      aria-label={t("onlinePlayerTitle")}
      style={{
        border: "1px solid var(--border)",
        padding: "12px",
        "border-radius": "var(--radius-md)",
      }}
    >
      <h3>{props.source.title}</h3>
      <p>{t("onlinePlayerHelp")}</p>
      <Show when={isCasting()}>
        <div
          style={{
            padding: "20px 16px",
            background: "var(--bg-base)",
            border: "1px solid var(--accent)",
            "border-radius": "var(--radius-md)",
            "text-align": "center",
            display: "flex",
            "flex-direction": "column",
            "align-items": "center",
            gap: "10px",
            "margin-bottom": "12px",
          }}
        >
          <div style={{ "font-size": "44px" }}>📺</div>
          <div style={{ "font-size": "17px", "font-weight": "bold" }}>
            Casting to {castDevice() || "Chromecast"}
          </div>
          <div style={{ color: "var(--text-secondary)", "font-size": "13px" }}>
            {props.source.title}
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                if (castManager.getState().playerState === "playing") {
                  castManager.pause();
                } else {
                  castManager.play();
                }
              }}
            >
              {castManager.getState().playerState === "playing" ? "⏸ Pause" : "▶ Play"}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => castManager.stopCasting()}>
              Disconnect Cast
            </Button>
          </div>
        </div>
      </Show>
      <video
        ref={media}
        controls
        playsinline
        preload="metadata"
        style={{ width: "100%", "max-height": "60dvh", background: "#000" }}
        onLoadedMetadata={() => {
          if (disposed || !session) return;
          if (Number.isFinite(media.duration) && session.position > 0)
            media.currentTime = Math.min(session.position, Math.max(0, media.duration - 1));
          clearTimeout(loadDeadline);
          setReady(true);
        }}
        onPlaying={() => {
          completed = false;
          hasPlayed = true;
          setPlaying(true);
          progress();
        }}
        onPause={() => {
          setPlaying(false);
          if (!sending) progress();
        }}
        onEnded={() => {
          completed = true;
          setPlaying(false);
          progress(true);
        }}
        onError={() => fail(t("onlinePlayerError"))}
      >
        <track kind="captions" />
      </video>
      <Show
        when={error()}
        fallback={
          <p role="status">
            {playing()
              ? t("onlinePlaying")
              : ready()
                ? t("onlinePreviewReady")
                : t("onlinePlayerLoading")}
          </p>
        }
      >
        <p role="alert">{error()}</p>
      </Show>
      <div style={{ display: "flex", gap: "8px", "align-items": "center", "margin-top": "12px" }}>
        <Show when={isCastAvailable() || (media && "remote" in media)}>
          <Button
            type="button"
            variant={isCasting() ? "primary" : "secondary"}
            disabled={castConnecting()}
            onClick={handleCast}
          >
            {isCasting()
              ? `📺 Connected to ${castDevice() || "TV"}`
              : castConnecting()
                ? "📺 Connecting..."
                : "📺 Cast to TV"}
          </Button>
        </Show>
        <Button type="button" onClick={props.onClose}>
          {t("close")}
        </Button>
      </div>
    </section>
  );
}
