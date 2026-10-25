import { useNavigate, useParams } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { usePlayer } from "../context/PlayerContext";
import { useToast } from "../context/ToastContext";

type ItemDetailDto = components["schemas"]["ItemDetail"];
type ItemSummary = components["schemas"]["ItemSummary"];

export function ItemDetail() {
  const params = useParams();
  const itemId = () => Number(params.id);
  const { playItem } = usePlayer();
  const { showToast } = useToast();
  const navigate = useNavigate();

  const [item, setItem] = createSignal<ItemDetailDto | null>(null);
  const [children, setChildren] = createSignal<ItemSummary[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [watchUpdating, setWatchUpdating] = createSignal(false);

  const loadData = async () => {
    setLoading(true);
    try {
      const [detailRes, childrenRes] = await Promise.all([
        client.GET("/api/v1/items/{id}", {
          params: { path: { id: itemId() } },
        }),
        client.GET("/api/v1/items/{id}/children", {
          params: { path: { id: itemId() } },
        }),
      ]);

      if (detailRes.data) setItem(detailRes.data);
      if (childrenRes.data) setChildren(childrenRes.data);
    } catch {
      // Ignore
    } finally {
      setLoading(false);
    }
  };

  onMount(loadData);

  const formatDuration = (ms?: number | null) => {
    if (!ms || ms <= 0) return null;
    const totalSecs = Math.floor(ms / 1000);
    const hours = Math.floor(totalSecs / 3600);
    const mins = Math.floor((totalSecs % 3600) / 60);
    if (hours > 0) return `${hours}h ${mins}m`;
    return `${mins}m`;
  };

  const backdropUrl = () => {
    const art = item()?.artwork.find((a) => a.kind === "backdrop");
    return art ? `/api/v1/images/${art.id}?w=1280` : null;
  };

  const posterUrl = () => {
    const art = item()?.artwork.find((a) => a.kind === "poster");
    return art ? `/api/v1/images/${art.id}?w=500` : null;
  };

  const isWatched = () => (item()?.watch.viewCount ?? 0) > 0;
  const viewOffsetMs = () => item()?.watch.viewOffsetMs ?? 0;

  const handlePlay = (startOffset = 0) => {
    const it = item();
    if (!it) return;
    playItem({
      itemId: it.id,
      initialStartMs: startOffset,
      title: it.title,
      subtitle: it.originallyAvailableAt ? it.originallyAvailableAt.substring(0, 4) : undefined,
    });
  };

  const handleToggleWatched = async () => {
    if (watchUpdating()) return;
    setWatchUpdating(true);

    try {
      if (isWatched()) {
        await client.DELETE("/api/v1/items/{id}/watched", {
          params: { path: { id: itemId() } },
        });
        showToast({ title: "Marked Unwatched", message: item()?.title || "", type: "info" });
      } else {
        await client.POST("/api/v1/items/{id}/watched", {
          params: { path: { id: itemId() } },
        });
        showToast({ title: "Marked Watched", message: item()?.title || "", type: "success" });
      }
      await loadData();
    } catch (e) {
      showToast({
        title: "Action Failed",
        message: e instanceof Error ? e.message : "Error updating watch state",
        type: "danger",
      });
    } finally {
      setWatchUpdating(false);
    }
  };

  return (
    <Show
      when={!loading() && item()}
      fallback={
        <div style={{ padding: "60px 0", "text-align": "center", color: "var(--text-muted)" }}>
          Loading media details...
        </div>
      }
    >
      <div style={{ position: "relative" }}>
        {/* Backdrop Hero Header */}
        <div
          style={{
            position: "relative",
            width: "100%",
            height: "360px",
            "border-radius": "var(--radius-lg)",
            overflow: "hidden",
            background: "var(--bg-surface)",
            "margin-bottom": "-100px",
          }}
        >
          <Show when={backdropUrl()}>
            <img
              src={backdropUrl() || ""}
              alt=""
              style={{
                width: "100%",
                height: "100%",
                "object-fit": "cover",
                opacity: 0.5,
              }}
            />
          </Show>
          <div
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              background: "linear-gradient(to top, var(--bg-base) 0%, rgba(12, 14, 20, 0.4) 100%)",
            }}
          />
        </div>

        {/* Details Container */}
        <div
          style={{
            position: "relative",
            display: "flex",
            gap: "36px",
            padding: "0 20px",
            "z-index": 1,
          }}
        >
          {/* Left Poster */}
          <div
            style={{
              width: "220px",
              "flex-shrink": 0,
              display: "flex",
              "flex-direction": "column",
              gap: "16px",
            }}
          >
            <div
              style={{
                width: "100%",
                "aspect-ratio": "2 / 3",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                overflow: "hidden",
                "box-shadow": "0 20px 30px -10px rgba(0, 0, 0, 0.8)",
              }}
            >
              <Show
                when={posterUrl()}
                fallback={
                  <div
                    style={{
                      width: "100%",
                      height: "100%",
                      display: "flex",
                      "align-items": "center",
                      "justify-content": "center",
                      "font-size": "36px",
                    }}
                  >
                    🎬
                  </div>
                }
              >
                <img
                  src={posterUrl() || ""}
                  alt={item()?.title}
                  style={{ width: "100%", height: "100%", "object-fit": "cover" }}
                />
              </Show>
            </div>

            {/* Action Buttons below poster */}
            <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
              <button
                type="button"
                onClick={() => handlePlay(Math.max(0, viewOffsetMs()))}
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  gap: "8px",
                  padding: "12px",
                  background: "var(--accent)",
                  color: "#fff",
                  border: "none",
                  "border-radius": "var(--radius-sm)",
                  "font-size": "15px",
                  "font-weight": "700",
                  cursor: "pointer",
                }}
              >
                <span>▶</span>
                <span>{viewOffsetMs() > 0 ? "Resume" : "Play"}</span>
              </button>

              <button
                type="button"
                onClick={handleToggleWatched}
                disabled={watchUpdating()}
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  gap: "8px",
                  padding: "10px",
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  color: "var(--text-primary)",
                  "border-radius": "var(--radius-sm)",
                  "font-size": "13px",
                  "font-weight": "600",
                  cursor: watchUpdating() ? "not-allowed" : "pointer",
                }}
              >
                <span>{isWatched() ? "✓" : "○"}</span>
                <span>{isWatched() ? "Watched" : "Mark as Watched"}</span>
              </button>
            </div>
          </div>

          {/* Right Info Details */}
          <div style={{ flex: 1, "margin-top": "24px" }}>
            <button
              type="button"
              onClick={() => navigate(`/library/${item()?.libraryId}`)}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                "font-size": "13px",
                cursor: "pointer",
                padding: 0,
                "margin-bottom": "8px",
              }}
            >
              &larr; Back to Library
            </button>

            <h1 style={{ margin: "0 0 6px", "font-size": "32px", "font-weight": "800" }}>
              {item()?.title}
            </h1>

            <Show when={item()?.tagline}>
              <div
                style={{
                  "font-size": "15px",
                  "font-style": "italic",
                  color: "var(--text-muted)",
                  "margin-bottom": "14px",
                }}
              >
                "{item()?.tagline}"
              </div>
            </Show>

            {/* Badges / Meta row */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "12px",
                "font-size": "13px",
                color: "var(--text-secondary)",
                "margin-bottom": "20px",
                "flex-wrap": "wrap",
              }}
            >
              <Show when={item()?.originallyAvailableAt}>
                <span>{item()?.originallyAvailableAt?.substring(0, 4)}</span>
              </Show>

              <Show when={item()?.durationMs}>
                <span>•</span>
                <span>{formatDuration(item()?.durationMs)}</span>
              </Show>

              <Show when={item()?.contentRating}>
                <span>•</span>
                <span
                  style={{
                    padding: "2px 6px",
                    border: "1px solid var(--border)",
                    "border-radius": "3px",
                    "font-size": "11px",
                    "font-weight": "600",
                  }}
                >
                  {item()?.contentRating}
                </span>
              </Show>

              <Show when={item()?.rating}>
                <span>•</span>
                <span style={{ color: "var(--warning)", "font-weight": "600" }}>
                  ★ {item()?.rating?.toFixed(1)}
                </span>
              </Show>

              <Show when={item()?.studio}>
                <span>•</span>
                <span>{item()?.studio}</span>
              </Show>
            </div>

            {/* Genres */}
            <Show when={(item()?.genres.length ?? 0) > 0}>
              <div
                style={{
                  display: "flex",
                  gap: "8px",
                  "margin-bottom": "20px",
                  "flex-wrap": "wrap",
                }}
              >
                <For each={item()?.genres}>
                  {(g) => (
                    <span
                      style={{
                        padding: "3px 10px",
                        background: "var(--bg-surface)",
                        border: "1px solid var(--border)",
                        "border-radius": "12px",
                        "font-size": "12px",
                        color: "var(--text-secondary)",
                      }}
                    >
                      {g}
                    </span>
                  )}
                </For>
              </div>
            </Show>

            {/* Overview */}
            <div style={{ "margin-bottom": "28px" }}>
              <h3 style={{ margin: "0 0 8px", "font-size": "16px", "font-weight": "700" }}>
                Overview
              </h3>
              <p
                style={{
                  margin: 0,
                  "font-size": "14px",
                  "line-height": "1.6",
                  color: "var(--text-secondary)",
                  "max-width": "800px",
                }}
              >
                {item()?.summary || "No description provided."}
              </p>
            </div>

            {/* Cast & Crew */}
            <Show when={(item()?.people.length ?? 0) > 0}>
              <div style={{ "margin-bottom": "28px" }}>
                <h3 style={{ margin: "0 0 10px", "font-size": "16px", "font-weight": "700" }}>
                  Cast & Crew
                </h3>
                <div style={{ display: "flex", gap: "10px", "flex-wrap": "wrap" }}>
                  <For each={item()?.people}>
                    {(person) => (
                      <div
                        style={{
                          padding: "6px 12px",
                          background: "var(--bg-surface)",
                          border: "1px solid var(--border)",
                          "border-radius": "var(--radius-sm)",
                          "font-size": "12px",
                        }}
                      >
                        <div style={{ "font-weight": "600", color: "var(--text-primary)" }}>
                          {person.name}
                        </div>
                        <div style={{ color: "var(--text-muted)", "font-size": "11px" }}>
                          {person.character || person.role}
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            {/* Children: Seasons / Episodes (for Shows) */}
            <Show when={children().length > 0}>
              <div style={{ "margin-top": "36px" }}>
                <h2 style={{ margin: "0 0 16px", "font-size": "20px", "font-weight": "700" }}>
                  Episodes & Children
                </h2>
                <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
                  <For each={children()}>
                    {(child) => (
                      <div
                        style={{
                          display: "flex",
                          "align-items": "center",
                          "justify-content": "space-between",
                          padding: "14px 18px",
                          background: "var(--bg-surface)",
                          border: "1px solid var(--border)",
                          "border-radius": "var(--radius-md)",
                        }}
                      >
                        <div style={{ display: "flex", "align-items": "center", gap: "14px" }}>
                          <span
                            style={{
                              "font-size": "13px",
                              "font-weight": "700",
                              color: "var(--text-muted)",
                              width: "30px",
                            }}
                          >
                            {child.idx ? `#${child.idx}` : "-"}
                          </span>
                          <div>
                            <div style={{ "font-weight": "600", "font-size": "14px" }}>
                              {child.title}
                            </div>
                            <div style={{ color: "var(--text-muted)", "font-size": "12px" }}>
                              {formatDuration(child.durationMs)}
                            </div>
                          </div>
                        </div>

                        <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
                          <button
                            type="button"
                            onClick={() =>
                              playItem({
                                itemId: child.id,
                                initialStartMs: Math.max(0, child.viewOffsetMs),
                                title: child.title,
                                subtitle: item()?.title,
                              })
                            }
                            style={{
                              padding: "6px 14px",
                              background: "var(--accent)",
                              color: "#fff",
                              border: "none",
                              "border-radius": "var(--radius-sm)",
                              "font-weight": "600",
                              "font-size": "13px",
                              cursor: "pointer",
                            }}
                          >
                            ▶ Play
                          </button>
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}
export default ItemDetail;
