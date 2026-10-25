import { A } from "@solidjs/router";
import { type JSX, Show } from "solid-js";
import { Badge } from "./Badge";
import { Button } from "./Button";

export interface HeroProps {
  title: string;
  tagline?: string;
  backdropUrl?: string;
  posterUrl?: string;
  year?: number;
  durationMs?: number;
  genres?: string[];
  badges?: string[];
  dominantColor?: string;
  vibrantColor?: string;
  onPlay?: () => void;
  detailsHref?: string;
  children?: JSX.Element;
}

export function Hero(props: HeroProps) {
  const formatDuration = (ms?: number) => {
    if (!ms) return null;
    const totalMinutes = Math.floor(ms / 60000);
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return hours > 0 ? `${hours}h ${minutes}m` : `${minutes}m`;
  };

  const ambientBg = () => {
    const dominant = props.dominantColor || "#181b26";
    const vibrant = props.vibrantColor || "var(--accent)";
    return `radial-gradient(circle at 75% 30%, ${vibrant}26 0%, transparent 60%), radial-gradient(circle at 25% 70%, ${dominant}80 0%, var(--bg-base) 100%)`;
  };

  return (
    <div
      class="mbl-hero"
      style={{
        position: "relative",
        width: "100%",
        "min-height": "420px",
        display: "flex",
        "align-items": "flex-end",
        padding: "var(--space-8) var(--space-6)",
        overflow: "hidden",
        "border-radius": "var(--radius-lg)",
        "margin-bottom": "var(--space-8)",
        background: ambientBg(),
      }}
    >
      {/* Background Backdrop Image */}
      <Show when={props.backdropUrl}>
        <div
          style={{
            position: "absolute",
            inset: 0,
            "z-index": 0,
            "background-image": `url(${props.backdropUrl})`,
            "background-size": "cover",
            "background-position": "center top",
            opacity: "0.45",
            filter: "brightness(0.85)",
          }}
        />
        {/* Gradient Fade to Base */}
        <div
          style={{
            position: "absolute",
            inset: 0,
            "z-index": 1,
            background:
              "linear-gradient(to top, var(--bg-base) 5%, rgba(12, 14, 20, 0.7) 40%, transparent 100%), linear-gradient(to right, var(--bg-base) 20%, transparent 80%)",
          }}
        />
      </Show>

      {/* Content */}
      <div
        style={{
          position: "relative",
          "z-index": 2,
          "max-width": "640px",
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-3)",
        }}
      >
        {/* Badges / Metadata header */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "var(--space-2)",
            "flex-wrap": "wrap",
          }}
        >
          {props.year && (
            <span
              style={{
                "font-size": "var(--font-size-sm)",
                color: "var(--text-secondary)",
                "font-weight": "var(--font-weight-medium)",
              }}
            >
              {props.year}
            </span>
          )}
          {props.durationMs && (
            <span style={{ "font-size": "var(--font-size-sm)", color: "var(--text-secondary)" }}>
              • {formatDuration(props.durationMs)}
            </span>
          )}
          {props.genres?.slice(0, 3).map((g) => (
            <Badge variant="default">{g}</Badge>
          ))}
          {props.badges?.map((b) => (
            <Badge variant="accent">{b}</Badge>
          ))}
        </div>

        {/* Title */}
        <h1
          style={{
            margin: 0,
            "font-size": "clamp(28px, 5vw, var(--font-size-hero))",
            "font-weight": "var(--font-weight-bold)",
            "line-height": "var(--line-height-tight)",
            color: "var(--text-primary)",
            "text-shadow": "0 2px 12px rgba(0, 0, 0, 0.7)",
          }}
        >
          {props.title}
        </h1>

        {/* Tagline */}
        {props.tagline && (
          <p
            style={{
              margin: 0,
              "font-size": "var(--font-size-lg)",
              color: "var(--text-secondary)",
              "line-height": "var(--line-height-normal)",
            }}
          >
            {props.tagline}
          </p>
        )}

        {/* Action Buttons */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "var(--space-4)",
            "margin-top": "var(--space-2)",
          }}
        >
          {props.onPlay && (
            <Button size="lg" variant="primary" onClick={props.onPlay}>
              <span style={{ "font-size": "18px" }}>▶</span> Play Now
            </Button>
          )}
          {props.detailsHref && (
            <A href={props.detailsHref}>
              <Button size="lg" variant="secondary">
                Details & More
              </Button>
            </A>
          )}
          {props.children}
        </div>
      </div>
    </div>
  );
}
