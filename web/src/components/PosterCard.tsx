import { A } from "@solidjs/router";
import { createSignal, Show } from "solid-js";
import type { components } from "../api/schema";
import { usePlayer } from "../context/PlayerContext";

type ItemSummary = components["schemas"]["ItemSummary"];

export interface PosterCardProps {
  item: ItemSummary;
  subtitle?: string;
}

export function PosterCard(props: PosterCardProps) {
  const { playItem } = usePlayer();
  const [imageLoaded, setImageLoaded] = createSignal(false);
  const [imageError, setImageError] = createSignal(false);

  const item = () => props.item;
  const posterUrl = () => (item().posterId ? `/api/v1/images/${item().posterId}?w=300` : null);

  const progressPercent = () => {
    const cur = item().viewOffsetMs;
    const dur = item().durationMs;
    if (!dur || dur <= 0 || cur <= 0) return 0;
    return Math.min(100, Math.round((cur / dur) * 100));
  };

  const isWatched = () => item().viewCount > 0;

  const handleQuickPlay = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    playItem({
      itemId: item().id,
      initialStartMs: Math.max(0, item().viewOffsetMs),
      title: item().title,
      subtitle: item().year ? String(item().year) : undefined,
    });
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        width: "100%",
        "max-width": "180px",
      }}
    >
      <A
        href={`/item/${item().id}`}
        style={{
          "text-decoration": "none",
          color: "inherit",
          display: "flex",
          "flex-direction": "column",
        }}
      >
        {/* Poster Container */}
        <div
          class="poster-container"
          style={{
            position: "relative",
            width: "100%",
            "aspect-ratio": "2 / 3",
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            overflow: "hidden",
            transition: "transform 0.2s ease, box-shadow 0.2s ease",
          }}
        >
          {/* Poster Image */}
          <Show
            when={posterUrl() && !imageError()}
            fallback={
              <div
                style={{
                  width: "100%",
                  height: "100%",
                  display: "flex",
                  "flex-direction": "column",
                  "align-items": "center",
                  "justify-content": "center",
                  padding: "12px",
                  "text-align": "center",
                  background: "linear-gradient(135deg, var(--bg-card) 0%, var(--bg-surface) 100%)",
                }}
              >
                <span style={{ "font-size": "28px", "margin-bottom": "8px" }}>🎬</span>
                <span
                  style={{
                    "font-size": "12px",
                    "font-weight": "600",
                    color: "var(--text-secondary)",
                    display: "-webkit-box",
                    "-webkit-line-clamp": "3",
                    "-webkit-box-orient": "vertical",
                    overflow: "hidden",
                  }}
                >
                  {item().title}
                </span>
              </div>
            }
          >
            <img
              src={posterUrl() || ""}
              alt={item().title}
              loading="lazy"
              onLoad={() => setImageLoaded(true)}
              onError={() => setImageError(true)}
              style={{
                width: "100%",
                height: "100%",
                "object-fit": "cover",
                opacity: imageLoaded() ? 1 : 0,
                transition: "opacity 0.25s ease",
              }}
            />
          </Show>

          {/* Quick Play Hover Button */}
          <button
            type="button"
            onClick={handleQuickPlay}
            title={`Play ${item().title}`}
            style={{
              position: "absolute",
              top: "50%",
              left: "50%",
              transform: "translate(-50%, -50%)",
              width: "48px",
              height: "48px",
              "border-radius": "50%",
              background: "var(--accent)",
              border: "none",
              color: "#fff",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-size": "20px",
              cursor: "pointer",
              opacity: 0,
              transition: "opacity 0.2s ease, transform 0.2s ease",
              "box-shadow": "0 8px 16px rgba(0,0,0,0.5)",
              "z-index": 2,
            }}
            class="poster-play-btn"
          >
            ▶
          </button>

          {/* Watched Badge */}
          <Show when={isWatched()}>
            <div
              style={{
                position: "absolute",
                top: "8px",
                right: "8px",
                background: "rgba(0, 0, 0, 0.75)",
                color: "var(--accent)",
                width: "22px",
                height: "22px",
                "border-radius": "50%",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "font-size": "12px",
                "font-weight": "700",
                border: "1px solid rgba(255, 255, 255, 0.2)",
              }}
              title="Watched"
            >
              ✓
            </div>
          </Show>

          {/* Resume Progress Bar */}
          <Show when={progressPercent() > 0 && !isWatched()}>
            <div
              style={{
                position: "absolute",
                bottom: 0,
                left: 0,
                right: 0,
                height: "4px",
                background: "rgba(0, 0, 0, 0.6)",
              }}
            >
              <div
                style={{
                  width: `${progressPercent()}%`,
                  height: "100%",
                  background: "var(--accent)",
                }}
              />
            </div>
          </Show>
        </div>

        {/* Title & Metadata */}
        <div style={{ "margin-top": "8px" }}>
          <div
            title={item().title}
            style={{
              "font-size": "13px",
              "font-weight": "600",
              color: "var(--text-primary)",
              "white-space": "nowrap",
              overflow: "hidden",
              "text-overflow": "ellipsis",
            }}
          >
            {item().title}
          </div>
          <div
            style={{
              "font-size": "12px",
              color: "var(--text-muted)",
              "margin-top": "2px",
            }}
          >
            {props.subtitle || item().year || ""}
          </div>
        </div>
      </A>
    </div>
  );
}
export default PosterCard;
