import { createSignal, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Button } from "./ui";

type ScanStatus = components["schemas"]["LibraryScanStatus"];

export function LibraryScanControl(props: { id: number; onComplete: () => void }) {
  const { t } = useI18n();
  const [job, setJob] = createSignal<ScanStatus>();
  const [submitting, setSubmitting] = createSignal(false);
  const [error, setError] = createSignal("");
  const active = () => job()?.status === "queued" || job()?.status === "running";
  const lifetime = new AbortController();
  let revision = 0;
  let checking = false;

  const refresh = async () => {
    if (checking || submitting() || lifetime.signal.aborted) return;
    checking = true;
    const version = revision;
    try {
      const result = await client.GET("/api/v1/libraries/{id}/scan", {
        params: { path: { id: props.id } },
        signal: AbortSignal.any([lifetime.signal, AbortSignal.timeout(10_000)]),
      });
      if (lifetime.signal.aborted || version !== revision) return;
      if (!result.response.ok)
        throw new Error(apiErrorMessage(result.error, t("Scan status is unavailable.")));
      const completed = active() && result.data?.status === "done";
      setJob(result.data);
      setError("");
      if (completed) props.onComplete();
    } catch (cause) {
      if (!lifetime.signal.aborted && version === revision)
        setError(apiErrorMessage(cause, t("Scan status is unavailable.")));
    } finally {
      checking = false;
    }
  };

  const start = async () => {
    if (submitting() || active()) return;
    revision++;
    setSubmitting(true);
    setError("");
    try {
      const { data, error } = await client.POST("/api/v1/libraries/{id}/scan", {
        params: { path: { id: props.id } },
        signal: AbortSignal.any([lifetime.signal, AbortSignal.timeout(15_000)]),
      });
      if (lifetime.signal.aborted) return;
      if (!data) throw new Error(apiErrorMessage(error, t("Could not queue the library scan.")));
      setJob({ jobId: data.jobId, status: "queued", progress: 0 });
    } catch (cause) {
      if (!lifetime.signal.aborted)
        setError(apiErrorMessage(cause, t("Could not queue the library scan.")));
    } finally {
      if (!lifetime.signal.aborted) {
        setSubmitting(false);
        if (active()) void refresh();
      }
    }
  };

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (active() && !document.hidden) void refresh();
    }, 3_000);
    onCleanup(() => window.clearInterval(timer));
  });
  onCleanup(() => lifetime.abort());

  const label = (status: string) => {
    switch (status) {
      case "queued":
        return t("Scan queued");
      case "running":
        return t("Scanning library");
      case "done":
        return t("Scan complete");
      case "failed":
        return t("Scan failed");
      case "cancelled":
        return t("Scan cancelled");
      default:
        return status;
    }
  };

  return (
    <div style={{ display: "grid", gap: "4px", "justify-items": "start" }}>
      <Button size="sm" variant="secondary" disabled={submitting() || active()} onClick={start}>
        {t("Scan Library")}
      </Button>
      <Show when={job()}>
        {(current) => (
          <small role="status">
            {label(current().status)}{" "}
            {current().status === "running" && current().progress > 0
              ? `${Math.round(current().progress * 100)}%`
              : ""}{" "}
            (#{current().jobId})
            <Show when={current().error}>
              <span style={{ display: "block", "overflow-wrap": "anywhere" }}>
                {current().error}
              </span>
            </Show>
          </small>
        )}
      </Show>
      <Show when={error()}>
        <small role="alert">{error()}</small>
      </Show>
    </div>
  );
}
