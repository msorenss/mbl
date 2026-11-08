import { Dialog as KobalteDialog } from "@kobalte/core/dialog";
import { type JSX, Show, splitProps } from "solid-js";
import { useI18n } from "../../i18n";

export interface SheetProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  title?: string;
  side?: "right" | "bottom";
  children: JSX.Element;
  trigger?: JSX.Element;
}

export function Sheet(props: SheetProps) {
  const { t } = useI18n();
  const [local, rest] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "side",
    "children",
    "trigger",
  ]);
  const side = () => local.side || "right";

  const getPositionStyles = (): JSX.CSSProperties => {
    if (side() === "bottom") {
      return {
        bottom: "0",
        left: "0",
        right: "0",
        "max-height": "86dvh",
        "border-top-left-radius": "var(--radius-xl)",
        "border-top-right-radius": "var(--radius-xl)",
        "border-top": "1px solid var(--border)",
        padding:
          "var(--space-4) max(var(--space-5), env(safe-area-inset-right)) calc(var(--space-6) + env(safe-area-inset-bottom)) max(var(--space-5), env(safe-area-inset-left))",
      };
    }
    return {
      top: "0",
      bottom: "0",
      right: "0",
      width: "100%",
      "max-width": "420px",
      "border-left": "1px solid var(--border)",
      padding:
        "calc(var(--space-5) + env(safe-area-inset-top)) max(var(--space-5), env(safe-area-inset-right)) calc(var(--space-5) + env(safe-area-inset-bottom)) var(--space-5)",
    };
  };

  return (
    <KobalteDialog open={local.open} onOpenChange={local.onOpenChange} {...rest}>
      {local.trigger && <KobalteDialog.Trigger as="div">{local.trigger}</KobalteDialog.Trigger>}
      <KobalteDialog.Portal>
        <KobalteDialog.Overlay
          class="mbl-overlay"
          style={{
            position: "fixed",
            inset: "0",
            "background-color": "rgba(0, 0, 0, 0.72)",
            "backdrop-filter": "blur(6px)",
            "z-index": "100",
          }}
        />
        <KobalteDialog.Content
          class={side() === "bottom" ? "mbl-sheet-bottom" : "mbl-sheet-right"}
          style={{
            position: "fixed",
            "z-index": "101",
            "background-color": "var(--bg-card)",
            "box-shadow": "var(--shadow-xl)",
            display: "flex",
            "flex-direction": "column",
            overflow: "auto",
            outline: "none",
            ...getPositionStyles(),
          }}
        >
          <Show when={side() === "bottom"}>
            <div
              aria-hidden="true"
              style={{
                width: "40px",
                height: "4px",
                "border-radius": "var(--radius-full)",
                background: "var(--border)",
                margin: "0 auto var(--space-3)",
                "flex-shrink": 0,
              }}
            />
          </Show>
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "margin-bottom": "var(--space-5)",
              "border-bottom": "1px solid var(--border)",
              "padding-bottom": "var(--space-3)",
              "flex-shrink": 0,
            }}
          >
            {local.title && (
              <KobalteDialog.Title
                style={{
                  margin: "0",
                  "font-size": "var(--font-size-lg)",
                  "font-weight": "var(--font-weight-bold)",
                  color: "var(--text-primary)",
                }}
              >
                {t(local.title)}
              </KobalteDialog.Title>
            )}
            <KobalteDialog.CloseButton
              aria-label={t("Close")}
              style={{
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-full)",
                width: "32px",
                height: "32px",
                display: "inline-flex",
                "align-items": "center",
                "justify-content": "center",
                color: "var(--text-secondary)",
                "font-size": "15px",
                cursor: "pointer",
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
