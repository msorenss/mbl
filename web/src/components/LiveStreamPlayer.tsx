import Hls from "hls.js";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { Button } from "./ui";

/** Plays browser-compatible tuner URLs; MPEG-TS-only tuners still need a server HLS relay. */
export default function LiveStreamPlayer(props: {
  url: string;
  title: string;
  onClose: () => void;
}) {
  let video: HTMLVideoElement | undefined;
  let hls: Hls | null = null;
  const [error, setError] = createSignal("");
  onMount(() => {
    if (!video) return;
    const url = new URL(props.url, window.location.href);
    if (!["http:", "https:"].includes(url.protocol)) {
      setError("This channel does not have a supported HTTP stream.");
      return;
    }
    const play = () => video?.play().catch(() => {});
    if (url.pathname.endsWith(".m3u8") && Hls.isSupported()) {
      hls = new Hls();
      hls.loadSource(url.href);
      hls.attachMedia(video);
      hls.on(Hls.Events.MANIFEST_PARSED, play);
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (data.fatal) {
          setError(
            "The live stream could not be played. Check the source and browser compatibility.",
          );
          hls?.destroy();
          hls = null;
        }
      });
    } else {
      video.src = url.href;
      void play();
    }
  });
  onCleanup(() => {
    hls?.destroy();
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
        <p role="alert">
          {error()} MPEG-TS tuner streams require a browser-compatible HLS source or relay.
        </p>
      </Show>
      {/* biome-ignore lint/a11y/useMediaCaption: live source may provide captions in-band */}
      <video
        ref={video}
        controls
        playsinline
        style={{ width: "100%", "max-height": "60vh" }}
        onError={() => setError("This stream is unavailable or unsupported by your browser.")}
      />
      <Button onClick={props.onClose}>Close live player</Button>
    </section>
  );
}
