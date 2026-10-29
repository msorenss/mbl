import { useNavigate, useParams } from "@solidjs/router";
import { createVirtualizer } from "@tanstack/solid-virtual";
import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { PosterCard } from "../components/PosterCard";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";

type ItemSummary = components["schemas"]["ItemSummary"];
type LibraryDto = components["schemas"]["LibraryDto"];
type SortField = components["schemas"]["SortField"];
type SortOrder = components["schemas"]["SortOrder"];

export function LibraryDetail() {
  const params = useParams();
  const libraryId = () => Number(params.id);
  const { user } = useAuth();
  const { showToast } = useToast();
  const navigate = useNavigate();

  let parentRef: HTMLDivElement | undefined;

  const [library, setLibrary] = createSignal<LibraryDto | null>(null);
  const [items, setItems] = createSignal<ItemSummary[]>([]);
  const [nextCursor, setNextCursor] = createSignal<string | null>(null);
  const [loadError, setLoadError] = createSignal<string | null>(null);
  let requestVersion = 0;
  const [loading, setLoading] = createSignal(true);
  const [isScanning, setIsScanning] = createSignal(false);
  const [sortField, setSortField] = createSignal<SortField>("title");
  const [sortOrder, setSortOrder] = createSignal<SortOrder>("asc");
  const [unwatchedOnly, setUnwatchedOnly] = createSignal(false);
  const [containerWidth, setContainerWidth] = createSignal(1000);

  // Column calculation based on available width: min 160px per poster + 16px gap
  const columnCount = createMemo(() => {
    const width = containerWidth();
    return Math.max(2, Math.floor((width + 16) / (160 + 16)));
  });

  // Chunk items into rows for virtualizer
  const rows = createMemo(() => {
    const list = items();
    const cols = columnCount();
    const result: ItemSummary[][] = [];
    for (let i = 0; i < list.length; i += cols) {
      result.push(list.slice(i, i + cols));
    }
    return result;
  });

  const rowVirtualizer = createVirtualizer({
    get count() {
      return rows().length;
    },
    getScrollElement: () => parentRef || null,
    estimateSize: () => 290, // poster height ~240px + title ~40px + gap 10px
    overscan: 3,
  });

  const loadLibrary = async () => {
    try {
      const { data } = await client.GET("/api/v1/libraries/{id}", {
        params: { path: { id: libraryId() } },
      });
      if (data && data.id === libraryId()) setLibrary(data);
    } catch {
      // Ignore
    }
  };

  const loadItems = async (append = false) => {
    const version = ++requestVersion;
    const id = libraryId();
    const cursor = append ? nextCursor() : null;
    const sort = sortField();
    const order = sortOrder();
    const unwatched = unwatchedOnly();
    setLoading(true);
    setLoadError(null);
    if (!append) {
      setItems([]);
      setNextCursor(null);
    }
    try {
      const { data, error } = await client.GET("/api/v1/libraries/{id}/items", {
        params: {
          path: { id },
          query: {
            sort,
            order,
            unwatched: unwatched || undefined,
            limit: 200,
            cursor: cursor ?? undefined,
          },
        },
      });
      if (version !== requestVersion) return;
      if (error || !data) throw new Error("Could not load library items.");
      setItems((previous) => (append ? [...previous, ...data.items] : data.items));
      setNextCursor(data.nextCursor ?? null);
    } catch {
      if (version === requestVersion) setLoadError("Could not load library items. Please retry.");
    } finally {
      if (version === requestVersion) setLoading(false);
    }
  };

  const handleScan = async () => {
    if (isScanning()) return;
    setIsScanning(true);
    try {
      const { data, error } = await client.POST("/api/v1/libraries/{id}/scan", {
        params: { path: { id: libraryId() } },
      });
      if (data && !error) {
        showToast({
          title: "Scan Started",
          message: `Library scan queued (Job #${data.jobId})`,
          type: "success",
        });
      } else {
        showToast({
          title: "Scan Failed",
          message: error ? String(error) : "Failed to queue scan",
          type: "danger",
        });
      }
    } catch (e) {
      showToast({
        title: "Scan Error",
        message: e instanceof Error ? e.message : "Network error",
        type: "danger",
      });
    } finally {
      setIsScanning(false);
    }
  };

  onMount(() => {
    const updateWidth = () => {
      if (parentRef) {
        setContainerWidth(parentRef.clientWidth);
      }
    };

    updateWidth();
    window.addEventListener("resize", updateWidth);

    onCleanup(() => {
      window.removeEventListener("resize", updateWidth);
    });
  });

  createEffect(() => {
    libraryId();
    setLibrary(null);
    void loadLibrary();
  });

  // Reload when sort or filter changes
  createEffect(() => {
    sortField();
    sortOrder();
    unwatchedOnly();
    void loadItems();
  });

  return (
    <div style={{ display: "flex", "flex-direction": "column", height: "100%" }}>
      <Show when={loadError()}>
        <p role="alert">
          {loadError()}{" "}
          <button type="button" onClick={() => loadItems()}>
            Retry
          </button>
        </p>
      </Show>
      <Show when={nextCursor()}>
        <button type="button" disabled={loading()} onClick={() => loadItems(true)}>
          {loading() ? "Loading…" : "Load more items"}
        </button>
      </Show>
      {/* Header bar */}
      <div
        style={{
          display: "flex",
          "align-items": "flex-start",
          "justify-content": "space-between",
          "margin-bottom": "20px",
          "flex-shrink": 0,
        }}
      >
        <div>
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <button
              type="button"
              onClick={() => navigate("/libraries")}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                "font-size": "18px",
                cursor: "pointer",
                padding: "2px 6px",
              }}
              title="Back to libraries"
            >
              &larr;
            </button>
            <Show when={library()?.kind === "photo"}>
              <button type="button" onClick={() => navigate(`/photos/${libraryId()}`)}>
                Open photo timeline
              </button>
            </Show>
            <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "800" }}>
              {library()?.name || "Library"}
            </h1>
            <span
              style={{
                "font-size": "11px",
                padding: "2px 8px",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "10px",
                color: "var(--text-muted)",
                "text-transform": "capitalize",
              }}
            >
              {library()?.kind || "media"}
            </span>
          </div>
          <p
            style={{ margin: "4px 0 0 28px", color: "var(--text-secondary)", "font-size": "13px" }}
          >
            {items().length} {items().length === 1 ? "item" : "items"} in library
          </p>
        </div>

        {/* Scan Button (Admin) */}
        <Show when={user()?.isAdmin}>
          <button
            type="button"
            onClick={handleScan}
            disabled={isScanning()}
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              padding: "9px 16px",
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              color: "var(--text-primary)",
              "border-radius": "var(--radius-sm)",
              "font-size": "13px",
              "font-weight": "600",
              cursor: isScanning() ? "not-allowed" : "pointer",
              opacity: isScanning() ? 0.6 : 1,
            }}
          >
            <span>🔄</span>
            <span>{isScanning() ? "Scanning..." : "Scan Library"}</span>
          </button>
        </Show>
      </div>

      {/* Filter and Sort Toolbar */}
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "12px 16px",
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-md)",
          "margin-bottom": "20px",
          "flex-shrink": 0,
          gap: "16px",
          "flex-wrap": "wrap",
        }}
      >
        <div style={{ display: "flex", "align-items": "center", gap: "12px" }}>
          {/* Sort Dropdown */}
          <div style={{ display: "flex", "align-items": "center", gap: "6px" }}>
            <span style={{ "font-size": "13px", color: "var(--text-secondary)" }}>Sort:</span>
            <select
              value={sortField()}
              onChange={(e) => setSortField(e.currentTarget.value as SortField)}
              style={{
                padding: "6px 10px",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "13px",
                outline: "none",
                cursor: "pointer",
              }}
            >
              <option value="title">Title</option>
              <option value="addedAt">Date Added</option>
              <option value="year">Release Year</option>
              <option value="rating">Rating</option>
            </select>
          </div>

          {/* Sort Order Toggle */}
          <button
            type="button"
            onClick={() => setSortOrder(sortOrder() === "asc" ? "desc" : "asc")}
            title={
              sortOrder() === "asc"
                ? "Ascending (click for descending)"
                : "Descending (click for ascending)"
            }
            style={{
              padding: "6px 10px",
              background: "var(--bg-card)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-sm)",
              color: "var(--text-primary)",
              "font-size": "13px",
              cursor: "pointer",
            }}
          >
            {sortOrder() === "asc" ? "▲ Asc" : "▼ Desc"}
          </button>
        </div>

        {/* Unwatched Filter Toggle */}
        <label
          style={{
            display: "flex",
            "align-items": "center",
            gap: "8px",
            "font-size": "13px",
            color: "var(--text-secondary)",
            cursor: "pointer",
          }}
        >
          <input
            type="checkbox"
            checked={unwatchedOnly()}
            onChange={(e) => setUnwatchedOnly(e.currentTarget.checked)}
            style={{ "accent-color": "var(--accent)" }}
          />
          <span>Unwatched only</span>
        </label>
      </div>

      {/* Virtualized Poster Grid Container */}
      <div
        ref={parentRef}
        style={{
          flex: 1,
          overflow: "auto",
          position: "relative",
        }}
      >
        <Show
          when={!loading()}
          fallback={
            <div style={{ padding: "40px 0", color: "var(--text-muted)", "text-align": "center" }}>
              Loading items...
            </div>
          }
        >
          <Show
            when={items().length > 0}
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
                No media items match your current filters.
              </div>
            }
          >
            <div
              style={{
                height: `${rowVirtualizer.getTotalSize()}px`,
                width: "100%",
                position: "relative",
              }}
            >
              <For each={rowVirtualizer.getVirtualItems()}>
                {(virtualRow) => {
                  const rowItems = rows()[virtualRow.index] || [];
                  return (
                    <div
                      style={{
                        position: "absolute",
                        top: 0,
                        left: 0,
                        width: "100%",
                        height: `${virtualRow.size}px`,
                        transform: `translateY(${virtualRow.start}px)`,
                        display: "grid",
                        "grid-template-columns": `repeat(${columnCount()}, minmax(0, 1fr))`,
                        gap: "16px",
                        padding: "0 4px",
                      }}
                    >
                      <For each={rowItems}>{(item) => <PosterCard item={item} />}</For>
                    </div>
                  );
                }}
              </For>
            </div>
          </Show>
        </Show>
      </div>
    </div>
  );
}
export default LibraryDetail;
