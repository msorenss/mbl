import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { Badge, Button } from "../../components/ui";
import { useI18n } from "../../i18n";
import { browserLogs } from "../../utils/browserLogs";
import { castDiagnostics } from "../../utils/castDiagnostics";

type ServerEntry = components["schemas"]["ServerLogEntry"];
type LogEntry = Omit<ServerEntry, "details"> & {
  source: "server" | "browser" | "cast";
  details: Record<string, string | number | boolean | null>;
};

const PAGE_SIZE = 100;

export default function Logs() {
  const { t, locale } = useI18n();
  const [server, setServer] = createSignal<ServerEntry[]>([]);
  const [browser, setBrowser] = createSignal(browserLogs.snapshot());
  const [cast, setCast] = createSignal(castDiagnostics.getHistory());
  const [loading, setLoading] = createSignal(false);
  const [serverError, setServerError] = createSignal("");
  const [live, setLive] = createSignal(true);
  const [source, setSource] = createSignal("");
  const [level, setLevel] = createSignal("");
  const [query, setQuery] = createSignal("");
  const [visible, setVisible] = createSignal(PAGE_SIZE);
  const [copyStatus, setCopyStatus] = createSignal("");
  const [manualCopy, setManualCopy] = createSignal("");
  let request: AbortController | undefined;
  let disposed = false;

  const refresh = async () => {
    if (request || disposed) return;
    const controller = new AbortController();
    request = controller;
    setLoading(true);
    const timeout = window.setTimeout(() => controller.abort(), 12_000);
    try {
      const { data, response } = await client.GET("/api/v1/system/logs", {
        signal: controller.signal,
      });
      if (disposed) return;
      if (data) {
        setServer(data);
        setServerError("");
      } else {
        if (response.status === 401 || response.status === 403) setServer([]);
        setServerError(`HTTP ${response.status}`);
      }
    } catch {
      if (!disposed) setServerError(t("Check the connection and try again."));
    } finally {
      window.clearTimeout(timeout);
      request = undefined;
      if (!disposed) setLoading(false);
    }
  };

  onMount(() => {
    void refresh();
    const timer = window.setInterval(() => {
      if (live() && !document.hidden) void refresh();
    }, 5_000);
    onCleanup(() => window.clearInterval(timer));
  });
  onCleanup(browserLogs.subscribe(() => setBrowser(browserLogs.snapshot())));
  onCleanup(castDiagnostics.subscribe(() => setCast(castDiagnostics.getHistory())));
  onCleanup(() => {
    disposed = true;
    request?.abort();
  });

  const entries = createMemo<LogEntry[]>(() => {
    const all: LogEntry[] = [
      ...server().map((entry) => ({ ...entry, source: "server" as const })),
      ...browser().map((entry) => ({ ...entry, source: "browser" as const })),
      ...cast()
        .map(
          (entry): LogEntry => ({
            source: "cast",
            target: "chromecast",
            timestampMs: Date.parse(entry.at),
            level: /[._](error|failed)$/.test(entry.stage) ? "error" : "info",
            message: entry.stage,
            details: { elapsedMs: entry.elapsedMs, ...entry.details },
          }),
        )
        .reverse(),
    ];
    return all.sort((a, b) => b.timestampMs - a.timestampMs);
  });
  const matches = createMemo(() => {
    const needle = query().trim().toLowerCase();
    return entries().filter((entry) => {
      if (source() && entry.source !== source()) return false;
      if (level() && entry.level !== level()) return false;
      return (
        !needle ||
        `${entry.target} ${entry.message} ${JSON.stringify(entry.details)}`
          .toLowerCase()
          .includes(needle)
      );
    });
  });
  const shown = createMemo(() => matches().slice(0, visible()));
  const timeFormat = createMemo(
    () => new Intl.DateTimeFormat(locale(), { dateStyle: "short", timeStyle: "medium" }),
  );

  const sourceLabel = (value: LogEntry["source"]) =>
    value === "server" ? t("Server") : value === "browser" ? t("diagnosticBrowser") : "Chromecast";
  const copy = async () => {
    const report = JSON.stringify({ version: 1, events: matches() }, null, 2);
    setManualCopy("");
    try {
      await navigator.clipboard.writeText(report);
      if (!disposed) setCopyStatus(t("castDiagnosticsCopied"));
    } catch {
      if (!disposed) {
        setCopyStatus(t("castDiagnosticsFallback"));
        setManualCopy(report);
      }
    }
  };

  return (
    <div class="mbl-page">
      <h1>{t("diagnosticLogs")}</h1>
      <p>{t("diagnosticLogsHelp")}</p>
      <section class="mbl-card-section" aria-label={t("diagnosticLogs")}>
        <div style={{ display: "flex", "flex-wrap": "wrap", gap: "12px", "align-items": "end" }}>
          <label>
            {t("diagnosticSource")}{" "}
            <select
              value={source()}
              onChange={(e) => {
                setSource(e.currentTarget.value);
                setVisible(PAGE_SIZE);
              }}
            >
              <option value="">{t("diagnosticAllSources")}</option>
              <For each={["server", "browser", "cast"] as const}>
                {(value) => <option value={value}>{sourceLabel(value)}</option>}
              </For>
            </select>
          </label>
          <label>
            {t("diagnosticLevel")}{" "}
            <select
              value={level()}
              onChange={(e) => {
                setLevel(e.currentTarget.value);
                setVisible(PAGE_SIZE);
              }}
            >
              <option value="">{t("diagnosticAllLevels")}</option>
              <For each={["error", "warn", "info", "debug", "trace"]}>
                {(value) => <option value={value}>{value.toUpperCase()}</option>}
              </For>
            </select>
          </label>
          <label>
            {t("search")}{" "}
            <input
              type="search"
              value={query()}
              onInput={(e) => {
                setQuery(e.currentTarget.value);
                setVisible(PAGE_SIZE);
              }}
            />
          </label>
          <Button size="sm" variant="secondary" disabled={loading()} onClick={() => void refresh()}>
            {t("refresh")}
          </Button>
          <Button size="sm" variant="secondary" disabled={!matches().length} onClick={copy}>
            {t("diagnosticCopyEvents")}
          </Button>
          <label>
            <input
              type="checkbox"
              checked={live()}
              onChange={(e) => setLive(e.currentTarget.checked)}
            />{" "}
            {t("diagnosticAutoRefresh")}
          </label>
        </div>
        <Show when={copyStatus()}>
          <p role="status">{copyStatus()}</p>
        </Show>
        <Show when={manualCopy()}>
          <textarea
            aria-label={t("diagnosticLogs")}
            readOnly
            value={manualCopy()}
            rows={8}
            spellcheck={false}
            style={{ width: "100%" }}
          />
        </Show>
        <Show when={serverError()}>
          <p role="alert">
            {t("diagnosticLoadError")} {serverError()}
          </p>
        </Show>
        <p>
          {t("diagnosticEventCount")} {shown().length} / {matches().length}
        </p>
        <Show when={shown().length > 0} fallback={<p>{t("diagnosticEmpty")}</p>}>
          <ul style={{ display: "grid", gap: "8px", padding: 0, "list-style": "none" }}>
            <For each={shown()}>
              {(entry) => (
                <li
                  style={{
                    "border-top": "1px solid var(--border)",
                    padding: "8px 0",
                    "overflow-wrap": "anywhere",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      "flex-wrap": "wrap",
                      gap: "8px",
                      "align-items": "center",
                    }}
                  >
                    <time dateTime={new Date(entry.timestampMs).toISOString()}>
                      {timeFormat().format(entry.timestampMs)}
                    </time>
                    <Badge
                      variant={
                        entry.level === "error"
                          ? "danger"
                          : entry.level === "warn"
                            ? "warning"
                            : "default"
                      }
                    >
                      {entry.level}
                    </Badge>
                    <span>{sourceLabel(entry.source)}</span>
                    <code>{entry.target}</code>
                  </div>
                  <div>
                    <code>{entry.message}</code>
                  </div>
                  <Show when={Object.keys(entry.details).length > 0}>
                    <small>
                      <code>
                        {Object.entries(entry.details)
                          .map(([key, value]) => `${key}=${value}`)
                          .join(" · ")}
                      </code>
                    </small>
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </Show>
        <Show when={shown().length < matches().length}>
          <Button variant="secondary" onClick={() => setVisible((n) => n + PAGE_SIZE)}>
            {t("diagnosticMore")}
          </Button>
        </Show>
      </section>
    </div>
  );
}
