import { A } from "@solidjs/router";
import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { requestTitle, serverReason } from "./MissingEpisodes";
import { Button } from "./ui";

type Missing = components["schemas"]["LibraryMissing"];

/** What a TV or movie library lacks compared with TMDB, with buttons to request it. */
export default function LibraryMissing(props: { libraryId: number }) {
  const [missing, setMissing] = createSignal<Missing | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal("");
  const [requested, setRequested] = createSignal<number[]>([]);

  const check = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/libraries/{id}/missing", {
        params: { path: { id: props.libraryId } },
      });
      if (!result.data) throw new Error(serverReason(result.error, "Could not check the library."));
      setMissing(result.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not check the library.");
    } finally {
      setLoading(false);
    }
  };

  const request = async (kind: "movie" | "show", tmdbId: number, title: string) => {
    setError("");
    try {
      await requestTitle(kind, tmdbId, title);
      setRequested((list) => [...list, tmdbId]);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not send the request.");
    }
  };

  return (
    <section class="mbl-card-section" aria-label="Missing from the library">
      <Show
        when={missing()}
        fallback={
          <Button variant="secondary" onClick={check} disabled={loading()}>
            {loading() ? "Checking…" : "What's missing?"}
          </Button>
        }
      >
        {(m) => (
          <>
            <h2 style={{ margin: "0 0 8px" }}>Missing from the library</h2>
            <p style={{ margin: "0 0 8px", color: "var(--text-secondary)" }}>
              {`Compared ${m().checked} titles with TMDB.`}
              <Show when={m().withoutTmdb > 0}>
                {` ${m().withoutTmdb} have no TMDB match and were skipped.`}
              </Show>
              <Show when={m().truncated}> Only the first titles were checked.</Show>
            </p>
            <Show
              when={m().shows.length > 0 || m().movies.length > 0}
              fallback={<p>Nothing is missing.</p>}
            >
              <ul
                style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "6px" }}
              >
                <For each={m().shows}>
                  {(show) => (
                    <li style={{ display: "flex", gap: "8px", "align-items": "center" }}>
                      <A href={`/item/${show.showId}`} style={{ flex: 1 }}>
                        {show.title}
                      </A>
                      <span style={{ color: "var(--text-secondary)" }}>
                        {`${show.missingCount} episodes`}
                      </span>
                      <Show when={show.tmdbId}>
                        {(id) => (
                          <Button
                            size="sm"
                            variant="secondary"
                            disabled={requested().includes(id())}
                            onClick={() => request("show", id(), show.title)}
                          >
                            {requested().includes(id()) ? "Requested" : "Request"}
                          </Button>
                        )}
                      </Show>
                    </li>
                  )}
                </For>
                <For each={m().movies}>
                  {(movie) => (
                    <li style={{ display: "flex", gap: "8px", "align-items": "center" }}>
                      <span style={{ flex: 1 }}>
                        {movie.title}
                        <Show when={movie.year}>{` (${movie.year})`}</Show>
                        <span
                          style={{ color: "var(--text-secondary)" }}
                        >{` · ${movie.collection}`}</span>
                      </span>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={requested().includes(movie.tmdbId)}
                        onClick={() => request("movie", movie.tmdbId, movie.title)}
                      >
                        {requested().includes(movie.tmdbId) ? "Requested" : "Request"}
                      </Button>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
          </>
        )}
      </Show>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
    </section>
  );
}
