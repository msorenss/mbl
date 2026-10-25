import { type JSX, ErrorBoundary as SolidErrorBoundary } from "solid-js";

export function ErrorBoundary(props: { children: JSX.Element }) {
  return (
    <SolidErrorBoundary
      fallback={(err, reset) => (
        <div
          style={{
            padding: "24px",
            margin: "24px",
            background: "var(--danger-light)",
            border: "1px solid var(--danger)",
            "border-radius": "var(--radius-md)",
            color: "var(--text-primary)",
          }}
        >
          <h3 style={{ margin: "0 0 8px 0", color: "var(--danger)" }}>Something went wrong</h3>
          <p style={{ margin: "0 0 16px 0", color: "var(--text-secondary)", "font-size": "14px" }}>
            {err?.message || String(err)}
          </p>
          <button
            type="button"
            onClick={reset}
            style={{
              padding: "8px 16px",
              background: "var(--danger)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              cursor: "pointer",
              "font-weight": "500",
            }}
          >
            Try Again
          </button>
        </div>
      )}
    >
      {props.children}
    </SolidErrorBoundary>
  );
}
