import { createSignal, For, type JSX, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button, Dialog, SegmentedControl } from "./ui";

type Metadata = components["schemas"]["ItemMetadata"];
type Candidate = components["schemas"]["ArtworkCandidate"];
type Hit = components["schemas"]["TmdbSearchHit"];
type Tab = "details" | "match" | "artwork";
type TextField =
  | "title"
  | "sortTitle"
  | "originalTitle"
  | "tagline"
  | "summary"
  | "contentRating"
  | "studio";

/** Lock name used by the server for each form field. */
const LOCK: Record<string, string> = {
  title: "title",
  originalTitle: "original_title",
  year: "year",
  tagline: "tagline",
  summary: "summary",
  contentRating: "content_rating",
  studio: "studio",
  genres: "genres",
};

const reason = (value: unknown, fallback: string) =>
  (value as { error?: { message?: string } } | undefined)?.error?.message?.replace(
    /^(validation|external service|unavailable): /,
    "",
  ) || fallback;

const label = { display: "grid", gap: "4px", "font-size": "13px" } as const;

/** Admin dialog: edit and lock fields, match to another TMDB entry, choose artwork. */
export default function MetadataEditor(props: { itemId: number; onSaved: () => void }) {
  const [open, setOpen] = createSignal(false);
  const [tab, setTab] = createSignal<Tab>("details");
  const [meta, setMeta] = createSignal<Metadata | null>(null);
  const [form, setForm] = createSignal<Record<string, string>>({});
  const [locks, setLocks] = createSignal<string[]>([]);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<Hit[] | null>(null);
  const [artKind, setArtKind] = createSignal<"poster" | "backdrop">("poster");
  const [art, setArt] = createSignal<Candidate[] | null>(null);
  const [artAvailable, setArtAvailable] = createSignal(true);
  const [current, setCurrent] = createSignal<string | null>(null);

  const fill = (m: Metadata) => {
    setMeta(m);
    setLocks(m.lockedFields);
    setForm({
      title: m.title,
      sortTitle: m.sortTitle,
      originalTitle: m.originalTitle ?? "",
      year: m.year ? String(m.year) : "",
      tagline: m.tagline ?? "",
      summary: m.summary ?? "",
      contentRating: m.contentRating ?? "",
      studio: m.studio ?? "",
      genres: m.genres.join(", "),
    });
    setQuery(m.title);
  };

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  };

  const openEditor = () => {
    setOpen(true);
    setTab("details");
    setHits(null);
    setArt(null);
    void run(async () => {
      const result = await client.GET("/api/v1/items/{id}/metadata", {
        params: { path: { id: props.itemId } },
      });
      if (!result.data) throw new Error(reason(result.error, "Could not load metadata."));
      fill(result.data);
    });
  };

  const setField = (name: string, value: string) => {
    setForm((f) => ({ ...f, [name]: value }));
    const lock = LOCK[name];
    if (lock && !locks().includes(lock)) setLocks((l) => [...l, lock]);
  };
  const toggleLock = (lock: string, on: boolean) =>
    setLocks((l) => (on ? [...l, lock] : l.filter((x) => x !== lock)));

  const original = (m: Metadata): Record<string, string> => ({
    title: m.title,
    sortTitle: m.sortTitle,
    originalTitle: m.originalTitle ?? "",
    year: m.year ? String(m.year) : "",
    tagline: m.tagline ?? "",
    summary: m.summary ?? "",
    contentRating: m.contentRating ?? "",
    studio: m.studio ?? "",
    genres: m.genres.join(", "),
  });

  const save = (e: Event) => {
    e.preventDefault();
    const m = meta();
    if (!m) return;
    void run(async () => {
      const before = original(m);
      const now = form();
      const changed = (name: string) => now[name].trim() !== before[name].trim();
      const body: components["schemas"]["UpdateItemMetadata"] = { lockedFields: locks() };
      for (const name of [
        "title",
        "sortTitle",
        "originalTitle",
        "tagline",
        "summary",
        "contentRating",
        "studio",
      ] as TextField[]) {
        if (changed(name)) body[name] = now[name];
      }
      if (changed("year")) {
        const year = now.year.trim() ? Number(now.year) : 0;
        if (!Number.isInteger(year)) throw new Error("Year must be a number.");
        body.year = year;
      }
      if (changed("genres")) {
        body.genres = now.genres
          .split(",")
          .map((g) => g.trim())
          .filter(Boolean);
      }
      const result = await client.PATCH("/api/v1/items/{id}/metadata", {
        params: { path: { id: props.itemId } },
        body,
      });
      if (!result.data) throw new Error(reason(result.error, "Could not save metadata."));
      fill(result.data);
      setNotice("Saved. Locked fields keep their values when metadata is refreshed.");
      props.onSaved();
    });
  };

  const search = (e: Event) => {
    e.preventDefault();
    const m = meta();
    if (!m || !query().trim()) return;
    void run(async () => {
      const result = await client.GET("/api/v1/system/tmdb/search", {
        params: { query: { query: query().trim(), kind: m.kind === "show" ? "tv" : "movie" } },
      });
      if (!result.data) throw new Error("TMDB search failed.");
      if (!result.data.configured) throw new Error("Add a TMDB API key in server settings first.");
      setHits(result.data.results);
    });
  };

  const useMatch = (hit: Hit) =>
    run(async () => {
      const result = await client.POST("/api/v1/items/{id}/match", {
        params: { path: { id: props.itemId } },
        body: { tmdbId: hit.id },
      });
      if (result.error) throw new Error(reason(result.error, "Could not start matching."));
      setNotice(`Matching to "${hit.title}". The details update in a moment.`);
      setTimeout(props.onSaved, 4000);
    });

  const loadArt = (kind: "poster" | "backdrop") => {
    setArtKind(kind);
    setArt(null);
    void run(async () => {
      const result = await client.GET("/api/v1/items/{id}/artwork", {
        params: { path: { id: props.itemId }, query: { kind } },
      });
      if (!result.data) throw new Error(reason(result.error, "Could not load images."));
      setArtAvailable(result.data.available);
      setCurrent(result.data.current ?? null);
      setArt(result.data.candidates);
    });
  };

  const choose = (url: string) =>
    run(async () => {
      const result = await client.PUT("/api/v1/items/{id}/artwork", {
        params: { path: { id: props.itemId } },
        body: { kind: artKind(), url },
      });
      if (result.error) throw new Error(reason(result.error, "Could not save the image."));
      setCurrent(url);
      setNotice("Saved. The image is downloaded in the background.");
      setTimeout(props.onSaved, 3000);
    });

  const lockBox = (name: string) => (
    <label style={{ display: "inline-flex", gap: "4px", "align-items": "center" }}>
      <input
        type="checkbox"
        checked={locks().includes(LOCK[name])}
        onChange={(e) => toggleLock(LOCK[name], e.currentTarget.checked)}
      />
      <span>Lock</span>
    </label>
  );

  const field = (name: string, text: string, input: JSX.Element) => (
    <div style={label}>
      <div style={{ display: "flex", "justify-content": "space-between", gap: "8px" }}>
        <span style={{ "font-weight": 600 }}>{text}</span>
        <Show when={LOCK[name]}>{lockBox(name)}</Show>
      </div>
      {input}
    </div>
  );
  const textInput = (name: string, text: string) =>
    field(
      name,
      text,
      <input value={form()[name] ?? ""} onInput={(e) => setField(name, e.currentTarget.value)} />,
    );

  return (
    <>
      <Button size="md" variant="outline" onClick={openEditor}>
        <span>✎</span>
        <span>Edit metadata</span>
      </Button>
      <Dialog open={open()} onOpenChange={setOpen} title="Edit metadata">
        <div style={{ display: "grid", gap: "14px", "min-width": "min(640px, 86vw)" }}>
          <SegmentedControl
            ariaLabel="Section"
            size="sm"
            value={tab()}
            onChange={(t) => {
              setTab(t);
              setError("");
              setNotice("");
              if (t === "artwork" && !art()) loadArt(artKind());
            }}
            options={[
              { value: "details", label: "Details" },
              ...(meta()?.canMatch
                ? [
                    { value: "match" as Tab, label: "Match" },
                    { value: "artwork" as Tab, label: "Images" },
                  ]
                : []),
            ]}
          />
          <Show when={error()}>
            <p role="alert" style={{ margin: 0, color: "var(--danger)" }}>
              {error()}
            </p>
          </Show>
          <Show when={notice()}>
            <p role="status" style={{ margin: 0 }}>
              {notice()}
            </p>
          </Show>

          <Show when={tab() === "details" && meta()}>
            <form onSubmit={save} style={{ display: "grid", gap: "12px" }}>
              {textInput("title", "Title")}
              {textInput("sortTitle", "Sort title")}
              {textInput("originalTitle", "Original title")}
              <div style={{ display: "grid", gap: "12px", "grid-template-columns": "1fr 1fr" }}>
                {field(
                  "year",
                  "Year",
                  <input
                    inputmode="numeric"
                    value={form().year ?? ""}
                    onInput={(e) => setField("year", e.currentTarget.value)}
                  />,
                )}
                {textInput("contentRating", "Content rating")}
              </div>
              {textInput("studio", "Studio")}
              {textInput("genres", "Genres (separated by commas)")}
              {textInput("tagline", "Tagline")}
              {field(
                "summary",
                "Summary",
                <textarea
                  rows={6}
                  value={form().summary ?? ""}
                  onInput={(e) => setField("summary", e.currentTarget.value)}
                />,
              )}
              <p style={{ margin: 0, "font-size": "12px", color: "var(--text-secondary)" }}>
                Fields you change are locked, so refreshing metadata keeps them. Clear a lock to let
                the next refresh replace the value.
              </p>
              <div>
                <Button type="submit" variant="primary" size="md" disabled={busy()}>
                  {busy() ? "Saving…" : "Save"}
                </Button>
              </div>
            </form>
          </Show>

          <Show when={tab() === "match"}>
            <form onSubmit={search} style={{ display: "flex", gap: "8px" }}>
              <input
                aria-label="Search TMDB"
                style={{ flex: 1 }}
                value={query()}
                onInput={(e) => setQuery(e.currentTarget.value)}
              />
              <Button type="submit" size="sm" variant="secondary" disabled={busy()}>
                Search
              </Button>
            </form>
            <Show when={hits()?.length === 0}>
              <p style={{ margin: 0 }}>No matches found.</p>
            </Show>
            <div style={{ display: "grid", gap: "8px", "max-height": "50vh", overflow: "auto" }}>
              <For each={hits() ?? []}>
                {(hit) => (
                  <div style={{ display: "flex", gap: "10px", "align-items": "center" }}>
                    <Show when={hit.posterUrl}>
                      <img src={hit.posterUrl ?? ""} alt="" width="46" loading="lazy" />
                    </Show>
                    <div style={{ flex: 1 }}>
                      <strong>{hit.title}</strong>
                      <Show when={hit.year}>{(y) => <span>{` (${y()})`}</span>}</Show>
                      <div style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
                        {`TMDB ${hit.id}`}
                        {meta()?.tmdbId === hit.id ? " · current" : ""}
                      </div>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={busy()}
                      onClick={() => useMatch(hit)}
                    >
                      Use this
                    </Button>
                  </div>
                )}
              </For>
            </div>
          </Show>

          <Show when={tab() === "artwork"}>
            <SegmentedControl
              ariaLabel="Image type"
              size="sm"
              value={artKind()}
              onChange={(k) => loadArt(k)}
              options={[
                { value: "poster", label: "Poster" },
                { value: "backdrop", label: "Backdrop" },
              ]}
            />
            <Show when={art() && !artAvailable()}>
              <p style={{ margin: 0 }}>
                Images come from TMDB. Add a TMDB API key and match the title first.
              </p>
            </Show>
            <Show when={art()?.length === 0 && artAvailable()}>
              <p style={{ margin: 0 }}>TMDB has no other images for this title.</p>
            </Show>
            <div
              style={{
                display: "grid",
                gap: "8px",
                "grid-template-columns": `repeat(auto-fill, minmax(${artKind() === "poster" ? 110 : 200}px, 1fr))`,
                "max-height": "55vh",
                overflow: "auto",
              }}
            >
              <For each={art() ?? []}>
                {(c) => (
                  <button
                    type="button"
                    disabled={busy()}
                    aria-pressed={current() === c.url}
                    onClick={() => choose(c.url)}
                    style={{
                      padding: 0,
                      border:
                        current() === c.url ? "3px solid var(--accent)" : "1px solid var(--border)",
                      "border-radius": "var(--radius-sm)",
                      overflow: "hidden",
                      background: "none",
                      cursor: "pointer",
                    }}
                  >
                    <img
                      src={c.previewUrl}
                      alt={c.language ? `Image (${c.language})` : "Image"}
                      loading="lazy"
                      style={{ width: "100%", display: "block" }}
                    />
                  </button>
                )}
              </For>
            </div>
          </Show>
        </div>
      </Dialog>
    </>
  );
}
