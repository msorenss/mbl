import { createSignal, For, onCleanup, Show } from "solid-js";
import client from "../api/client";
import { apiErrorMessage } from "../api/errors";
import type { components } from "../api/schema";
import { type TranslationKey, useI18n } from "../i18n";
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
    const add = (source: Source) => {
      if (version !== searchVersion) return;
      if (source.availability !== "media_detected") return;
      setVerified((previous) =>
        previous.some((s) => s.url === source.url) ? previous : [...previous, source],
      );
    };
    const queue = [...candidates];
    const worker = async () => {
      for (let source = queue.shift(); source; source = queue.shift()) {
        if (version !== searchVersion || signal.aborted) return;
        if (source.availability === "media_detected") {
          add(source);
          continue;
        }
        try {
          const res = await client.POST("/api/v1/online/inspect", {
            body: { url: source.url },
            signal: AbortSignal.any([signal, AbortSignal.timeout(21000)]),
          });
          if (res.data) add(res.data);
        } catch {
          // Unverifiable sources are simply not listed.
        }
      }
    };
    await Promise.all([worker(), worker(), worker()]);
    if (version === searchVersion) setVerifying(false);
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
    setSelected(undefined);
    setPreview(undefined);
    setPlayer(undefined);
    setError("");
    try {
      const res = await client.GET("/api/v1/online/search", {
        params: { query: { q: title } },
        signal: abort.signal,
      });
      if (version !== searchVersion) return;
      if (res.error || !res.data)
        throw new Error(apiErrorMessage(res.error, t("onlineSearchError")));
      setResults(res.data);
      setSearching(false);
      await verifyCandidates(res.data, version, abort.signal);
    } catch (cause) {
      if (version === searchVersion) setError(apiErrorMessage(cause, t("onlineSearchError")));
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

  const sourceCard = (source: Source) => (
    <article
      aria-label={source.title}
      style={{
        padding: "12px",
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-sm)",
        background: "var(--bg-base)",
        "overflow-wrap": "anywhere",
      }}
    >
      <strong>{source.title}</strong>
      <div style={{ color: "var(--text-muted)", "font-size": "12px", "margin-top": "4px" }}>
        {webUrl(source.url) ? new URL(source.url).hostname : ""}
      </div>
      <p>{source.summary}</p>
      <p>
        <span>{t(availabilityKeys[source.availability])}</span>
      </p>
      <Show
        when={source.seriesTitle || source.seasonNumber !== null || source.episodeNumber !== null}
      >
        <p>
          {source.seriesTitle}{" "}
          <Show when={source.seasonNumber !== null}>
            {t("onlineSeason")} {source.seasonNumber}
          </Show>{" "}
          <Show when={source.episodeNumber !== null}>
            {t("onlineEpisode")} {source.episodeNumber}
          </Show>
        </p>
      </Show>
      <div style={{ display: "flex", "flex-wrap": "wrap", gap: "8px", "align-items": "center" }}>
        <Button
          type="button"
          size="sm"
          disabled={checking() === source.url}
          onClick={() => void inspect(source.url)}
        >
          {checking() === source.url ? t("onlineChecking") : t("onlineCheck")}
        </Button>
      </div>
    </article>
  );
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
            <Show when={source.availability !== "media_detected"}>
              <p role="status">{t("onlineNotVerified")}</p>
            </Show>
            <Show when={source.availability === "media_detected" && !!source.mediaUrl}>
              <Button
                type="button"
                disabled={!!checking()}
                onClick={() => void inspect(source.url, true)}
              >
                {t("onlinePreview")}
              </Button>
            </Show>
            <Show when={source.availability === "media_detected" && !!source.mediaUrl}>
              <Button
                type="button"
                disabled={!!checking()}
                onClick={() => {
                  setPreview(undefined);
                  setPlayer(source);
                }}
              >
                {t("onlinePlay")}
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
            <p>
              {t("onlineProvider")}: {response.provider}
            </p>
            <h3>
              {t("onlineVerifiedResults")} — {response.query}
            </h3>
            <Show when={verifying()}>
              <p role="status">{t("onlineVerifying")}</p>
            </Show>
            <Show when={!verifying() && verified().length === 0}>
              <p>{t("onlineNoVerified")}</p>
            </Show>
            <For each={verified()}>{sourceCard}</For>
          </div>
        )}
      </Show>
    </div>
  );
}
