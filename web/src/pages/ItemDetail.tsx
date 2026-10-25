import { useNavigate, useParams } from "@solidjs/router";
import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Badge, Button, Chip } from "../components/ui";
import { usePlayer } from "../context/PlayerContext";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";

type ItemDetailDto = components["schemas"]["ItemDetail"];
type ItemSummary = components["schemas"]["ItemSummary"];
type PlaybackPreview = components["schemas"]["PlaybackPreview"];

export function ItemDetail() {
  const params = useParams();
  const itemId = () => Number(params.id);
  const { playItem } = usePlayer();
  const { showToast } = useToast();
  const { t } = useI18n();
  const navigate = useNavigate();

  const [item, setItem] = createSignal<ItemDetailDto | null>(null);
  const [children, setChildren] = createSignal<ItemSummary[]>([]);
  const [preview, setPreview] = createSignal<PlaybackPreview | null>(null);
  const [selectedAudioIdx, setSelectedAudioIdx] = createSignal<number | null>(null);
  const [selectedSubIdx, setSelectedSubIdx] = createSignal<number | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [watchUpdating, setWatchUpdating] = createSignal(false);

  const loadData = async () => {
    setLoading(true);
    try {
      const [detailRes, childrenRes, previewRes] = await Promise.all([
        client.GET("/api/v1/items/{id}", {
          params: { path: { id: itemId() } },
        }),
        client.GET("/api/v1/items/{id}/children", {
          params: { path: { id: itemId() } },
        }),
        client.GET("/api/v1/items/{id}/playback-preview", {
          params: { path: { id: itemId() }, query: { profile: "chrome" } },
        }),
      ]);

      if (detailRes.data) setItem(detailRes.data);
      if (childrenRes.data) setChildren(childrenRes.data);
      if (previewRes.data) setPreview(previewRes.data);
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

  const endsAtTime = (ms?: number | null) => {
    if (!ms || ms <= 0) return null;
    const endTime = new Date(Date.now() + ms);
    return endTime.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  };

  const backdropArt = () => item()?.artwork.find((a) => a.kind === "backdrop");
  const posterArt = () => item()?.artwork.find((a) => a.kind === "poster");

  const backdropUrl = () => {
    const art = backdropArt();
    return art ? `/api/v1/images/${art.id}?w=1280` : null;
  };

  const posterUrl = () => {
    const art = posterArt();
    return art ? `/api/v1/images/${art.id}?w=500` : null;
  };

  // Ambient palette colors from artwork (M3.4)
  const ambientColors = () => {
    const p = backdropArt()?.palette || posterArt()?.palette || [];
    const dominant = p[0] || "#141721";
    const vibrant = p[1] || "var(--accent)";
    return { dominant, vibrant };
  };

  const isWatched = () => (item()?.watch.viewCount ?? 0) > 0;
  const viewOffsetMs = () => item()?.watch.viewOffsetMs ?? 0;

  // Primary video version & streams
  const primaryVersion = () => item()?.versions?.[0];
  const primaryPart = () => primaryVersion()?.parts?.[0];
  const audioStreams = () => primaryPart()?.streams.filter((s) => s.kind === "audio") || [];
  const subStreams = () => primaryPart()?.streams.filter((s) => s.kind === "subtitle") || [];

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
      <div
        style={{
          position: "relative",
          "border-radius": "var(--radius-lg)",
          overflow: "hidden",
          background: `radial-gradient(circle at 80% 20%, ${ambientColors().vibrant}33 0%, transparent 60%), radial-gradient(circle at 20% 80%, ${ambientColors().dominant}88 0%, var(--bg-base) 100%)`,
          padding: "var(--space-6)",
        }}
      >
        {/* Ambient Blurred Backdrop Top */}
        <div
          style={{
            position: "relative",
            width: "100%",
            height: "360px",
            "border-radius": "var(--radius-md)",
            overflow: "hidden",
            "margin-bottom": "-120px",
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
                opacity: 0.55,
              }}
            />
          </Show>
          <div
            style={{
              position: "absolute",
              inset: 0,
              background: "linear-gradient(to top, var(--bg-base) 5%, rgba(12, 14, 20, 0.4) 100%)",
            }}
          />
        </div>

        {/* Details Container */}
        <div
          style={{
            position: "relative",
            display: "flex",
            gap: "var(--space-8)",
            padding: "0 var(--space-4)",
            "z-index": 2,
            "flex-wrap": "wrap",
          }}
        >
          {/* Left Poster Card & Primary Actions */}
          <div
            style={{
              width: "240px",
              "flex-shrink": 0,
              display: "flex",
              "flex-direction": "column",
              gap: "var(--space-4)",
            }}
          >
            <div
              style={{
                width: "100%",
                "aspect-ratio": "2 / 3",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-lg)",
                overflow: "hidden",
                "box-shadow": "var(--shadow-xl)",
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
            <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-3)" }}>
              <Button
                size="lg"
                variant="primary"
                onClick={() => handlePlay(Math.max(0, viewOffsetMs()))}
              >
                <span>▶</span>
                <span>{viewOffsetMs() > 0 ? t("resume") : t("play")}</span>
              </Button>

              {viewOffsetMs() > 0 && (
                <Button size="md" variant="secondary" onClick={() => handlePlay(0)}>
                  {t("fromBeginning")}
                </Button>
              )}

              <Button
                size="md"
                variant="outline"
                onClick={handleToggleWatched}
                disabled={watchUpdating()}
              >
                <span>{isWatched() ? "✓" : "○"}</span>
                <span>{isWatched() ? "Watched" : "Mark as Watched"}</span>
              </Button>
            </div>

            {/* Playback Prediction Chip (M3.7) */}
            <Show when={preview()}>
              {(p) => {
                const method = () => p().method;
                const badgeVariant = () =>
                  method() === "directPlay"
                    ? "direct-play"
                    : method() === "directStream"
                      ? "direct-stream"
                      : "transcode";
                const badgeLabel = () =>
                  method() === "directPlay"
                    ? "⚡ Direct Play"
                    : method() === "directStream"
                      ? "🔄 Direct Stream"
                      : "⚙ Transcode";

                return (
                  <div
                    style={{
                      padding: "var(--space-3)",
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-md)",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "space-between",
                        "margin-bottom": "4px",
                      }}
                    >
                      <span
                        style={{
                          "font-size": "11px",
                          color: "var(--text-muted)",
                          "text-transform": "uppercase",
                          "font-weight": "bold",
                        }}
                      >
                        Playback Engine
                      </span>
                      <Badge variant={badgeVariant()}>{badgeLabel()}</Badge>
                    </div>
                    <Show when={p().reasons.length > 0}>
                      <div
                        style={{
                          "font-size": "11px",
                          color: "var(--text-secondary)",
                          "margin-top": "4px",
                        }}
                      >
                        {p().reasons[0]}
                      </div>
                    </Show>
                  </div>
                );
              }}
            </Show>
          </div>

          {/* Right Info Details */}
          <div style={{ flex: 1, "min-width": "300px", "margin-top": "var(--space-6)" }}>
            <button
              type="button"
              onClick={() => navigate(`/library/${item()?.libraryId}`)}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-secondary)",
                "font-size": "var(--font-size-sm)",
                cursor: "pointer",
                padding: 0,
                "margin-bottom": "var(--space-2)",
              }}
            >
              &larr; Back to Library
            </button>

            <h1
              style={{
                margin: "0 0 8px",
                "font-size": "clamp(26px, 4vw, 38px)",
                "font-weight": "800",
              }}
            >
              {item()?.title}
            </h1>

            <Show when={item()?.tagline}>
              <div
                style={{
                  "font-size": "var(--font-size-md)",
                  "font-style": "italic",
                  color: "var(--text-muted)",
                  "margin-bottom": "var(--space-4)",
                }}
              >
                "{item()?.tagline}"
              </div>
            </Show>

            {/* Badges / Meta row with Media Tags & "Ends at" time */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "var(--space-2)",
                "font-size": "var(--font-size-sm)",
                color: "var(--text-secondary)",
                "margin-bottom": "var(--space-4)",
                "flex-wrap": "wrap",
              }}
            >
              {item()?.originallyAvailableAt && (
                <span>{item()?.originallyAvailableAt?.substring(0, 4)}</span>
              )}

              {item()?.durationMs && (
                <>
                  <span>•</span>
                  <span>{formatDuration(item()?.durationMs)}</span>
                  <span>•</span>
                  <span style={{ color: "var(--accent)" }}>
                    {t("endsAt")} {endsAtTime(item()?.durationMs)}
                  </span>
                </>
              )}

              {item()?.contentRating && <Badge variant="default">{item()?.contentRating}</Badge>}
              {item()?.rating && <Badge variant="warning">★ {item()?.rating?.toFixed(1)}</Badge>}

              {/* Resolution & Codec Badges */}
              {(primaryVersion()?.width ?? 0) >= 3800 && (
                <Badge variant="accent">4K Ultra HD</Badge>
              )}
              {primaryVersion()?.hdr && <Badge variant="accent">{primaryVersion()?.hdr}</Badge>}
              {primaryVersion()?.videoCodec && (
                <Badge variant="default">{primaryVersion()?.videoCodec?.toUpperCase()}</Badge>
              )}
              {primaryVersion()?.audioCodec && (
                <Badge variant="default">
                  {primaryVersion()?.audioCodec?.toUpperCase()} {primaryVersion()?.audioChannels}ch
                </Badge>
              )}
            </div>

            {/* Genres */}
            <Show when={(item()?.genres.length ?? 0) > 0}>
              <div
                style={{
                  display: "flex",
                  gap: "var(--space-2)",
                  "margin-bottom": "var(--space-6)",
                  "flex-wrap": "wrap",
                }}
              >
                <For each={item()?.genres}>{(g) => <Chip label={g} />}</For>
              </div>
            </Show>

            {/* Overview */}
            <div style={{ "margin-bottom": "var(--space-6)" }}>
              <h3
                style={{
                  margin: "0 0 8px",
                  "font-size": "var(--font-size-md)",
                  "font-weight": "var(--font-weight-bold)",
                }}
              >
                Overview
              </h3>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-sm)",
                  "line-height": "1.6",
                  color: "var(--text-secondary)",
                  "max-width": "800px",
                }}
              >
                {item()?.summary || "No description provided."}
              </p>
            </div>

            {/* Stream Pickers before playback (Audio / Subtitles) */}
            <div
              style={{
                display: "flex",
                gap: "var(--space-6)",
                "margin-bottom": "var(--space-6)",
                "flex-wrap": "wrap",
              }}
            >
              {audioStreams().length > 1 && (
                <div>
                  <label
                    for="audio-stream-select"
                    style={{
                      display: "block",
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "bold",
                      color: "var(--text-muted)",
                      "margin-bottom": "4px",
                    }}
                  >
                    {t("audio")}
                  </label>
                  <select
                    id="audio-stream-select"
                    value={selectedAudioIdx() ?? ""}
                    onChange={(e) => setSelectedAudioIdx(Number(e.currentTarget.value))}
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      padding: "6px 10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    <For each={audioStreams()}>
                      {(s) => (
                        <option value={s.idx}>
                          {s.language?.toUpperCase() || "Unknown"} ({s.codec} {s.channels}ch)
                        </option>
                      )}
                    </For>
                  </select>
                </div>
              )}

              {subStreams().length > 0 && (
                <div>
                  <label
                    for="subtitle-stream-select"
                    style={{
                      display: "block",
                      "font-size": "var(--font-size-xs)",
                      "font-weight": "bold",
                      color: "var(--text-muted)",
                      "margin-bottom": "4px",
                    }}
                  >
                    {t("subtitles")}
                  </label>
                  <select
                    id="subtitle-stream-select"
                    value={selectedSubIdx() ?? ""}
                    onChange={(e) =>
                      setSelectedSubIdx(
                        e.currentTarget.value ? Number(e.currentTarget.value) : null,
                      )
                    }
                    style={{
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      padding: "6px 10px",
                      "border-radius": "var(--radius-sm)",
                    }}
                  >
                    <option value="">Off</option>
                    <For each={subStreams()}>
                      {(s) => (
                        <option value={s.idx}>
                          {s.language?.toUpperCase() || "Subtitle"} ({s.codec}){" "}
                          {s.isForced ? "[Forced]" : ""}
                        </option>
                      )}
                    </For>
                  </select>
                </div>
              )}
            </div>

            {/* Cast & Crew */}
            <Show when={(item()?.people.length ?? 0) > 0}>
              <div style={{ "margin-bottom": "var(--space-6)" }}>
                <h3
                  style={{
                    margin: "0 0 10px",
                    "font-size": "var(--font-size-md)",
                    "font-weight": "var(--font-weight-bold)",
                  }}
                >
                  {t("cast")}
                </h3>
                <div style={{ display: "flex", gap: "var(--space-3)", "flex-wrap": "wrap" }}>
                  <For each={item()?.people.slice(0, 10)}>
                    {(person) => (
                      <div
                        style={{
                          padding: "6px 12px",
                          background: "var(--bg-surface)",
                          border: "1px solid var(--border)",
                          "border-radius": "var(--radius-sm)",
                        }}
                      >
                        <div
                          style={{
                            "font-weight": "600",
                            "font-size": "var(--font-size-xs)",
                            color: "var(--text-primary)",
                          }}
                        >
                          {person.name}
                        </div>
                        <div style={{ color: "var(--text-muted)", "font-size": "10px" }}>
                          {person.character || person.role}
                        </div>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            </Show>

            {/* Children: Seasons / Episodes */}
            <Show when={children().length > 0}>
              <div style={{ "margin-top": "var(--space-8)" }}>
                <h2
                  style={{
                    margin: "0 0 16px",
                    "font-size": "var(--font-size-xl)",
                    "font-weight": "var(--font-weight-bold)",
                  }}
                >
                  {t("episodes")}
                </h2>
                <div style={{ display: "flex", "flex-direction": "column", gap: "var(--space-3)" }}>
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
                        <div
                          style={{
                            display: "flex",
                            "align-items": "center",
                            gap: "var(--space-4)",
                          }}
                        >
                          <span
                            style={{
                              "font-size": "var(--font-size-sm)",
                              "font-weight": "bold",
                              color: "var(--text-muted)",
                              width: "32px",
                            }}
                          >
                            {child.idx ? `#${child.idx}` : "-"}
                          </span>
                          <div>
                            <div
                              style={{ "font-weight": "600", "font-size": "var(--font-size-sm)" }}
                            >
                              {child.title}
                            </div>
                            <div
                              style={{
                                color: "var(--text-muted)",
                                "font-size": "var(--font-size-xs)",
                              }}
                            >
                              {formatDuration(child.durationMs)}
                            </div>
                          </div>
                        </div>

                        <Button
                          size="sm"
                          variant="primary"
                          onClick={() =>
                            playItem({
                              itemId: child.id,
                              initialStartMs: Math.max(0, child.viewOffsetMs),
                              title: child.title,
                              subtitle: item()?.title,
                            })
                          }
                        >
                          ▶ Play
                        </Button>
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
