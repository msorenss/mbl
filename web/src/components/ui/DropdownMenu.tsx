import { DropdownMenu as KobalteDropdown } from "@kobalte/core/dropdown-menu";
import { For, type JSX, Show } from "solid-js";

export interface DropdownMenuItem {
  id: string;
  label: string;
  icon?: JSX.Element | string;
  shortcut?: string;
  danger?: boolean;
  disabled?: boolean;
  checked?: boolean;
  separatorBefore?: boolean;
  onSelect: () => void;
}

export interface DropdownMenuProps {
  trigger: JSX.Element;
  triggerLabel?: string;
  triggerClass?: string;
  triggerStyle?: JSX.CSSProperties;
  header?: JSX.Element;
  items: DropdownMenuItem[];
  gutter?: number;
}

export function DropdownMenu(props: DropdownMenuProps) {
  return (
    <KobalteDropdown gutter={props.gutter ?? 6}>
      <KobalteDropdown.Trigger
        aria-label={props.triggerLabel}
        class={props.triggerClass}
        style={{
          background: "transparent",
          border: "none",
          padding: 0,
          display: "inline-flex",
          "align-items": "center",
          cursor: "pointer",
          color: "inherit",
          ...props.triggerStyle,
        }}
      >
        {props.trigger}
      </KobalteDropdown.Trigger>
      <KobalteDropdown.Portal>
        <KobalteDropdown.Content class="mbl-dropdown-content">
          <Show when={props.header}>
            <div
              style={{
                padding: "8px 10px",
                "border-bottom": "1px solid var(--border)",
                "margin-bottom": "4px",
              }}
            >
              {props.header}
            </div>
          </Show>
          <For each={props.items}>
            {(item) => (
              <>
                <Show when={item.separatorBefore}>
                  <KobalteDropdown.Separator
                    style={{
                      height: "1px",
                      background: "var(--border)",
                      margin: "4px 2px",
                      border: "none",
                    }}
                  />
                </Show>
                <KobalteDropdown.Item
                  disabled={item.disabled}
                  onSelect={item.onSelect}
                  data-danger={item.danger ? "true" : undefined}
                  class="mbl-dropdown-item"
                >
                  <span
                    aria-hidden="true"
                    style={{
                      width: "20px",
                      display: "inline-flex",
                      "align-items": "center",
                      "justify-content": "center",
                      "flex-shrink": 0,
                      "font-size": "15px",
                    }}
                  >
                    {item.icon ?? (item.checked ? "✓" : "")}
                  </span>
                  <span style={{ flex: 1 }}>{item.label}</span>
                  <Show when={item.checked && item.icon}>
                    <span
                      aria-hidden="true"
                      style={{ color: "var(--accent)", "font-weight": "700" }}
                    >
                      ✓
                    </span>
                  </Show>
                  <Show when={item.shortcut}>
                    <span
                      class="mbl-kbd-hint"
                      aria-hidden="true"
                      style={{
                        "font-size": "11px",
                        color: "var(--text-muted)",
                        "margin-left": "auto",
                      }}
                    >
                      {item.shortcut}
                    </span>
                  </Show>
                </KobalteDropdown.Item>
              </>
            )}
          </For>
        </KobalteDropdown.Content>
      </KobalteDropdown.Portal>
    </KobalteDropdown>
  );
}
