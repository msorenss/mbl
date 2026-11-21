import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type ShowMissing = components["schemas"]["ShowMissing"];

/** The server's reason without its prefix ("validation: …"). */
export function serverReason(error: unknown, fallback: string): string {
  const text = (error as { message?: string } | undefined)?.message;
  return typeof text === "string" && text ? text.replace(/^[a-z_]+:\s*/, "") : fallback;
}

/** Ask for a title through the request page (Sonarr/Radarr when connected). */
export async function requestTitle(kind: "movie" | "show", tmdbId: number, title: string) {
  const result = await client.POST("/api/v1/requests", { body: { kind, tmdbId, title } });
  if (!result.data) throw new Error(serverReason(result.error, "Could not send the request."));
}

/**
 * Aired episodes a show lacks, compared with TMDB: gaps, episodes after the last one you have
 * and whole seasons. Specials and episodes that have not aired are left out.
 */
export default function MissingEpisodes(props: { showId: number }) {
  const [missing, setMissing] = createSignal<ShowMissing | null>(null);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const check = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/items/{id}/missing", {
        params: { path: { id: props.showId } },
      });
      if (!result.data)
        throw new Error(serverReason(result.error, "Could not check for missing episodes."));
      setMissing(result.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not check for missing episodes.");
    } finally {
      setLoading(false);
    }
  };

  const request = async (show: ShowMissing) => {
    if (!show.tmdbId) return;
    setLoading(true);
    setError("");
    try {
      await requestTitle("show", show.tmdbId, show.title);
      setNotice("Requested. You can follow it on the Requests page.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not send the request.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <section class="mbl-card-section" aria-label="Missing episodes">
      <Show
        when={missing()}
        fallback={
          <Button variant="secondary" onClick={check} disabled={loading()}>
            Check for missing episodes
          </Button>
        }
      >
        {(show) => (
          <Show
            when={show().tmdbId}
            fallback={<p>This show has no TMDB match, so it cannot be compared.</p>}
          >
            <Show
              when={show().missingCount > 0}
              fallback={<p>Every aired episode is in the library.</p>}
            >
              <h3 style={{ margin: 0 }}>{`Missing episodes (${show().missingCount})`}</h3>
              <For each={show().seasons}>
                {(season) => (
                  <div style={{ margin: "8px 0" }}>
                    <strong>
                      {season.name ?? `Season ${season.season}`}
                      <Show when={season.whole}> · the whole season</Show>
                    </strong>
                    <ul style={{ margin: "4px 0", "padding-left": "20px" }}>
                      <For each={season.episodes}>
                        {(e) => (
                          <li style={{ color: "var(--text-secondary)" }}>
                            {`E${e.episode}`}
                            <Show when={e.title}>{` · ${e.title}`}</Show>
                            <Show when={e.airDate}>{` · ${e.airDate}`}</Show>
                          </li>
                        )}
                      </For>
                    </ul>
                  </div>
                )}
              </For>
              <Button variant="secondary" disabled={loading()} onClick={() => request(show())}>
                Request the show
              </Button>
            </Show>
          </Show>
        )}
      </Show>
      <Show when={notice()}>
        <p role="status">{notice()}</p>
      </Show>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
    </section>
  );
}
