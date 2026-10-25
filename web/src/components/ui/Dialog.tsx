import { Dialog as KobalteDialog } from "@kobalte/core/dialog";
import { type JSX, splitProps } from "solid-js";

export interface DialogProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  title?: string;
  description?: string;
  children: JSX.Element;
  trigger?: JSX.Element;
}

export function Dialog(props: DialogProps) {
  const [local, rest] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "description",
    "children",
    "trigger",
  ]);

  return (
    <KobalteDialog open={local.open} onOpenChange={local.onOpenChange} {...rest}>
      {local.trigger && <KobalteDialog.Trigger as="div">{local.trigger}</KobalteDialog.Trigger>}
      <KobalteDialog.Portal>
        <KobalteDialog.Overlay
          style={{
            position: "fixed",
            inset: "0",
            "background-color": "rgba(0, 0, 0, 0.75)",
            "backdrop-filter": "blur(4px)",
            "z-index": "100",
            animation: "fadeIn var(--duration-fast) var(--ease-standard)",
          }}
        />
        <div
          style={{
            position: "fixed",
            inset: "0",
            "z-index": "101",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            padding: "var(--space-4)",
          }}
        >
          <KobalteDialog.Content
            style={{
              width: "100%",
              "max-width": "480px",
              "background-color": "var(--bg-card)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-lg)",
              padding: "var(--space-6)",
              "box-shadow": "var(--shadow-xl)",
              outline: "none",
            }}
          >
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                "margin-bottom": "var(--space-4)",
              }}
            >
              {local.title && (
                <KobalteDialog.Title
                  style={{
                    margin: "0",
                    "font-size": "var(--font-size-xl)",
                    "font-weight": "var(--font-weight-bold)",
                    color: "var(--text-primary)",
                  }}
                >
                  {local.title}
                </KobalteDialog.Title>
              )}
              <KobalteDialog.CloseButton
                style={{
                  background: "none",
                  border: "none",
                  color: "var(--text-secondary)",
                  "font-size": "20px",
                  cursor: "pointer",
                  padding: "var(--space-1)",
                }}
              >
                ✕
              </KobalteDialog.CloseButton>
            </div>
            {local.description && (
              <KobalteDialog.Description
                style={{
                  "margin-top": "0",
                  "margin-bottom": "var(--space-4)",
                  color: "var(--text-secondary)",
                  "font-size": "var(--font-size-sm)",
                }}
              >
                {local.description}
              </KobalteDialog.Description>
            )}
            {local.children}
          </KobalteDialog.Content>
        </div>
      </KobalteDialog.Portal>
    </KobalteDialog>
  );
}
