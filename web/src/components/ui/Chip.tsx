import type { JSX } from "solid-js";

export interface ChipProps {
  label: string;
  active?: boolean;
  onClick?: () => void;
  onClear?: () => void;
  class?: string;
  icon?: JSX.Element;
}

export function Chip(props: ChipProps) {
  return (
    <button
      type="button"
      onClick={() => props.onClick?.()}
      class={`mbl-chip ${props.class || ""}`}
      style={{
        display: "inline-flex",
        "align-items": "center",
        gap: "var(--space-1)",
        padding: "4px 12px",
        "border-radius": "var(--radius-full)",
        "font-size": "var(--font-size-sm)",
        "font-weight": props.active ? "var(--font-weight-semibold)" : "var(--font-weight-regular)",
        "background-color": props.active ? "var(--accent)" : "var(--bg-surface)",
        color: props.active ? "#ffffff" : "var(--text-secondary)",
        border: props.active ? "1px solid var(--accent)" : "1px solid var(--border)",
        cursor: "pointer",
        transition: "all var(--duration-fast) var(--ease-standard)",
        outline: "none",
      }}
    >
      {props.icon}
      <span>{props.label}</span>
      {props.onClear && (
        // biome-ignore lint/a11y/useSemanticElements: clear button inside interactive chip element
        <span
          role="button"
          tabIndex={0}
          onClick={(e) => {
            e.stopPropagation();
            props.onClear?.();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.stopPropagation();
              props.onClear?.();
            }
          }}
          style={{
            "margin-left": "var(--space-1)",
            "font-size": "14px",
            opacity: "0.75",
          }}
        >
          ×
        </span>
      )}
    </button>
  );
}
