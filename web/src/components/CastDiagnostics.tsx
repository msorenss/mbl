import { createSignal, onCleanup } from "solid-js";
import { useI18n } from "../i18n";
import { castDiagnostics } from "../utils/castDiagnostics";

export default function CastDiagnostics() {
  const { t } = useI18n();
  const [version, setVersion] = createSignal(0);
  const [copied, setCopied] = createSignal("");
  let disposed = false;
  onCleanup(castDiagnostics.subscribe(() => setVersion((v) => v + 1)));
  onCleanup(() => {
    disposed = true;
  });
  const report = () => {
    version();
    return castDiagnostics.export();
  };
  return (
    <details style={{ "margin-top": "12px" }}>
      <summary>{t("castDiagnosticsTitle")}</summary>
      <p>{t("castDiagnosticsHelp")}</p>
      <button
        type="button"
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(report());
            if (!disposed) setCopied(t("castDiagnosticsCopied"));
          } catch {
            if (!disposed) setCopied(t("castDiagnosticsFallback"));
          }
        }}
      >
        {t("castDiagnosticsCopy")}
      </button>
      <span role="status">{copied()}</span>
      <textarea
        aria-label={t("castDiagnosticsLog")}
        readOnly
        value={report()}
        spellcheck={false}
        style={{ width: "100%", height: "180px", "box-sizing": "border-box", "font-size": "11px" }}
      />
    </details>
  );
}
