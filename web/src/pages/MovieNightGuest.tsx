import { useParams } from "@solidjs/router";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { formatPlannedTime } from "../utils/plannedTime";
import { seededShuffle } from "../utils/seededShuffle";

type View = components["schemas"]["MatchmakerGuestView"];
type Title = components["schemas"]["MatchmakerGuestTitle"];

const tokenKey = (code: string) => `mbl.movieNight.${code}`;
const positionKey = (code: string) => `mbl.movieNight.${code}.position`;

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {}
}

/** Movie night for guests: join with a name, then swipe like everyone else. No account needed. */
export default function MovieNightGuest() {
  const params = useParams();
  const code = () => params.code ?? "";
  const [token, setToken] = createSignal(read(tokenKey(code())));
  const [name, setName] = createSignal("");
  const [view, setView] = createSignal<View | null>(null);
  const [deck, setDeck] = createSignal<Title[]>([]);
  const [position, setPosition] = createSignal(Number(read(positionKey(code())) ?? 0) || 0);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const headers = () => ({ "x-mbl-guest": token() ?? "" });

  const forget = (message: string) => {
    write(tokenKey(code()), null);
    write(positionKey(code()), null);
    setToken(null);
    setView(null);
    setError(message);
  };

  const load = async () => {
    if (!token()) return;
    try {
      const result = await client.GET("/api/v1/matchmaker/guest", {
        params: { header: headers() },
      });
      if (result.response.status === 401) {
        forget("This movie night has ended, or the host turned off the guest link.");
        return;
      }
      if (!result.data) throw new Error();
      const first = !view();
      setView(result.data);
      if (first) setDeck(seededShuffle(result.data.titles, result.data.seed));
    } catch {
      setError("Could not reach the server.");
    }
  };

  onMount(() => void load());
  const timer = setInterval(() => {
    if (token() && view()?.status !== "matched" && view()?.status !== "closed") void load();
  }, 3000);
  onCleanup(() => clearInterval(timer));

  const join = async (event: SubmitEvent) => {
    event.preventDefault();
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/matchmaker/guest/join/{code}", {
        params: { path: { code: code() } },
        body: { name: name().trim() },
      });
      if (result.response.status === 404) {
        setError("This movie night has ended, or the host turned off the guest link.");
        return;
      }
      if (result.response.status === 409) {
        setError("This movie night is full.");
        return;
      }
      if (!result.data) {
        setError("Enter a name of up to 40 characters.");
        return;
      }
      write(tokenKey(code()), result.data.token);
      setToken(result.data.token);
      await load();
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const current = () => deck()[position()];

  const vote = async (like: boolean) => {
    const title = current();
    if (!title || busy()) return;
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/matchmaker/guest/vote", {
        params: { header: headers() },
        body: { itemId: title.id, vote: like ? 1 : 0 },
      });
      if (result.response.status === 401) {
        forget("This movie night has ended, or the host turned off the guest link.");
        return;
      }
      if (!result.data) {
        setError("Your vote was not saved. Try again.");
        return;
      }
      setPosition(position() + 1);
      write(positionKey(code()), String(position()));
      if (result.data.matched) await load();
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const poster = (id: number) =>
    `/api/v1/matchmaker/guest/images/${id}?w=300&token=${encodeURIComponent(token() ?? "")}`;

  return (
    <main class="mbl-page" style={{ "max-width": "640px", margin: "0 auto", padding: "16px" }}>
      <h1>Movie night</h1>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show
        when={token()}
        fallback={
          <form class="mbl-card-section" onSubmit={join}>
            <p>
              You are invited to pick a movie together. Everyone swipes through the same titles;
              when all of you like the same one, it is the pick for tonight.
            </p>
            <label>
              Your name{" "}
              <input
                value={name()}
                maxLength={40}
                required
                onInput={(e) => setName(e.currentTarget.value)}
              />
            </label>{" "}
            <Button type="submit" disabled={busy() || !name().trim()}>
              Join
            </Button>
          </form>
        }
      >
        <Show when={view()?.matched}>
          {(title) => (
            <section class="mbl-card-section" aria-live="polite">
              <h2>It's a match!</h2>
              <p>{`Everyone liked ${title().title}.`}</p>
            </section>
          )}
        </Show>
        <Show when={view()?.status === "closed"}>
          <p class="mbl-card-section">Voting has closed and no title got a like.</p>
        </Show>
        <Show when={view() && view()?.status === "active"}>
          <Show when={view()?.startsAt}>
            {(at) => <p>{`Voting closes ${formatPlannedTime(at())}.`}</p>}
          </Show>
          <p>{`${view()?.memberCount ?? 0} people have joined.`}</p>
          <Show
            when={current()}
            fallback={
              <p class="mbl-card-section">
                You have gone through every title. Wait for the others to finish.
              </p>
            }
          >
            {(title) => (
              <section
                class="mbl-card-section"
                style={{ "text-align": "center" }}
                aria-label={title().title}
              >
                <Show when={title().posterId}>
                  {(id) => (
                    <img
                      src={poster(id())}
                      alt=""
                      style={{ "max-width": "240px", width: "100%", "border-radius": "12px" }}
                    />
                  )}
                </Show>
                <h2>
                  {title().title}
                  <Show when={title().year}> ({title().year})</Show>
                </h2>
                <p>
                  {position() + 1} / {deck().length}
                </p>
                <div style={{ display: "flex", gap: "1rem", "justify-content": "center" }}>
                  <Button variant="secondary" disabled={busy()} onClick={() => vote(false)}>
                    Pass
                  </Button>
                  <Button disabled={busy()} onClick={() => vote(true)}>
                    Like
                  </Button>
                </div>
              </section>
            )}
          </Show>
        </Show>
      </Show>
    </main>
  );
}
