import { useParams } from "@solidjs/router";
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import PhotoImage from "../components/PhotoImage";
import { Button } from "../components/ui";
import { Dialog } from "../components/ui/Dialog";

type Photo = components["schemas"]["PhotoSummary"];
type Bucket = components["schemas"]["PhotoTimelineBucket"];
export default function Photos() {
  const params = useParams();
  const id = () => Number(params.id);
  const [library, setLibrary] = createSignal<components["schemas"]["LibraryDto"] | null>(null);
  const [buckets, setBuckets] = createSignal<Bucket[]>([]);
  const [bucket, setBucket] = createSignal<Bucket | null>(null);
  const [photos, setPhotos] = createSignal<Photo[]>([]);
  const [cursor, setCursor] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [loadingPhotos, setLoadingPhotos] = createSignal(false);
  const [error, setError] = createSignal("");
  const [selected, setSelected] = createSignal<number | null>(null);
  const [slideshow, setSlideshow] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  let libraryAbort: AbortController | undefined, photoAbort: AbortController | undefined;
  let libraryVersion = 0,
    photoVersion = 0;
  const current = () => {
    const index = selected();
    return index === null ? null : (photos()[index] ?? null);
  };
  const close = () => {
    setSelected(null);
    setSlideshow(false);
    setReady(false);
  };
  const move = (direction: number) => {
    const index = selected();
    if (index === null) return;
    const next = index + direction;
    if (next < 0 || next >= photos().length) {
      setSlideshow(false);
      return;
    }
    setReady(false);
    setSelected(next);
  };
  const loadPhotos = async (value: Bucket, append = false) => {
    photoAbort?.abort();
    photoAbort = new AbortController();
    const version = ++photoVersion;
    setLoadingPhotos(true);
    setError("");
    if (!append) {
      close();
      setPhotos([]);
      setCursor(null);
    }
    setBucket(value);
    try {
      const result = await client.GET("/api/v1/libraries/{id}/photos", {
        params: {
          path: { id: id() },
          query: {
            year: value.year,
            month: value.month,
            cursor: append ? (cursor() ?? undefined) : undefined,
            limit: 60,
          },
        },
        signal: photoAbort.signal,
      });
      if (version !== photoVersion) return;
      if (result.error || !result.data)
        throw new Error("Could not load photos. Retry the selected month.");
      setPhotos((previous) => (append ? [...previous, ...result.data.items] : result.data.items));
      setCursor(result.data.nextCursor ?? null);
    } catch (reason) {
      if (version === photoVersion)
        setError(reason instanceof Error ? reason.message : "Could not load photos.");
    } finally {
      if (version === photoVersion) setLoadingPhotos(false);
    }
  };
  const load = async () => {
    libraryAbort?.abort();
    photoAbort?.abort();
    photoVersion++;
    libraryAbort = new AbortController();
    const version = ++libraryVersion;
    close();
    setLibrary(null);
    setBuckets([]);
    setBucket(null);
    setPhotos([]);
    setCursor(null);
    setLoadingPhotos(false);
    setLoading(true);
    setError("");
    try {
      const [info, timeline] = await Promise.all([
        client.GET("/api/v1/libraries/{id}", {
          params: { path: { id: id() } },
          signal: libraryAbort.signal,
        }),
        client.GET("/api/v1/libraries/{id}/timeline", {
          params: { path: { id: id() } },
          signal: libraryAbort.signal,
        }),
      ]);
      if (version !== libraryVersion) return;
      if (
        info.error ||
        timeline.error ||
        !info.data ||
        !timeline.data ||
        info.data.kind !== "photo"
      )
        throw new Error("Could not load photo library.");
      setLibrary(info.data);
      setBuckets(timeline.data);
      if (timeline.data[0]) void loadPhotos(timeline.data[0]);
    } catch (reason) {
      if (version === libraryVersion)
        setError(reason instanceof Error ? reason.message : "Could not load library.");
    } finally {
      if (version === libraryVersion) setLoading(false);
    }
  };
  createEffect(() => {
    id();
    void load();
  });
  onMount(() => {
    const key = (event: KeyboardEvent) => {
      if (
        !current() ||
        (event.target instanceof HTMLElement &&
          event.target.closest("input,select,textarea,[contenteditable='true']"))
      )
        return;
      if (event.key === "ArrowRight" || event.key === "ArrowLeft") {
        event.preventDefault();
        move(event.key === "ArrowRight" ? 1 : -1);
      }
    };
    window.addEventListener("keydown", key);
    const timer = setInterval(() => {
      if (slideshow() && ready() && !document.hidden) move(1);
    }, 5000);
    onCleanup(() => {
      window.removeEventListener("keydown", key);
      clearInterval(timer);
    });
  });
  onCleanup(() => {
    libraryVersion++;
    photoVersion++;
    libraryAbort?.abort();
    photoAbort?.abort();
  });
  return (
    <div class="mbl-page" style={{ display: "grid", gap: "18px" }}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "flex-wrap": "wrap",
          gap: "12px",
        }}
      >
        <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "700" }}>
          {library()?.name || "Photos"}
        </h1>
        <Button variant="secondary" disabled={loading()} onClick={load}>
          Refresh timeline
        </Button>
      </div>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={loading()}>
        <p role="status">Loading timeline…</p>
      </Show>
      <Show when={library()}>
        <Show
          when={buckets().length}
          fallback={
            <div class="mbl-card-section" style={{ color: "var(--text-muted)" }}>
              No visible photos in this library.
            </div>
          }
        >
          <div
            class="mbl-card-section"
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "flex-wrap": "wrap",
              gap: "12px",
              padding: "var(--space-3) var(--space-4)",
            }}
          >
            <div
              style={{ display: "flex", "align-items": "center", gap: "10px", "flex-wrap": "wrap" }}
            >
              <label
                for="photo-month"
                style={{
                  "font-size": "var(--font-size-sm)",
                  "font-weight": "var(--font-weight-semibold)",
                  color: "var(--text-secondary)",
                }}
              >
                Month
              </label>
              <select
                id="photo-month"
                value={bucket() ? `${bucket()?.year}-${bucket()?.month}` : ""}
                onChange={(event) => {
                  const value = buckets().find(
                    (entry) => `${entry.year}-${entry.month}` === event.currentTarget.value,
                  );
                  if (value) void loadPhotos(value);
                }}
              >
                <For each={buckets()}>
                  {(entry) => (
                    <option value={`${entry.year}-${entry.month}`}>
                      {entry.year}-{String(entry.month).padStart(2, "0")} ({entry.count})
                    </option>
                  )}
                </For>
              </select>
            </div>
            <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
              <span style={{ "font-size": "var(--font-size-sm)", color: "var(--text-secondary)" }}>
                {photos().length} of {bucket()?.count ?? 0} photos loaded
              </span>
              <Show when={bucket()}>
                {(value) => (
                  <Button
                    size="sm"
                    disabled={loadingPhotos()}
                    variant="ghost"
                    onClick={() => loadPhotos(value())}
                  >
                    Reload month
                  </Button>
                )}
              </Show>
            </div>
          </div>
          <div
            style={{
              display: "grid",
              "grid-template-columns": "repeat(auto-fill, minmax(160px, 1fr))",
              gap: "14px",
            }}
          >
            <For each={photos()}>
              {(photo, index) => (
                <article
                  class="mbl-card-section"
                  style={{
                    padding: "var(--space-2)",
                    display: "flex",
                    "flex-direction": "column",
                    gap: "6px",
                  }}
                >
                  <Show when={photo.partId != null} fallback={<p>Original file missing</p>}>
                    <PhotoImage partId={photo.partId as number} title={photo.title} />
                  </Show>
                  <Button
                    variant="ghost"
                    size="sm"
                    disabled={photo.partId == null}
                    onClick={() => {
                      setReady(false);
                      setSelected(index());
                    }}
                  >
                    {photo.title}
                  </Button>
                  <p
                    style={{
                      margin: 0,
                      "font-size": "var(--font-size-xs)",
                      color: "var(--text-muted)",
                      "text-align": "center",
                    }}
                  >
                    {new Date(photo.takenAt).toLocaleDateString()}
                  </p>
                </article>
              )}
            </For>
          </div>
          <Show when={loadingPhotos()}>
            <p role="status">Loading photos…</p>
          </Show>
          <Show when={cursor()}>
            <div>
              <Button
                variant="secondary"
                disabled={loadingPhotos()}
                onClick={() => {
                  const value = bucket();
                  if (value) void loadPhotos(value, true);
                }}
              >
                Load more photos
              </Button>
            </div>
          </Show>
        </Show>
      </Show>
      <Dialog
        open={selected() !== null}
        onOpenChange={(open) => {
          if (!open) close();
        }}
        title={current()?.title || "Photo"}
      >
        <Show keyed when={current()}>
          {(photo) => (
            <div style={{ display: "grid", gap: "12px" }}>
              <PhotoImage
                partId={photo.partId as number}
                title={photo.title}
                eager
                onReady={() => setReady(true)}
                onFailure={() => {
                  setSlideshow(false);
                  setReady(false);
                }}
              />
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-xs)",
                  color: "var(--text-muted)",
                }}
              >
                {new Date(photo.takenAt).toLocaleString()}
              </p>
              <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={selected() === 0}
                  onClick={() => move(-1)}
                >
                  Previous photo
                </Button>
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={selected() === photos().length - 1}
                  onClick={() => move(1)}
                >
                  Next photo
                </Button>
                <Button
                  size="sm"
                  disabled={!slideshow() && (!ready() || selected() === photos().length - 1)}
                  onClick={() => setSlideshow(!slideshow())}
                >
                  {slideshow() ? "Pause slideshow" : "Start slideshow"}
                </Button>
              </div>
              <p role="status" style={{ margin: 0 }}>
                {(selected() ?? 0) + 1} / {photos().length}
                {selected() === photos().length - 1 && cursor()
                  ? " — load more photos to continue"
                  : ""}
              </p>
            </div>
          )}
        </Show>
      </Dialog>
    </div>
  );
}
