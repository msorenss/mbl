import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";
import { usePlayer } from "../context/PlayerContext";
import { formatClock } from "./Remote";

type Hit = components["schemas"]["SceneSearchHit"];
type Library = components["schemas"]["LibraryDto"];
type Benchmark = components["schemas"]["HailoEmbeddingBenchmark"];

function AdminTools() {
  const [libraries, setLibraries] = createSignal<Library[]>([]);
  const [library, setLibrary] = createSignal<number | null>(null);
  const [notice, setNotice] = createSignal("");
  const [benchmark, setBenchmark] = createSignal<Benchmark | null>(null);
  const [busy, setBusy] = createSignal(false);

  void client
    .GET("/api/v1/libraries")
    .then(({ data }) => {
      const list = (data ?? []).filter(
        (l) => l.kind === "movie" || l.kind === "show" || l.kind === "homevideo",
      );
      setLibraries(list);
      setLibrary(list[0]?.id ?? null);
    })
    .catch(() => undefined);

  const index = async () => {
    const id = library();
    if (id === null) return;
    setBusy(true);
    setNotice("");
    try {
      const result = await client.POST("/api/v1/libraries/{id}/index-scenes", {
        params: { path: { id } },
      });
      if (!result.data) {
        setNotice("Could not start scene indexing.");
        return;
      }
      setNotice(
        result.data.pendingItems === 0
          ? "Every title in this library is already indexed."
          : `Indexing ${result.data.pendingItems} titles in the background. It pauses while something is playing.`,
      );
    } catch {
      setNotice("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const runBenchmark = async () => {
    setBusy(true);
    setBenchmark(null);
    try {
      const result = await client.GET("/api/v1/ai/benchmark");
      if (result.data) setBenchmark(result.data);
      else setNotice("Could not run the speed test.");
    } catch {
      setNotice("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="mbl-card-section">
      <h2 style={{ "font-size": "1.1rem" }}>Index titles</h2>
      <p style={{ color: "var(--color-text-muted)" }}>
        Search only finds scenes in titles that have been indexed. Indexing uses the AI accelerator
        when one is installed.
      </p>
      <Show when={libraries().length > 0} fallback={<p>No video library to index.</p>}>
        <label>
          Library{" "}
          <select
            value={library() ?? ""}
            onChange={(e) => setLibrary(Number(e.currentTarget.value))}
          >
            <For each={libraries()}>{(l) => <option value={l.id}>{l.name}</option>}</For>
          </select>
        </label>{" "}
        <Button onClick={index} disabled={busy()}>
          Index scenes
        </Button>{" "}
        <Button variant="secondary" onClick={runBenchmark} disabled={busy()}>
          Test AI speed
        </Button>
      </Show>
      <Show when={notice()}>
        <p role="status">{notice()}</p>
      </Show>
      <Show when={benchmark()}>
        {(b) => (
          <p role="status">
            {`${b().backend}: ${Math.round(b().framesPerSec)} frames per second, ${Math.round(b().avgLatencyUs)} µs per frame`}
          </p>
        )}
      </Show>
    </section>
  );
}

/** Find a scene by describing it, then start playback right there. */
export default function Discover() {
  const { user } = useAuth();
  const { playItem } = usePlayer();
  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<Hit[] | null>(null);
  const [error, setError] = createSignal("");
  const [searching, setSearching] = createSignal(false);

  const search = async (event: Event) => {
    event.preventDefault();
    const q = query().trim();
    if (!q) return;
    setSearching(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/ai/search-scenes", {
        params: { query: { q, limit: 20 } },
      });
      if (!result.data) {
        setError("Search failed.");
        return;
      }
      setHits(result.data);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setSearching(false);
    }
  };

  return (
    <div class="mbl-page" style={{ "max-width": "760px" }}>
      <h1>Scene search</h1>
      <p style={{ color: "var(--color-text-muted)" }}>
        Describe what happens in a scene, for example "car chase at night" or "dog on a beach".
      </p>
      <form onSubmit={search} style={{ display: "flex", gap: "0.5rem" }}>
        <input
          type="search"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          placeholder="Describe a scene"
          aria-label="Describe a scene"
          style={{ flex: 1 }}
        />
        <Button type="submit" disabled={searching() || !query().trim()}>
          Search
        </Button>
      </form>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={hits()}>
        {(list) => (
          <Show
            when={list().length > 0}
            fallback={<p>No matching scenes. Titles must be indexed before they can be found.</p>}
          >
            <ul style={{ "list-style": "none", padding: 0 }}>
              <For each={list()}>
                {(hit) => (
                  <li class="mbl-card-section">
                    <strong>{hit.itemTitle}</strong> · {formatClock(hit.timestampMs)}
                    <p style={{ margin: "0.25rem 0" }}>{hit.caption}</p>
                    <Show when={hit.tags.length > 0}>
                      <small style={{ color: "var(--color-text-muted)" }}>
                        {hit.tags.join(", ")}
                      </small>
                    </Show>
                    <div>
                      <Button
                        size="sm"
                        onClick={() =>
                          playItem({
                            itemId: hit.itemId,
                            initialStartMs: hit.timestampMs,
                            title: hit.itemTitle,
                          })
                        }
                      >
                        Play from here
                      </Button>
                    </div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        )}
      </Show>
      <Show when={user()?.isAdmin}>
        <AdminTools />
      </Show>
    </div>
  );
}
