import type { JSX } from "solid-js";

export type BadgeVariant =
  | "default"
  | "accent"
  | "success"
  | "warning"
  | "danger"
  | "info"
  | "direct-play"
  | "direct-stream"
  | "transcode";

export interface BadgeProps {
  variant?: BadgeVariant;
  children: JSX.Element;
  class?: string;
  title?: string;
}

export function Badge(props: BadgeProps) {
  const variant = () => props.variant || "default";

  const getColors = () => {
    switch (variant()) {
      case "accent":
        return {
          bg: "var(--accent-light)",
          text: "var(--accent)",
          border: "1px solid var(--accent)",
        };
      case "success":
      case "direct-play":
        return {
          bg: "var(--success-light)",
          text: "var(--success)",
          border: "1px solid var(--success)",
        };
      case "warning":
      case "direct-stream":
        return {
          bg: "var(--warning-light)",
          text: "var(--warning)",
          border: "1px solid var(--warning)",
        };
      case "danger":
        return {
          bg: "var(--danger-light)",
          text: "var(--danger)",
          border: "1px solid var(--danger)",
        };
      case "info":
      case "transcode":
        return {
          bg: "var(--info-light)",
          text: "var(--info)",
          border: "1px solid var(--info)",
        };
      case "default":
      default:
        return {
          bg: "var(--bg-overlay)",
          text: "var(--text-secondary)",
          border: "1px solid var(--border)",
        };
    }
  };

  return (
    <span
      class={`mbl-badge ${props.class || ""}`}
      title={props.title}
      style={{
        display: "inline-flex",
        "align-items": "center",
        padding: "2px 8px",
        "font-size": "var(--font-size-xs)",
        "font-weight": "var(--font-weight-semibold)",
        "letter-spacing": "0.04em",
        "text-transform": "uppercase",
        "border-radius": "var(--radius-sm)",
        "background-color": getColors().bg,
        color: getColors().text,
        border: getColors().border,
        "line-height": "1.4",
      }}
    >
      {props.children}
    </span>
  );
}
