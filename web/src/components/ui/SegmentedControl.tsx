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
  class?: string;
}

export function SegmentedControl<T extends string = string>(props: SegmentedControlProps<T>) {
  const isSm = () => props.size === "sm";

  return (
    <div
      role="radiogroup"
      class={`mbl-segmented-control ${props.class || ""}`}
      style={{
        display: "inline-flex",
        "background-color": "var(--bg-card)",
        padding: "3px",
        "border-radius": "var(--radius-sm)",
        border: "1px solid var(--border)",
        gap: "2px",
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
              style={{
                border: "none",
                outline: "none",
                cursor: "pointer",
                padding: isSm() ? "4px 10px" : "6px 14px",
                "font-size": isSm() ? "var(--font-size-xs)" : "var(--font-size-sm)",
                "font-weight": isSelected()
                  ? "var(--font-weight-semibold)"
                  : "var(--font-weight-medium)",
                "border-radius": "calc(var(--radius-sm) - 2px)",
                "background-color": isSelected() ? "var(--accent)" : "transparent",
                color: isSelected() ? "#ffffff" : "var(--text-secondary)",
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
