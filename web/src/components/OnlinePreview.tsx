import { createSignal, onCleanup, onMount, Show } from "solid-js";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Button } from "./ui/Button";

type Source = components["schemas"]["OnlineSource"];
export function OnlinePreview(props: { source: Source; onClose: () => void }) {
  const { t } = useI18n();
  const [ready, setReady] = createSignal(false);
  const [playing, setPlaying] = createSignal(false);
  const [failed, setFailed] = createSignal(false);
  let media: HTMLMediaElement | undefined;
  let section: HTMLElement | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  const stop = () => {
    if (media) {
      media.pause();
      media.removeAttribute("src");
      media.load();
    }
  };
  const loaded = () => {
    if (!disposed) {
      clearTimeout(deadline);
      setReady(true);
    }
  };
  const error = () => {
    if (!disposed) {
      clearTimeout(deadline);
      setPlaying(false);
      setFailed(true);
    }
  };
  onMount(() => {
    section?.scrollIntoView?.({ block: "nearest" });
    deadline = setTimeout(() => {
      error();
      stop();
    }, 15000);
  });
  onCleanup(() => {
    disposed = true;
    clearTimeout(deadline);
    stop();
  });
  return (
    <section
      ref={section}
      aria-label={t("onlinePreview")}
      style={{
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-md)",
        padding: "12px",
      }}
    >
      <strong>{props.source.title}</strong>
      <p>{t("onlinePreviewHelp")}</p>
      <Show
        when={props.source.hasVideo}
        fallback={
          <audio
            ref={(el) => {
              media = el;
            }}
            src={props.source.mediaUrl || undefined}
            controls
            preload="metadata"
            onLoadedMetadata={loaded}
            onLoadedData={loaded}
            onPlaying={() => {
              if (!disposed) setPlaying(true);
            }}
            onPause={() => setPlaying(false)}
            onEnded={() => setPlaying(false)}
            onError={error}
            style={{ width: "100%" }}
          >
            <track kind="captions" />
          </audio>
        }
      >
        <video
          ref={(el) => {
            media = el;
          }}
          src={props.source.mediaUrl || undefined}
          controls
          playsinline
          preload="metadata"
          onLoadedMetadata={loaded}
          onLoadedData={loaded}
          onPlaying={() => {
            if (!disposed) setPlaying(true);
          }}
          onPause={() => setPlaying(false)}
          onEnded={() => setPlaying(false)}
          onError={error}
          style={{ width: "100%", "max-height": "45dvh", background: "#000" }}
        >
          <track kind="captions" />
        </video>
      </Show>
      <Show when={failed()}>
        <p role="alert">{t("onlinePreviewError")}</p>
      </Show>
      <Show when={!failed()}>
        <p role="status">
          {playing()
            ? t("onlinePlaying")
            : ready()
              ? t("onlinePreviewReady")
              : t("onlinePreviewLoading")}
        </p>
      </Show>
      <Button type="button" variant="secondary" onClick={props.onClose}>
        {t("close")}
      </Button>
    </section>
  );
}
