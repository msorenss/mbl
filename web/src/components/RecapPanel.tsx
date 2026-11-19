import { createSignal, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type Recap = components["schemas"]["SpoilerFreeRecap"];

/** "Previously on" summary built only from episodes the viewer has watched. */
export default function RecapPanel(props: { itemId: number }) {
  const [recap, setRecap] = createSignal<Recap | null>(null);
  const [error, setError] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/ai/recap/{id}", {
        params: { path: { id: props.itemId } },
      });
      if (!result.data) {
        setError("Could not create a recap.");
        return;
      }
      setRecap(result.data);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <section class="mbl-card-section" aria-label="Previously on">
      <Show
        when={recap()}
        fallback={
          <Button variant="secondary" onClick={load} disabled={loading()}>
            {loading() ? "Writing recap…" : "Previously on…"}
          </Button>
        }
      >
        {(r) => (
          <>
            <h3 style={{ margin: 0 }}>{`Previously on ${r().showTitle}`}</h3>
            <Show
              when={r().episodesIncluded > 0}
              fallback={<p>You have not watched any episodes yet, so there is nothing to recap.</p>}
            >
              <p style={{ "white-space": "pre-line" }}>{r().recapText}</p>
              <small style={{ color: "var(--color-text-muted)" }}>
                {`Based on ${r().episodesIncluded} watched episodes. Episodes you have not seen are left out.`}
              </small>
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
