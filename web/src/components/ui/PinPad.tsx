import { createSignal, For } from "solid-js";

export interface PinPadProps {
  length?: number;
  onComplete?: (pin: string) => void;
  title?: string;
  error?: string;
}

export function PinPad(props: PinPadProps) {
  const pinLength = () => props.length || 4;
  const [digits, setDigits] = createSignal<string[]>([]);

  const handleDigit = (d: string) => {
    if (digits().length >= pinLength()) return;
    const next = [...digits(), d];
    setDigits(next);
    if (next.length === pinLength()) {
      props.onComplete?.(next.join(""));
    }
  };

  const handleBackspace = () => {
    if (digits().length === 0) return;
    setDigits(digits().slice(0, -1));
  };

  const handleClear = () => {
    setDigits([]);
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        gap: "var(--space-6)",
        "max-width": "280px",
        margin: "0 auto",
      }}
    >
      {props.title && (
        <h3
          style={{ margin: "0", color: "var(--text-primary)", "font-size": "var(--font-size-lg)" }}
        >
          {props.title}
        </h3>
      )}

      {/* Masked PIN dots */}
      <div style={{ display: "flex", gap: "var(--space-4)" }}>
        <For each={Array.from({ length: pinLength() })}>
          {(_, i) => {
            const isFilled = () => i() < digits().length;
            return (
              <div
                style={{
                  width: "16px",
                  height: "16px",
                  "border-radius": "var(--radius-full)",
                  border: "2px solid var(--border)",
                  "background-color": isFilled() ? "var(--accent)" : "transparent",
                  transition: "all var(--duration-fast) var(--ease-standard)",
                  transform: isFilled() ? "scale(1.15)" : "scale(1)",
                }}
              />
            );
          }}
        </For>
      </div>

      {props.error && (
        <span style={{ color: "var(--danger)", "font-size": "var(--font-size-sm)" }}>
          {props.error}
        </span>
      )}

      {/* 3x4 Keypad grid */}
      <div
        style={{
          display: "grid",
          "grid-template-columns": "repeat(3, 1fr)",
          gap: "var(--space-3)",
          width: "100%",
        }}
      >
        <For each={["1", "2", "3", "4", "5", "6", "7", "8", "9"]}>
          {(num) => (
            <button
              type="button"
              onClick={() => handleDigit(num)}
              style={{
                height: "56px",
                "font-size": "var(--font-size-xl)",
                "font-weight": "var(--font-weight-semibold)",
                "background-color": "var(--bg-surface)",
                color: "var(--text-primary)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
                cursor: "pointer",
                transition: "all var(--duration-fast) var(--ease-standard)",
              }}
            >
              {num}
            </button>
          )}
        </For>
        <button
          type="button"
          onClick={handleClear}
          style={{
            height: "56px",
            "font-size": "var(--font-size-sm)",
            "font-weight": "var(--font-weight-medium)",
            "background-color": "transparent",
            color: "var(--text-secondary)",
            border: "none",
            cursor: "pointer",
          }}
        >
          Clear
        </button>
        <button
          type="button"
          onClick={() => handleDigit("0")}
          style={{
            height: "56px",
            "font-size": "var(--font-size-xl)",
            "font-weight": "var(--font-weight-semibold)",
            "background-color": "var(--bg-surface)",
            color: "var(--text-primary)",
            border: "1px solid var(--border)",
            "border-radius": "var(--radius-md)",
            cursor: "pointer",
          }}
        >
          0
        </button>
        <button
          type="button"
          onClick={handleBackspace}
          style={{
            height: "56px",
            "font-size": "var(--font-size-md)",
            "background-color": "transparent",
            color: "var(--text-secondary)",
            border: "none",
            cursor: "pointer",
          }}
        >
          ⌫
        </button>
      </div>
    </div>
  );
}
