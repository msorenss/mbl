import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { PosterCard } from "../components/PosterCard";

type SearchGroup = components["schemas"]["SearchGroup"];

export function Search() {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<SearchGroup[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [hasSearched, setHasSearched] = createSignal(false);

  let searchVersion = 0;
  let debounceTimer: number | null = null;

  const performSearch = async (searchTerm: string, version: number) => {
    const q = searchTerm.trim();
    if (!q) {
      setResults([]);
      setHasSearched(false);
      setLoading(false);
      return;
    }

    setLoading(true);
    setHasSearched(true);
    try {
      const { data } = await client.GET("/api/v1/search", {
        params: { query: { q } },
      });
      if (version === searchVersion) setResults(data ?? []);
    } catch {
      if (version === searchVersion) setResults([]);
    } finally {
      if (version === searchVersion) setLoading(false);
    }
  };

  createEffect(() => {
    const q = query();
    const version = ++searchVersion;
    if (debounceTimer) clearTimeout(debounceTimer);
    if (!q.trim()) {
      setResults([]);
      setHasSearched(false);
      setLoading(false);
      return;
    }

    debounceTimer = window.setTimeout(() => {
      performSearch(q, version);
    }, 300);
  });

  onCleanup(() => {
    searchVersion++;
    if (debounceTimer) clearTimeout(debounceTimer);
  });

  const totalResultsCount = () => results().reduce((acc, g) => acc + (g.items?.length || 0), 0);

  const formatKindTitle = (kind: string) => {
    switch (kind) {
      case "movie":
        return "Movies";
      case "show":
        return "TV Shows";
      case "season":
        return "Seasons";
      case "episode":
        return "Episodes";
      case "album":
        return "Albums";
      case "track":
        return "Tracks";
      case "extra":
        return "Extras";
      default:
        return kind.charAt(0).toUpperCase() + kind.slice(1);
    }
  };

  return (
    <div>
      {/* Header */}
      <div style={{ "margin-bottom": "24px" }}>
        <h1 style={{ margin: "0 0 6px", "font-size": "24px", "font-weight": "800" }}>Search</h1>
        <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
          Instant search across all media libraries, genres, and titles
        </p>
      </div>

      {/* Search Input Box */}
      <div style={{ "max-width": "640px", "margin-bottom": "32px", position: "relative" }}>
        <input
          type="text"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          placeholder="Search for movies, series, episodes..."
          autofocus
          style={{
            width: "100%",
            padding: "14px 18px 14px 44px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
            "font-size": "16px",
            outline: "none",
            "box-shadow": "0 4px 12px rgba(0, 0, 0, 0.2)",
          }}
        />
        <span
          style={{
            position: "absolute",
            left: "16px",
            top: "50%",
            transform: "translateY(-50%)",
            "font-size": "16px",
            color: "var(--text-muted)",
          }}
        >
          🔍
        </span>
        <Show when={query()}>
          <button
            type="button"
            onClick={() => setQuery("")}
            style={{
              position: "absolute",
              right: "14px",
              top: "50%",
              transform: "translateY(-50%)",
              background: "transparent",
              border: "none",
              color: "var(--text-muted)",
              cursor: "pointer",
              "font-size": "14px",
              padding: "4px",
            }}
          >
            ✕
          </button>
        </Show>
      </div>

      {/* Loading Indicator */}
      <Show when={loading()}>
        <div style={{ padding: "30px 0", color: "var(--text-muted)", "text-align": "center" }}>
          Searching...
        </div>
      </Show>

      {/* Search Results */}
      <Show when={!loading() && hasSearched()}>
        <Show
          when={totalResultsCount() > 0}
          fallback={
            <div
              style={{
                padding: "48px 24px",
                "text-align": "center",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                color: "var(--text-muted)",
              }}
            >
              No results found matching "{query()}".
            </div>
          }
        >
          <div style={{ display: "flex", "flex-direction": "column", gap: "36px" }}>
            <For each={results()}>
              {(group) => (
                <Show when={group.items && group.items.length > 0}>
                  <div>
                    <h2
                      style={{
                        margin: "0 0 16px",
                        "font-size": "18px",
                        "font-weight": "700",
                        display: "flex",
                        "align-items": "center",
                        gap: "8px",
                      }}
                    >
                      <span>{formatKindTitle(group.kind)}</span>
                      <span
                        style={{
                          "font-size": "12px",
                          color: "var(--text-muted)",
                          "font-weight": "500",
                        }}
                      >
                        ({group.items.length})
                      </span>
                    </h2>

                    <div
                      style={{
                        display: "grid",
                        "grid-template-columns": "repeat(auto-fill, minmax(150px, 1fr))",
                        gap: "16px",
                      }}
                    >
                      <For each={group.items}>{(item) => <PosterCard item={item} />}</For>
                    </div>
                  </div>
                </Show>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
export default Search;
