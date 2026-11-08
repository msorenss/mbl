import { createSignal, onCleanup, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import { useI18n } from "../i18n";
import { Button } from "./ui";
import { Dialog } from "./ui/Dialog";

export function RestartServerControl(props: { onRestart: () => void }) {
  const { t } = useI18n();
  const [confirm, setConfirm] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [status, setStatus] = createSignal("");
  const [error, setError] = createSignal("");
  const lifetime = new AbortController();
  onCleanup(() => lifetime.abort());

  const restart = async () => {
    if (busy()) return;
    setConfirm(false);
    setBusy(true);
    setError("");
    setStatus(t("Restarting server…"));
    try {
      const { data, error } = await client.POST("/api/v1/system/restart", {
        signal: AbortSignal.any([lifetime.signal, AbortSignal.timeout(10_000)]),
      });
      if (!data) throw new Error(apiErrorMessage(error, t("Could not restart the server.")));

      const deadline = performance.now() + 60_000;
      while (!lifetime.signal.aborted && performance.now() < deadline) {
        await new Promise((resolve) => window.setTimeout(resolve, 1_500));
        if (lifetime.signal.aborted) return;
        try {
          const result = await client.GET("/api/v1/system/info", {
            cache: "no-store",
            signal: AbortSignal.any([lifetime.signal, AbortSignal.timeout(3_000)]),
          });
          if (result.data && result.data.startedAt !== data.previousStartedAt) {
            setStatus(t("Server restarted."));
            props.onRestart();
            return;
          }
        } catch {
          // The listener is briefly unavailable during a successful restart.
        }
      }
      if (!lifetime.signal.aborted) throw new Error(t("The server has not come back online yet."));
    } catch (cause) {
      if (!lifetime.signal.aborted) {
        setStatus("");
        setError(apiErrorMessage(cause, t("Could not restart the server.")));
      }
    } finally {
      if (!lifetime.signal.aborted) setBusy(false);
    }
  };

  return (
    <>
      <Button size="sm" variant="outline" disabled={busy()} onClick={() => setConfirm(true)}>
        {t("Restart server")}
      </Button>
      <Show when={status()}>
        <span role="status">{status()}</span>
      </Show>
      <Show when={error()}>
        <span role="alert">{error()}</span>
      </Show>
      <Dialog
        open={confirm()}
        onOpenChange={setConfirm}
        title={t("Restart server")}
        description={t("Active playback will stop and the server will be briefly unavailable.")}
      >
        <div style={{ display: "flex", gap: "8px", "justify-content": "end" }}>
          <Button variant="secondary" onClick={() => setConfirm(false)}>
            {t("cancel")}
          </Button>
          <Button variant="danger" onClick={restart}>
            {t("Restart server")}
          </Button>
        </div>
      </Dialog>
    </>
  );
}
