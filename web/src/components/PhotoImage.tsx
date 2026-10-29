import { createSignal, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
export default function PhotoImage(props: {
  partId: number;
  title: string;
  eager?: boolean;
  onReady?: () => void;
  onFailure?: () => void;
}) {
  let element: HTMLDivElement | undefined;
  let observer: IntersectionObserver | undefined;
  const abort = new AbortController();
  const [url, setUrl] = createSignal("");
  const [error, setError] = createSignal(false);
  let loading = false,
    disposed = false;
  const load = async () => {
    if (loading || url() || disposed) return;
    loading = true;
    setError(false);
    try {
      const result = await client.GET("/api/v1/stream/direct/{partId}", {
        params: { path: { partId: props.partId } },
        parseAs: "blob",
        signal: abort.signal,
      });
      if (disposed) return;
      if (result.error || !result.data) throw new Error("Photo unavailable");
      setUrl(URL.createObjectURL(result.data));
    } catch {
      if (!disposed) {
        setError(true);
        props.onFailure?.();
      }
    } finally {
      loading = false;
    }
  };
  const failed = () => {
    const old = url();
    setUrl("");
    if (old) URL.revokeObjectURL(old);
    setError(true);
    props.onFailure?.();
  };
  onMount(() => {
    if (props.eager || typeof IntersectionObserver === "undefined") void load();
    else {
      observer = new IntersectionObserver((entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          observer?.disconnect();
          void load();
        }
      });
      if (element) observer.observe(element);
    }
  });
  onCleanup(() => {
    disposed = true;
    abort.abort();
    observer?.disconnect();
    if (url()) URL.revokeObjectURL(url());
  });
  return (
    <div ref={element} style={{ "min-height": "100px" }}>
      <Show
        when={!error()}
        fallback={
          <div role="alert">
            Photo unavailable or unsupported by this browser.{" "}
            <button type="button" onClick={load}>
              Retry image
            </button>
          </div>
        }
      >
        <Show when={url()} fallback={<span role="status">Loading {props.title}…</span>}>
          <img
            src={url()}
            alt={props.title}
            onLoad={() => props.onReady?.()}
            onError={failed}
            style={{
              width: "100%",
              "max-height": props.eager ? "60vh" : "180px",
              "object-fit": props.eager ? "contain" : "cover",
              "image-orientation": "from-image",
            }}
          />
        </Show>
      </Show>
    </div>
  );
}
