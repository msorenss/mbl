import { createSignal, For, onCleanup, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { type TranslationKey, useI18n } from "../i18n";
import { classifyStreamingProvider } from "../utils/streamingProviders";
import { OnlineHistory } from "./OnlineHistory";
import { OnlinePlayer } from "./OnlinePlayer";
import { OnlinePreview } from "./OnlinePreview";
import { OnlineProviderSettings } from "./OnlineProviderSettings";
import { Button } from "./ui/Button";

type Source = components["schemas"]["OnlineSource"];
type Results = components["schemas"]["OnlineSearchResponse"];
const availabilityKeys: Record<Source["availability"], TranslationKey> = {
  unchecked: "onlineUnchecked",
  external_page: "onlineExternalPage",
  external_player: "onlineExternalPlayer",
  media_detected: "onlineMediaDetected",
  restricted: "onlineRestricted",
  unavailable: "onlineUnavailable",
  unknown: "onlineUnknown",
};
const fieldStyle = {
  width: "100%",
  "min-width": "0",
  padding: "9px 12px",
  background: "var(--bg-base)",
  color: "var(--text-primary)",
  border: "1px solid var(--border)",
  "border-radius": "var(--radius-sm)",
} as const;
function webUrl(raw: string): string | undefined {
  try {
    const url = new URL(raw);
    return ["http:", "https:"].includes(url.protocol) && !url.username && !url.password
      ? url.href
      : undefined;
  } catch {
    return undefined;
  }
}

export const isOnlinePlayable = (s: Source) =>
  (s.availability === "media_detected" && !!s.mediaUrl) || s.availability === "external_player";

export function OnlineSearch(props: { admin?: boolean } = {}) {
  const { t } = useI18n();
  const [query, setQuery] = createSignal("");
  const [sourceUrl, setSourceUrl] = createSignal("");
  const [results, setResults] = createSignal<Results>();
  const [searching, setSearching] = createSignal(false);
  const [checking, setChecking] = createSignal<string>();
  const [selected, setSelected] = createSignal<Source>();
  const [historyVersion, setHistoryVersion] = createSignal(0);
  const [player, setPlayer] = createSignal<Source>();
  const [preview, setPreview] = createSignal<Source>();
  const [error, setError] = createSignal("");
  const [verified, setVerified] = createSignal<Source[]>([]);
  const [verifying, setVerifying] = createSignal(false);
  const [resultsTab, setResultsTab] = createSignal<"verified" | "all">("verified");
  let searchRequest: AbortController | undefined;
  let checkRequest: AbortController | undefined;
  let searchDeadline: ReturnType<typeof setTimeout> | undefined;
  let checkDeadline: ReturnType<typeof setTimeout> | undefined;
  let searchVersion = 0;
  let checkVersion = 0;
  let detailElement: HTMLElement | undefined;

  const verifyCandidates = async (data: Results, version: number, signal: AbortSignal) => {
    const seen = new Set<string>();
    const candidates = [...data.results, ...data.indexed]
      .filter((source) => {
        if (seen.has(source.url) || !webUrl(source.url)) return false;
        seen.add(source.url);
        return true;
      })
      .slice(0, 8);
    setVerifying(true);
    console.info(
      `[WebTV] Verifierar de ${candidates.length} främsta sökträffarna för direkt videoström...`,
    );
    const add = (source: Source) => {
      if (version !== searchVersion) return;
      if (!isOnlinePlayable(source)) return;
      setVerified((previous) =>
        previous.some((s) => s.url === source.url) ? previous : [...previous, source],
      );
    };
    const queue = [...candidates];
    const worker = async () => {
      for (let source = queue.shift(); source; source = queue.shift()) {
        if (version !== searchVersion || signal.aborted) return;
        if (isOnlinePlayable(source)) {
          add(source);
          continue;
        }
        try {
          const res = await client.POST("/api/v1/online/inspect", {
            body: { url: source.url },
            signal: AbortSignal.any([signal, AbortSignal.timeout(21000)]),
          });
          if (res.data) {
            console.info(
              `[WebTV] Inspekterade "${source.title}" (${source.url}) => ${res.data.availability}: ${res.data.detail}`,
            );
            add(res.data);
          }
        } catch (err) {
          console.warn(`[WebTV] Kunde inte inspektera "${source.title}" (${source.url}):`, err);
        }
      }
    };
    await Promise.all([worker(), worker(), worker()]);
    if (version === searchVersion) {
      setVerifying(false);
      console.info(`[WebTV] Verifiering slutförd: ${verified().length} spelbara källor hittades.`);
    }
  };
  const search = async () => {
    const title = query().trim();
    if (title.length < 2 || title.length > 160) return;
    const version = ++searchVersion;
    searchRequest?.abort();
    clearTimeout(searchDeadline);
    checkVersion++;
    checkRequest?.abort();
    clearTimeout(checkDeadline);
    setChecking(undefined);
    const abort = new AbortController();
    searchRequest = abort;
    searchDeadline = setTimeout(() => abort.abort(), 120000);
    setSearching(true);
    setResults(undefined);
    setVerified([]);
    setVerifying(false);
    setResultsTab("verified");
    setSelected(undefined);
    setPreview(undefined);
    setPlayer(undefined);
    setError("");
    console.info(`[WebTV] Söker på webben efter "${title}"...`);
    try {
      const res = await client.GET("/api/v1/online/search", {
        params: { query: { q: title } },
        signal: abort.signal,
      });
      if (version !== searchVersion) return;
      if (res.error || !res.data)
        throw new Error(apiErrorMessage(res.error, t("onlineSearchError")));
      console.info(
        `[WebTV] Leverantör "${res.data.provider}" returnerade ${res.data.results.length} träffar och ${res.data.indexed.length} indexerade`,
      );
      setResults(res.data);
      setSearching(false);
      await verifyCandidates(res.data, version, abort.signal);
    } catch (cause) {
      if (version === searchVersion) {
        console.warn(`[WebTV] Sökning misslyckades:`, cause);
        setError(apiErrorMessage(cause, t("onlineSearchError")));
      }
    } finally {
      if (version === searchVersion) {
        clearTimeout(searchDeadline);
        setSearching(false);
      }
    }
  };
  const inspect = async (url: string, openPreview = false) => {
    if (!webUrl(url)) {
      setError(t("onlineSourceError"));
      return;
    }
    const version = ++checkVersion;
    checkRequest?.abort();
    clearTimeout(checkDeadline);
    const abort = new AbortController();
    checkRequest = abort;
    checkDeadline = setTimeout(() => abort.abort(), 21000);
    setChecking(url);
    setError("");
    setSelected(undefined);
    setPreview(undefined);
    setPlayer(undefined);
    try {
      const res = await client.POST("/api/v1/online/inspect", {
        body: { url },
        signal: abort.signal,
      });
      if (version !== checkVersion) return;
      if (res.error || !res.data)
        throw new Error(apiErrorMessage(res.error, t("onlineSourceError")));
      const source = res.data;
      setSelected(source);
      queueMicrotask(() => {
        if (version === checkVersion && !openPreview)
          detailElement?.scrollIntoView?.({ block: "nearest" });
      });
      setResults((previous) =>
        previous
          ? {
              ...previous,
              results: previous.results.map((s) => (s.id === source.id ? source : s)),
              indexed: previous.indexed.map((s) => (s.id === source.id ? source : s)),
            }
          : previous,
      );
      setVerified((previous) =>
        previous.map((s) => (s.id === source.id ? source : s)).filter(isOnlinePlayable),
      );
      if (
        openPreview &&
        source.availability === "media_detected" &&
        source.mediaUrl &&
        webUrl(source.mediaUrl)
      )
        setPreview(source);
    } catch (cause) {
      if (version === checkVersion) setError(apiErrorMessage(cause, t("onlineSourceError")));
    } finally {
      if (version === checkVersion) {
        clearTimeout(checkDeadline);
        setChecking(undefined);
      }
    }
  };
  onCleanup(() => {
    searchVersion++;
    checkVersion++;
    searchRequest?.abort();
    checkRequest?.abort();
    clearTimeout(searchDeadline);
    clearTimeout(checkDeadline);
  });

  const sourceCard = (source: Source) => {
    const info = classifyStreamingProvider(
      source.url,
      source,
      t("onlineTitle").includes("WebTV") ? "sv" : "en",
    );
    return (
      <article
        aria-label={source.title}
        style={{
          padding: "14px",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-sm)",
          background: "var(--bg-base)",
          "overflow-wrap": "anywhere",
          display: "flex",
          "flex-direction": "column",
          gap: "8px",
        }}
      >
        <div
          style={{
            display: "flex",
            "align-items": "flex-start",
            "justify-content": "space-between",
            gap: "8px",
            "flex-wrap": "wrap",
          }}
        >
          <div>
            <strong style={{ "font-size": "15px" }}>{source.title}</strong>
            <div style={{ color: "var(--text-muted)", "font-size": "12px", "margin-top": "2px" }}>
              {info.siteName !== info.hostname ? `${info.siteName} • ` : ""}
              {info.hostname}
            </div>
          </div>
          <div style={{ display: "flex", gap: "6px", "flex-wrap": "wrap" }}>
            <span
              style={{
                "font-size": "11px",
                "font-weight": "600",
                padding: "3px 8px",
                "border-radius": "var(--radius-sm)",
                background:
                  info.streamingBadge.variant === "accent"
                    ? "var(--accent)"
                    : info.streamingBadge.variant === "success"
                      ? "rgba(34, 197, 94, 0.15)"
                      : "var(--bg-overlay)",
                color:
                  info.streamingBadge.variant === "accent"
                    ? "#ffffff"
                    : info.streamingBadge.variant === "success"
                      ? "#22c55e"
                      : "var(--text-muted)",
                border:
                  info.streamingBadge.variant === "success"
                    ? "1px solid rgba(34, 197, 94, 0.3)"
                    : "1px solid var(--border)",
              }}
            >
              {info.streamingBadge.text}
            </span>
            <span
              style={{
                "font-size": "11px",
                "font-weight": "600",
                padding: "3px 8px",
                "border-radius": "var(--radius-sm)",
                background:
                  info.paymentBadge.variant === "warning"
                    ? "rgba(234, 179, 8, 0.15)"
                    : info.paymentBadge.variant === "info"
                      ? "rgba(59, 130, 246, 0.15)"
                      : info.paymentBadge.variant === "success"
                        ? "rgba(16, 185, 129, 0.15)"
                        : "var(--bg-overlay)",
                color:
                  info.paymentBadge.variant === "warning"
                    ? "#eab308"
                    : info.paymentBadge.variant === "info"
                      ? "#3b82f6"
                      : info.paymentBadge.variant === "success"
                        ? "#10b981"
                        : "var(--text-muted)",
                border:
                  info.paymentBadge.variant === "warning"
                    ? "1px solid rgba(234, 179, 8, 0.3)"
                    : info.paymentBadge.variant === "info"
                      ? "1px solid rgba(59, 130, 246, 0.3)"
                      : info.paymentBadge.variant === "success"
                        ? "1px solid rgba(16, 185, 129, 0.3)"
                        : "1px solid var(--border)",
              }}
            >
              {info.paymentBadge.text}
            </span>
          </div>
        </div>

        <Show when={source.summary}>
          <p style={{ margin: "2px 0", "font-size": "13px", color: "var(--text-secondary)" }}>
            {source.summary}
          </p>
        </Show>

        <p style={{ margin: 0, "font-size": "12px", color: "var(--text-muted)" }}>
          <span>{t(availabilityKeys[source.availability])}</span>
          {source.detail && source.detail !== "Public web result" ? ` — ${source.detail}` : ""}
        </p>

        <Show
          when={source.seriesTitle || source.seasonNumber !== null || source.episodeNumber !== null}
        >
          <p style={{ margin: "2px 0", "font-size": "13px" }}>
            {source.seriesTitle}{" "}
            <Show when={source.seasonNumber !== null}>
              {t("onlineSeason")} {source.seasonNumber}
            </Show>{" "}
            <Show when={source.episodeNumber !== null}>
              {t("onlineEpisode")} {source.episodeNumber}
            </Show>
          </p>
        </Show>

        <div
          style={{
            display: "flex",
            "flex-wrap": "wrap",
            gap: "8px",
            "align-items": "center",
            "margin-top": "4px",
          }}
        >
          <Button
            type="button"
            size="sm"
            disabled={checking() === source.url}
            onClick={() => void inspect(source.url)}
          >
            {checking() === source.url ? t("onlineChecking") : t("onlineCheck")}
          </Button>

          <Show when={isOnlinePlayable(source)}>
            <Button
              type="button"
              size="sm"
              onClick={() => {
                setPreview(undefined);
                setPlayer(source);
              }}
            >
              ▶ {t("onlinePlay")}
            </Button>
          </Show>

          <Show when={source.availability === "media_detected" && !!source.mediaUrl}>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={checking() === source.url}
              onClick={() => void inspect(source.url, true)}
            >
              {t("onlinePreview")}
            </Button>
          </Show>

          <Show when={webUrl(source.url)}>
            <a
              href={source.url}
              target="_blank"
              rel="noopener noreferrer"
              class="mbl-button"
              style={{
                display: "inline-flex",
                "align-items": "center",
                gap: "6px",
                "text-decoration": "none",
                padding: "6px 12px",
                "font-size": "var(--font-size-sm)",
                "border-radius": "var(--radius-sm)",
                background: "var(--bg-overlay)",
                color: "var(--text-primary)",
                border: "1px solid var(--border)",
                cursor: "pointer",
              }}
            >
              🌐 {t("onlineOpen")} ↗
            </a>
          </Show>
        </div>
      </article>
    );
  };
  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        gap: "16px",
        "overflow-wrap": "anywhere",
      }}
    >
      <Show when={player()} keyed>
        {(source) => (
          <div
            style={{
              background: "var(--bg-surface)",
              border: "2px solid var(--accent)",
              "border-radius": "var(--radius-lg)",
              padding: "16px",
              "box-shadow": "0 8px 32px rgba(0, 0, 0, 0.4)",
            }}
          >
            <OnlinePlayer
              source={source}
              onClose={() => setPlayer(undefined)}
              onSaved={() => setHistoryVersion((v) => v + 1)}
            />
          </div>
        )}
      </Show>
      <Show when={preview()} keyed>
        {(source) => (
          <div
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-lg)",
              padding: "16px",
            }}
          >
            <OnlinePreview source={source} onClose={() => setPreview(undefined)} />
          </div>
        )}
      </Show>
      <For each={[historyVersion()]}>
        {() => (
          <OnlineHistory
            onPlay={(source) => {
              setPreview(undefined);
              setPlayer(source);
            }}
          />
        )}
      </For>
      <Show when={props.admin}>
        <OnlineProviderSettings />
      </Show>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void search();
        }}
        style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
      >
        <label for="online-query">{t("onlineSearchLabel")}</label>
        <input
          id="online-query"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          placeholder="Single’s Inferno"
          minlength={2}
          maxlength={160}
          required
          style={fieldStyle}
        />
        <Button type="submit" disabled={searching() || query().trim().length < 2}>
          {searching() ? t("onlineSearching") : t("onlineSearchAction")}
        </Button>
      </form>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void inspect(sourceUrl().trim());
        }}
        style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
      >
        <label for="online-source-url">{t("onlineSourceUrlLabel")}</label>
        <input
          id="online-source-url"
          type="url"
          required
          value={sourceUrl()}
          onInput={(e) => setSourceUrl(e.currentTarget.value)}
          placeholder="https://…"
          style={fieldStyle}
        />
        <Button
          type="submit"
          variant="secondary"
          disabled={!sourceUrl().trim() || checking() === sourceUrl().trim()}
        >
          {t("onlineCheck")}
        </Button>
      </form>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={searching()}>
        <p role="status">{t("onlineSearching")}</p>
      </Show>
      <Show when={checking()}>
        <p role="status">{t("onlineChecking")}</p>
      </Show>
      <Show when={selected()} keyed>
        {(source) => (
          <section ref={detailElement} aria-label={t("onlineCheckedSource")}>
            <h3>{t("onlineCheckedSource")}</h3>
            {sourceCard(source)}
            <p>{source.detail}</p>
            <Show when={!isOnlinePlayable(source)}>
              <p role="status">{t("onlineNotVerified")}</p>
            </Show>
            <Show when={isOnlinePlayable(source)}>
              <Button
                type="button"
                disabled={!!checking()}
                onClick={() => {
                  setPreview(undefined);
                  setPlayer(source);
                }}
              >
                ▶ {t("onlinePlay")}
              </Button>
            </Show>
            <Show when={source.availability === "media_detected" && !!source.mediaUrl}>
              <Button
                type="button"
                variant="secondary"
                disabled={!!checking()}
                onClick={() => void inspect(source.url, true)}
              >
                {t("onlinePreview")}
              </Button>
            </Show>
            <Show when={source.episodes.length > 0}>
              <h4>{t("episodes")}</h4>
              <ol style={{ "padding-left": "20px" }}>
                <For each={source.episodes}>
                  {(episode) => (
                    <li style={{ "margin-bottom": "8px" }}>
                      <Show when={episode.seasonNumber !== null}>
                        {t("onlineSeason")} {episode.seasonNumber}{" "}
                      </Show>
                      <Show when={episode.episodeNumber !== null}>
                        {t("onlineEpisode")} {episode.episodeNumber}:{" "}
                      </Show>
                      {episode.title}{" "}
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        onClick={() => void inspect(episode.url)}
                      >
                        {t("onlineCheck")}
                      </Button>
                    </li>
                  )}
                </For>
              </ol>
            </Show>
          </section>
        )}
      </Show>
      <Show when={results()} keyed>
        {(response) => (
          <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
            <Show when={response.warning}>
              <p role="status">{response.warning}</p>
            </Show>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
                "flex-wrap": "wrap",
                gap: "8px",
                "border-bottom": "1px solid var(--border)",
                "padding-bottom": "8px",
              }}
            >
              <p style={{ margin: 0, "font-size": "13px", color: "var(--text-muted)" }}>
                {t("onlineProvider")}: <strong>{response.provider}</strong> •{" "}
                {response.results.length} {t("resultsCount")}
              </p>
              <div style={{ display: "flex", gap: "6px" }}>
                <Button
                  type="button"
                  size="sm"
                  variant={resultsTab() === "verified" ? "primary" : "secondary"}
                  onClick={() => setResultsTab("verified")}
                >
                  ✓ {t("onlineVerifiedResults")} ({verified().length})
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant={resultsTab() === "all" ? "primary" : "secondary"}
                  onClick={() => setResultsTab("all")}
                >
                  ☰ {t("onlineResults")} ({response.results.length})
                </Button>
              </div>
            </div>

            <Show when={resultsTab() === "verified"}>
              <Show when={verifying()}>
                <p
                  role="status"
                  style={{
                    color: "var(--accent)",
                    display: "flex",
                    "align-items": "center",
                    gap: "6px",
                  }}
                >
                  <span>⏳</span> {t("onlineVerifying")}
                </p>
              </Show>
              <Show when={!verifying() && verified().length === 0}>
                <div
                  style={{
                    padding: "16px",
                    background: "var(--bg-surface)",
                    border: "1px dashed var(--border)",
                    "border-radius": "var(--radius-md)",
                    display: "flex",
                    "flex-direction": "column",
                    gap: "10px",
                  }}
                >
                  <div>
                    <p style={{ margin: "0 0 6px 0", "font-weight": "600" }}>
                      {t("onlineNoVerified")}
                    </p>
                    <p style={{ margin: 0, "font-size": "13px", color: "var(--text-secondary)" }}>
                      Commercial series are often hosted on external streaming services that require
                      a browser or sign-in. You can view and open all {response.results.length}{" "}
                      search results with web links under the{" "}
                      <strong>"{t("onlineResults")}"</strong> tab.
                    </p>
                  </div>
                  <div>
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      onClick={() => setResultsTab("all")}
                    >
                      ☰ View all {response.results.length} search results with web links
                    </Button>
                  </div>
                </div>
              </Show>
              <For each={verified()}>{sourceCard}</For>
            </Show>

            <Show when={resultsTab() === "all"}>
              <p style={{ margin: 0, "font-size": "13px", color: "var(--text-secondary)" }}>
                Showing all {response.results.length} search results from {response.provider}. Click
                "{t("onlineCheck")}" to inspect a page for direct video.
              </p>
              <For each={response.results}>{sourceCard}</For>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}
