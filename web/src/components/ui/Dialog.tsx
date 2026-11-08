import { Dialog as KobalteDialog } from "@kobalte/core/dialog";
import { type JSX, splitProps } from "solid-js";
import { useI18n } from "../../i18n";

export interface DialogProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  title?: string;
  description?: string;
  children: JSX.Element;
  trigger?: JSX.Element;
  onOpenAutoFocus?: (event: Event) => void;
  onCloseAutoFocus?: (event: Event) => void;
}

export function Dialog(props: DialogProps) {
  const { t } = useI18n();
  const [local, rest] = splitProps(props, [
    "open",
    "onOpenChange",
    "title",
    "description",
    "children",
    "trigger",
    "onOpenAutoFocus",
    "onCloseAutoFocus",
  ]);

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
        <div
          style={{
            position: "fixed",
            inset: "0",
            "z-index": "101",
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
            padding:
              "max(var(--space-4), env(safe-area-inset-top)) max(var(--space-4), env(safe-area-inset-right)) max(var(--space-4), env(safe-area-inset-bottom)) max(var(--space-4), env(safe-area-inset-left))",
          }}
        >
          <KobalteDialog.Content
            class="mbl-dialog-content"
            onOpenAutoFocus={local.onOpenAutoFocus}
            onCloseAutoFocus={local.onCloseAutoFocus}
            style={{
              width: "100%",
              "max-width": "500px",
              "max-height": "calc(100dvh - 2 * var(--space-4))",
              "overflow-y": "auto",
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
                  {t(local.title)}
                </KobalteDialog.Title>
              )}
              <KobalteDialog.CloseButton
                aria-label={t("Close")}
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
                {t(local.description)}
              </KobalteDialog.Description>
            )}
            {local.children}
          </KobalteDialog.Content>
        </div>
      </KobalteDialog.Portal>
    </KobalteDialog>
  );
}
