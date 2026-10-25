import { createContext, createSignal, For, type JSX, useContext } from "solid-js";

export interface Toast {
  id: string;
  title?: string;
  message: string;
  type?: "info" | "success" | "error" | "warning" | "danger";
}

interface ToastContextValue {
  showToast: (toast: Omit<Toast, "id">) => void;
  removeToast: (id: string) => void;
}

const ToastContext = createContext<ToastContextValue>();

export function ToastProvider(props: { children: JSX.Element }) {
  const [toasts, setToasts] = createSignal<Toast[]>([]);

  const removeToast = (id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  };

  const showToast = (toast: Omit<Toast, "id">) => {
    const id = Math.random().toString(36).substring(2, 9);
    const newToast: Toast = { ...toast, id };
    setToasts((prev) => [...prev, newToast]);
    setTimeout(() => removeToast(id), 4000);
  };

  return (
    <ToastContext.Provider value={{ showToast, removeToast }}>
      {props.children}
      <div
        style={{
          position: "fixed",
          bottom: "16px",
          right: "16px",
          display: "flex",
          "flex-direction": "column",
          gap: "8px",
          "z-index": "9999",
          "pointer-events": "none",
          "max-width": "380px",
        }}
      >
        <For each={toasts()}>
          {(t) => (
            <div
              style={{
                "pointer-events": "auto",
                background: "var(--bg-overlay)",
                color: "var(--text-primary)",
                border: "1px solid var(--border)",
                "border-left": `4px solid ${
                  t.type === "error" || t.type === "danger"
                    ? "var(--danger)"
                    : t.type === "warning"
                      ? "var(--warning)"
                      : t.type === "success"
                        ? "var(--success)"
                        : "var(--accent)"
                }`,
                "border-radius": "var(--radius-md)",
                padding: "12px 16px",
                "box-shadow": "0 10px 15px -3px rgba(0, 0, 0, 0.5)",
                display: "flex",
                "justify-content": "space-between",
                "align-items": "flex-start",
                gap: "12px",
              }}
            >
              <div>
                {t.title && (
                  <div style={{ "font-weight": "600", "margin-bottom": "2px" }}>{t.title}</div>
                )}
                <div style={{ color: "var(--text-secondary)", "font-size": "13px" }}>
                  {t.message}
                </div>
              </div>
              <button
                type="button"
                onClick={() => removeToast(t.id)}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "var(--text-muted)",
                  padding: "0",
                  cursor: "pointer",
                  "font-size": "16px",
                }}
              >
                &times;
              </button>
            </div>
          )}
        </For>
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return ctx;
}
