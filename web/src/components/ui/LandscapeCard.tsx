import { A } from "@solidjs/router";
import { createSignal, Show } from "solid-js";
import type { components } from "../../api/schema";
import { usePlayer } from "../../context/PlayerContext";

type ItemSummary = components["schemas"]["ItemSummary"];

export interface LandscapeCardProps {
  item: ItemSummary;
  subtitle?: string;
  badge?: string;
  onClick?: () => void;
}

export function LandscapeCard(props: LandscapeCardProps) {
  const { playItem } = usePlayer();
  const [imageLoaded, setImageLoaded] = createSignal(false);
  const [imageError, setImageError] = createSignal(false);

  const item = () => props.item;
  const imageUrl = () => (item().posterId ? `/api/v1/images/${item().posterId}?w=480` : null);

  const progressPercent = () => {
    const cur = item().viewOffsetMs;
    const dur = item().durationMs;
    if (!dur || dur <= 0 || cur <= 0) return 0;
    return Math.min(100, Math.round((cur / dur) * 100));
  };

  const handleQuickPlay = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    playItem({
      itemId: item().id,
      initialStartMs: Math.max(0, item().viewOffsetMs),
      title: item().title,
      subtitle: props.subtitle,
    });
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        width: "100%",
        "min-width": "240px",
        "max-width": "300px",
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
        <div
          class="poster-container"
          style={{
            position: "relative",
            width: "100%",
            "aspect-ratio": "16 / 9",
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            overflow: "hidden",
            transition: "transform 0.2s ease, box-shadow 0.2s ease",
          }}
        >
          <Show
            when={imageUrl() && !imageError()}
            fallback={
              <div
                style={{
                  width: "100%",
                  height: "100%",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  background: "linear-gradient(135deg, var(--bg-card) 0%, var(--bg-surface) 100%)",
                  color: "var(--text-muted)",
                }}
              >
                <span>📺</span>
              </div>
            }
          >
            <img
              src={imageUrl() || ""}
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

          {/* Quick Play Button */}
          <button
            type="button"
            onClick={handleQuickPlay}
            class="poster-play-btn"
            style={{
              position: "absolute",
              top: "50%",
              left: "50%",
              transform: "translate(-50%, -50%)",
              width: "44px",
              height: "44px",
              "border-radius": "var(--radius-full)",
              background: "var(--accent)",
              border: "none",
              color: "#fff",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              cursor: "pointer",
              "box-shadow": "var(--shadow-md)",
            }}
          >
            ▶
          </button>

          {/* Badge */}
          {props.badge && (
            <div
              style={{
                position: "absolute",
                top: "8px",
                left: "8px",
                padding: "2px 6px",
                "border-radius": "var(--radius-sm)",
                "font-size": "10px",
                "font-weight": "var(--font-weight-bold)",
                background: "rgba(0, 0, 0, 0.75)",
                color: "var(--text-primary)",
              }}
            >
              {props.badge}
            </div>
          )}

          {/* Resume Progress Bar */}
          <Show when={progressPercent() > 0 && item().viewCount === 0}>
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

        <div style={{ "margin-top": "8px" }}>
          <div
            title={item().title}
            style={{
              "font-size": "var(--font-size-sm)",
              "font-weight": "var(--font-weight-semibold)",
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
              "font-size": "var(--font-size-xs)",
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
