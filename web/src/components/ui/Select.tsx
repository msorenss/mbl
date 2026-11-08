import { For, type JSX, Show, splitProps } from "solid-js";

export interface SelectOption<T extends string = string> {
  value: T;
  label: string;
  disabled?: boolean;
}

export interface SelectProps extends JSX.SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
  options?: SelectOption[];
  size?: "sm" | "md";
  fullWidth?: boolean;
  children?: JSX.Element;
}

export function Select(props: SelectProps) {
  const [local, rest] = splitProps(props, [
    "label",
    "options",
    "size",
    "fullWidth",
    "children",
    "class",
    "style",
    "id",
  ]);

  const isSm = () => local.size === "sm";

  const selectEl = () => (
    <select
      id={local.id}
      class={`mbl-select ${local.class || ""}`}
      style={{
        width: local.fullWidth ? "100%" : undefined,
        padding: isSm() ? "5px 30px 5px 10px" : "8px 34px 8px 12px",
        "min-height": isSm() ? "32px" : "38px",
        "font-size": isSm() ? "var(--font-size-xs)" : "var(--font-size-sm)",
        ...(typeof local.style === "object" ? local.style : {}),
      }}
      {...rest}
    >
      <Show when={local.options} fallback={local.children}>
        <For each={local.options}>
          {(opt) => (
            <option value={opt.value} disabled={opt.disabled}>
              {opt.label}
            </option>
          )}
        </For>
      </Show>
    </select>
  );

  return (
    <Show when={local.label} fallback={selectEl()}>
      <div
        style={{
          display: "flex",
          "flex-direction": "column",
          gap: "var(--space-1)",
          width: local.fullWidth ? "100%" : undefined,
        }}
      >
        <label
          for={local.id}
          style={{
            "font-size": "var(--font-size-xs)",
            "font-weight": "var(--font-weight-semibold)",
            color: "var(--text-secondary)",
          }}
        >
          {local.label}
        </label>
        {selectEl()}
      </div>
    </Show>
  );
}
