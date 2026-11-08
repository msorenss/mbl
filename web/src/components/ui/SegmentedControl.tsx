import { For } from "solid-js";

export interface SegmentedControlOption<T extends string = string> {
  value: T;
  label: string;
}

export interface SegmentedControlProps<T extends string = string> {
  options: SegmentedControlOption<T>[];
  value: T;
  onChange: (val: T) => void;
  size?: "sm" | "md";
  ariaLabel?: string;
  class?: string;
}

export function SegmentedControl<T extends string = string>(props: SegmentedControlProps<T>) {
  const isSm = () => props.size === "sm";

  return (
    <div
      role="radiogroup"
      aria-label={props.ariaLabel}
      class={`mbl-segmented-control ${props.class || ""}`}
      style={{
        display: "inline-flex",
        "flex-wrap": "wrap",
        "background-color": "var(--bg-surface)",
        padding: "3px",
        "border-radius": "var(--radius-md)",
        border: "1px solid var(--border)",
        gap: "2px",
        "max-width": "100%",
      }}
    >
      <For each={props.options}>
        {(option) => {
          const isSelected = () => props.value === option.value;
          return (
            // biome-ignore lint/a11y/useSemanticElements: SegmentedControl uses styled button with role=radio
            <button
              type="button"
              role="radio"
              aria-checked={isSelected()}
              onClick={() => props.onChange(option.value)}
              class="mbl-segmented-btn"
              style={{
                border: "none",
                outline: "none",
                cursor: "pointer",
                padding: isSm() ? "5px 11px" : "7px 14px",
                "font-size": isSm() ? "var(--font-size-xs)" : "var(--font-size-sm)",
                "font-weight": isSelected()
                  ? "var(--font-weight-semibold)"
                  : "var(--font-weight-medium)",
                "border-radius": "calc(var(--radius-md) - 3px)",
                "background-color": isSelected() ? "var(--accent)" : "transparent",
                color: isSelected() ? "#ffffff" : "var(--text-secondary)",
                "box-shadow": isSelected() ? "0 2px 8px rgba(0, 0, 0, 0.28)" : "none",
                transition: "all var(--duration-fast) var(--ease-standard)",
              }}
            >
              {option.label}
            </button>
          );
        }}
      </For>
    </div>
  );
}
