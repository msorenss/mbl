import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type Missing = components["schemas"]["MissingEpisodeInfo"];

/** Lists gaps in the episode numbering of a show's seasons. */
export default function MissingEpisodes(props: { showId: number }) {
  const [missing, setMissing] = createSignal<Missing[] | null>(null);
  const [error, setError] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const check = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/requests/missing-episodes/{showId}", {
        params: { path: { showId: props.showId } },
      });
      if (!result.data) {
        setError("Could not check for missing episodes.");
        return;
      }
      setMissing(result.data);
    } catch {
      setError("Could not reach the server.");
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
        {(list) => (
          <Show
            when={list().length > 0}
            fallback={<p>No gaps found. Episodes after the last one you have are not checked.</p>}
          >
            <h3 style={{ margin: 0 }}>Missing episodes</h3>
            <ul>
              <For each={list()}>
                {(season) => (
                  <li>
                    {`Season ${season.seasonNumber}: episode ${season.missingEpisodeNumbers.join(", ")}`}
                  </li>
                )}
              </For>
            </ul>
            <a href="/requests">Request them</a>
          </Show>
        )}
      </Show>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
    </section>
  );
}
