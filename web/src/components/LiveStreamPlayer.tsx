import Hls from "hls.js";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { createHlsRecovery, hlsLoadPolicy } from "../utils/hlsRecovery";
import { Button } from "./ui";

/** The authenticated lease resolves to a same-origin server HLS relay. */
export default function LiveStreamPlayer(props: {
  url: string;
  title: string;
  onClose: () => void;
  onFailure?: (message: string) => void;
  onEnded?: () => void;
}) {
  let video: HTMLVideoElement | undefined;
  let hls: Hls | null = null;
  let recovery: ReturnType<typeof createHlsRecovery> | null = null;
  let disposed = false;
  let wantsPlay = true;
  let lastTime = 0;
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const fail = (message: string) => {
    if (disposed || error()) return;
    setError(message);
    recovery?.dispose();
    hls?.stopLoad();
    video?.pause();
    props.onFailure?.(message);
  };
  onMount(() => {
    if (!video) return;
    const url = new URL(props.url, window.location.href);
    if (!["http:", "https:"].includes(url.protocol)) {
      fail("This channel does not have a supported HTTP stream.");
      return;
    }
    const play = () => {
      if (!disposed && !error() && wantsPlay) void video?.play().catch(() => {});
    };
    if (url.pathname.endsWith(".m3u8") && Hls.isSupported()) {
      const instance = new Hls({
        manifestLoadPolicy: hlsLoadPolicy(),
        playlistLoadPolicy: hlsLoadPolicy(),
        fragLoadPolicy: hlsLoadPolicy(),
      });
      hls = instance;
      recovery = createHlsRecovery({
        active: () => !disposed && hls === instance && !error(),
        stop: () => instance.stopLoad(),
        notice: setNotice,
        retry: (reloadManifest) => {
          if (reloadManifest) instance.loadSource(url.href);
          if (!error()) instance.startLoad(); // live edge, not an expired VOD position
        },
        fail: (reason) =>
          fail(
            reason === "expired"
              ? "Live stream expired. Select the channel again."
              : reason === "access"
                ? "Live stream access was denied. Select the channel again."
                : "The live stream could not be played. Select the channel again or check the tuner source.",
          ),
      });
      instance.on(Hls.Events.MANIFEST_PARSED, play);
      instance.on(Hls.Events.ERROR, (_event, data) => recovery?.error(data));
      instance.on(Hls.Events.FRAG_BUFFERED, () => recovery?.progress());
      instance.loadSource(url.href);
      if (!error()) instance.attachMedia(video);
    } else {
      video.src = url.href;
      play();
    }
  });
  onCleanup(() => {
    disposed = true;
    recovery?.dispose();
    hls?.destroy();
    hls = null;
    video?.pause();
    video?.removeAttribute("src");
  });
  return (
    <section
      aria-label="Live player"
      style={{ padding: "16px", background: "var(--bg-surface)", "margin-bottom": "20px" }}
    >
      <h2>{props.title}</h2>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={notice()}>
        <p role="status">{notice()}</p>
      </Show>
      {/* biome-ignore lint/a11y/useMediaCaption: live source may provide captions in-band */}
      <video
        ref={video}
        controls
        playsinline
        style={{ width: "100%", "max-height": "60vh" }}
        onPlay={() => {
          wantsPlay = true;
        }}
        onPause={() => {
          wantsPlay = false;
        }}
        onTimeUpdate={(event) => {
          const v = event.currentTarget;
          if (!v.paused && !v.seeking && v.currentTime > lastTime + 0.25) recovery?.progress();
          lastTime = v.currentTime;
        }}
        onError={() => fail("This live stream is unavailable or unsupported by your browser.")}
        onEnded={() => {
          if (!disposed && !error()) props.onEnded?.();
        }}
      />
      <Button onClick={props.onClose}>Close live player</Button>
    </section>
  );
}
