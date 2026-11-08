import { createUniqueId, Show } from "solid-js";

export interface SwitchProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  description?: string;
  disabled?: boolean;
  id?: string;
}

export function Switch(props: SwitchProps) {
  const autoId = createUniqueId();
  const switchId = () => props.id || `mbl-switch-${autoId}`;

  const toggle = () => {
    if (!props.disabled) {
      props.onChange(!props.checked);
    }
  };

  const buttonEl = () => (
    <button
      id={switchId()}
      type="button"
      role="switch"
      aria-checked={props.checked}
      aria-label={props.label}
      disabled={props.disabled}
      onClick={toggle}
      style={{
        position: "relative",
        display: "inline-flex",
        "align-items": "center",
        width: "44px",
        height: "24px",
        padding: "2px",
        "border-radius": "var(--radius-full)",
        border: props.checked ? "1px solid var(--accent)" : "1px solid var(--border)",
        background: props.checked ? "var(--accent)" : "var(--bg-surface)",
        cursor: props.disabled ? "not-allowed" : "pointer",
        opacity: props.disabled ? "0.5" : "1",
        "flex-shrink": 0,
        transition:
          "background-color var(--duration-fast) var(--ease-standard), border-color var(--duration-fast) var(--ease-standard)",
      }}
    >
      <span
        style={{
          width: "18px",
          height: "18px",
          "border-radius": "var(--radius-full)",
          background: "#ffffff",
          "box-shadow": "0 1px 3px rgba(0, 0, 0, 0.35)",
          transform: props.checked ? "translateX(20px)" : "translateX(0)",
          transition: "transform var(--duration-fast) var(--ease-standard)",
        }}
      />
    </button>
  );

  return (
    <Show when={props.label} fallback={buttonEl()}>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          gap: "var(--space-3)",
        }}
      >
        <div style={{ flex: 1, "min-width": 0 }}>
          <label
            for={switchId()}
            style={{
              display: "block",
              "font-size": "var(--font-size-sm)",
              "font-weight": "var(--font-weight-medium)",
              color: "var(--text-primary)",
              cursor: props.disabled ? "not-allowed" : "pointer",
            }}
          >
            {props.label}
          </label>
          <Show when={props.description}>
            <div
              style={{
                "font-size": "var(--font-size-xs)",
                color: "var(--text-muted)",
                "margin-top": "2px",
              }}
            >
              {props.description}
            </div>
          </Show>
        </div>
        {buttonEl()}
      </div>
    </Show>
  );
}
