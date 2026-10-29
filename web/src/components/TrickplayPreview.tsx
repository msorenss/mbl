import { createEffect, createMemo, createSignal, onCleanup, Show, untrack } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";

type Manifest = components["schemas"]["TrickplayManifest"];

/** Mounted only while scrubbing; authenticated requests and blob URLs die with the preview. */
export default function TrickplayPreview(props: { partId: number; timeMs: number }) {
  const [manifest, setManifest] = createSignal<Manifest | null>(null);
  const [imageUrl, setImageUrl] = createSignal("");
  const [status, setStatus] = createSignal("Loading preview…");
  const [failed, setFailed] = createSignal(false);
  let manifestAbort: AbortController | undefined;
  let spriteAbort: AbortController | undefined;
  let poll: ReturnType<typeof setTimeout> | undefined;
  let disposed = false;
  let polls = 0;
  const releaseImage = () => {
    const old = untrack(imageUrl);
    setImageUrl("");
    if (old) URL.revokeObjectURL(old);
  };
  const load = async () => {
    manifestAbort?.abort();
    if (poll) clearTimeout(poll);
    manifestAbort = new AbortController();
    const signal = manifestAbort.signal;
    setFailed(false);
    try {
      const result = await client.GET("/api/v1/stream/trickplay/{partId}/index.json", {
        params: { path: { partId: props.partId } },
        signal,
      });
      if (disposed || signal.aborted) return;
      if (result.response.status === 202) {
        setStatus("Previews queued for idle generation.");
        if (++polls < 12) poll = setTimeout(() => void load(), 5000);
        else setFailed(true);
        return;
      }
      if (result.error || result.response.status !== 200 || !result.data) throw new Error();
      const data = result.data as Manifest;
      if (!data.tiles?.length || data.intervalMs <= 0 || data.partId !== props.partId)
        throw new Error();
      setManifest(data);
    } catch {
      if (!disposed && !signal.aborted) {
        setStatus("Preview unavailable.");
        setFailed(true);
      }
    }
  };
  createEffect(() => {
    // A part change invalidates both the manifest and every downloaded sheet.
    void props.partId;
    spriteAbort?.abort();
    releaseImage();
    setManifest(null);
    setStatus("Loading preview…");
    polls = 0;
    void load();
  });
  const tile = createMemo(() => {
    const data = manifest();
    if (!data) return null;
    const index = Math.max(0, Math.floor(props.timeMs / data.intervalMs));
    return data.tiles[Math.min(index, data.tiles.length - 1)] ?? null;
  });
  const spriteName = createMemo(() => tile()?.spriteFile);
  createEffect(() => {
    const name = spriteName();
    const partId = props.partId;
    spriteAbort?.abort();
    releaseImage();
    if (!name || !/^sprite_\d+\.jpg$/.test(name)) return;
    spriteAbort = new AbortController();
    const signal = spriteAbort.signal;
    setStatus("Loading preview…");
    void (async () => {
      try {
        const result = await client.GET("/api/v1/stream/trickplay/{partId}/{sprite}", {
          params: { path: { partId, sprite: name } },
          parseAs: "blob",
          signal,
        });
        if (disposed || signal.aborted) return;
        if (result.error || !result.data) throw new Error();
        setImageUrl(URL.createObjectURL(result.data));
      } catch {
        if (!disposed && !signal.aborted) {
          setStatus("Preview unavailable.");
          setFailed(true);
        }
      }
    })();
  });
  onCleanup(() => {
    disposed = true;
    manifestAbort?.abort();
    spriteAbort?.abort();
    if (poll) clearTimeout(poll);
    releaseImage();
  });
  return (
    <div
      class="trickplay-preview"
      style={{
        width: "160px",
        color: "white",
        background: "#111",
        padding: "4px",
        "border-radius": "4px",
      }}
    >
      <Show
        when={imageUrl() && tile() && manifest()}
        fallback={<span role="status">{status()}</span>}
      >
        <div
          role="img"
          aria-label="Video preview"
          style={{
            width: "160px",
            height: "90px",
            "background-image": `url(${imageUrl()})`,
            "background-position": `-${(tile()?.x ?? 0) / 2}px -${(tile()?.y ?? 0) / 2}px`,
            "background-size": `${((manifest()?.columns ?? 10) * (manifest()?.tileWidth ?? 320)) / 2}px ${((manifest()?.rows ?? 10) * (manifest()?.tileHeight ?? 180)) / 2}px`,
          }}
        />
      </Show>
      <div>
        {Math.floor(props.timeMs / 60000)}:
        {String(Math.floor(props.timeMs / 1000) % 60).padStart(2, "0")}
      </div>
      <Show when={failed()}>
        <small>Hover or focus the timeline again to retry.</small>
      </Show>
    </div>
  );
}
