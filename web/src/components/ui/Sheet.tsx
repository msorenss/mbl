import { Dialog as KobalteDialog } from "@kobalte/core/dialog";
import { type JSX, splitProps } from "solid-js";

export interface SheetProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  title?: string;
  side?: "right" | "bottom";
  children: JSX.Element;
  trigger?: JSX.Element;
}

export function Sheet(props: SheetProps) {
  const [local, rest] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "side",
    "children",
    "trigger",
  ]);
  const side = () => local.side || "right";

  const getPositionStyles = () => {
    if (side() === "bottom") {
      return {
        bottom: "0",
        left: "0",
        right: "0",
        "max-height": "80vh",
        "border-top-left-radius": "var(--radius-xl)",
        "border-top-right-radius": "var(--radius-xl)",
        "border-top": "1px solid var(--border)",
      };
    }
    return {
      top: "0",
      bottom: "0",
      right: "0",
      width: "100%",
      "max-width": "420px",
      "border-left": "1px solid var(--border)",
    };
  };

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
          }}
        />
        <KobalteDialog.Content
          style={{
            position: "fixed",
            "z-index": "101",
            "background-color": "var(--bg-card)",
            padding: "var(--space-6)",
            "box-shadow": "var(--shadow-xl)",
            display: "flex",
            "flex-direction": "column",
            overflow: "auto",
            outline: "none",
            ...getPositionStyles(),
          }}
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "margin-bottom": "var(--space-6)",
              "border-bottom": "1px solid var(--border)",
              "padding-bottom": "var(--space-4)",
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
          {local.children}
        </KobalteDialog.Content>
      </KobalteDialog.Portal>
    </KobalteDialog>
  );
}
