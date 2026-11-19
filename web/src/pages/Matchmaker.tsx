import { A, useNavigate, useParams } from "@solidjs/router";
import { createEffect, createSignal, For, on, onCleanup, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type Session = components["schemas"]["MatchmakerSessionRow"];
type Item = components["schemas"]["ItemSummary"];
type Library = components["schemas"]["LibraryDto"];

/** Deterministic shuffle so everyone in a session sees the same order. */
export function seededShuffle<T>(list: T[], seed: string): T[] {
  let h = 2166136261;
  for (let i = 0; i < seed.length; i++) h = Math.imul(h ^ seed.charCodeAt(i), 16777619);
  const next = () => {
    h = Math.imul(h ^ (h >>> 15), 2246822507);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    h ^= h >>> 16;
    return (h >>> 0) / 4294967296;
  };
  const out = [...list];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(next() * (i + 1));
    [out[i], out[j]] = [out[j], out[i]];
  }
  return out;
}

function libraryOf(session: Session): number | null {
  try {
    const value = JSON.parse(session.filterConfig)?.libraryId;
    return typeof value === "number" ? value : null;
  } catch {
    return null;
  }
}

const progressKey = (id: string) => `mbl.matchmaker.${id}`;

function Lobby() {
  const navigate = useNavigate();
  const [libraries, setLibraries] = createSignal<Library[]>([]);
  const [library, setLibrary] = createSignal<number | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  void client
    .GET("/api/v1/libraries")
    .then(({ data }) => {
      const list = (data ?? []).filter((l) => l.kind === "movie" || l.kind === "show");
      setLibraries(list);
      setLibrary(list[0]?.id ?? null);
    })
    .catch(() => setError("Could not load libraries."));

  const create = async () => {
    const id = library();
    if (id === null) return;
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/matchmaker/sessions", {
        body: { filterConfig: JSON.stringify({ libraryId: id }) },
      });
      if (!result.data) {
        setError("Could not start a movie night.");
        return;
      }
      navigate(`/matchmaker/${result.data.id}`);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="mbl-card-section">
      <p>
        Everyone swipes through the same titles on their own device. When all of you like the same
        title, it is your pick for tonight. Each person signs in with their own account.
      </p>
      <Show
        when={libraries().length > 0}
        fallback={<p>There is no movie or TV library to choose from.</p>}
      >
        <label>
          Library{" "}
          <select
            value={library() ?? ""}
            onChange={(e) => setLibrary(Number(e.currentTarget.value))}
          >
            <For each={libraries()}>{(l) => <option value={l.id}>{l.name}</option>}</For>
          </select>
        </label>{" "}
        <Button onClick={create} disabled={busy() || library() === null}>
          Start movie night
        </Button>
      </Show>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
    </section>
  );
}

function Swipe(props: { id: string }) {
  const { user } = useAuth();
  const [session, setSession] = createSignal<Session | null>(null);
  const [deck, setDeck] = createSignal<Item[]>([]);
  const [position, setPosition] = createSignal(0);
  const [matched, setMatched] = createSignal<Item | null>(null);
  const [error, setError] = createSignal("");
  const [copied, setCopied] = createSignal(false);
  const [voting, setVoting] = createSignal(false);

  const readPosition = () => {
    try {
      return Number(localStorage.getItem(progressKey(props.id)) ?? 0) || 0;
    } catch {
      return 0;
    }
  };
  const savePosition = (value: number) => {
    setPosition(value);
    try {
      localStorage.setItem(progressKey(props.id), String(value));
    } catch {}
  };

  const showMatch = async (itemId: number) => {
    const found = deck().find((i) => i.id === itemId);
    if (found) {
      setMatched(found);
      return;
    }
    const result = await client.GET("/api/v1/items/{id}", { params: { path: { id: itemId } } });
    if (result.data) setMatched(result.data as unknown as Item);
  };

  const refresh = async () => {
    const result = await client.GET("/api/v1/matchmaker/sessions/{id}", {
      params: { path: { id: props.id } },
    });
    if (!result.data) {
      setError("This movie night was not found or has expired.");
      return null;
    }
    setSession(result.data);
    if (result.data.matchedItemId && !matched()) await showMatch(result.data.matchedItemId);
    return result.data;
  };

  createEffect(
    on(
      () => props.id,
      async (id) => {
        setError("");
        setMatched(null);
        setDeck([]);
        try {
          // Joining a finished session fails; members can still read it.
          await client.POST("/api/v1/matchmaker/sessions/{id}/join", {
            params: { path: { id } },
          });
          const current = await refresh();
          if (!current) return;
          const libraryId = libraryOf(current);
          if (libraryId === null) {
            setError("This movie night has no library.");
            return;
          }
          const items = await client.GET("/api/v1/libraries/{id}/items", {
            params: { path: { id: libraryId }, query: { limit: 200 } },
          });
          setDeck(seededShuffle(items.data?.items ?? [], id));
          setPosition(readPosition());
        } catch {
          setError("Could not reach the server.");
        }
      },
    ),
  );

  const timer = setInterval(() => {
    if (!matched()) void refresh().catch(() => undefined);
  }, 3000);
  onCleanup(() => clearInterval(timer));

  const current = () => deck()[position()];

  const vote = async (like: boolean) => {
    const item = current();
    if (!item || voting()) return;
    setVoting(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/matchmaker/sessions/{id}/vote", {
        params: { path: { id: props.id } },
        body: { userName: user()?.username ?? "", itemId: item.id, vote: like ? 1 : 0 },
      });
      if (!result.data) {
        setError("Your vote was not saved. Try again.");
        return;
      }
      savePosition(position() + 1);
      if (result.data.matched && result.data.matchedItemId)
        await showMatch(result.data.matchedItemId);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setVoting(false);
    }
  };

  const link = () => `${window.location.origin}/matchmaker/${props.id}`;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(link());
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };

  return (
    <>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={matched()}>
        {(item) => (
          <section class="mbl-card-section" aria-live="polite">
            <h2>It's a match!</h2>
            <p>{`Everyone liked ${item().title}.`}</p>
            <A href={`/item/${item().id}`}>{`Open ${item().title}`}</A>
          </section>
        )}
      </Show>
      <Show when={!matched() && session()}>
        {(s) => (
          <>
            <section class="mbl-card-section">
              <p>
                {s().memberCount === 1
                  ? "Only you have joined so far. Send the link to the others."
                  : `${s().memberCount} people have joined.`}
              </p>
              <input readonly value={link()} aria-label="Invite link" style={{ width: "100%" }} />
              <Button variant="secondary" onClick={copy}>
                {copied() ? "Copied" : "Copy link"}
              </Button>
            </section>
            <Show
              when={current()}
              fallback={
                <p class="mbl-card-section">
                  You have gone through every title. Wait for the others, or start a new movie
                  night.
                </p>
              }
            >
              {(item) => (
                <section
                  class="mbl-card-section"
                  style={{ "text-align": "center" }}
                  aria-label={item().title}
                >
                  <Show when={item().posterId}>
                    <img
                      src={`/api/v1/images/${item().posterId}?w=300`}
                      alt=""
                      style={{ "max-width": "240px", width: "100%", "border-radius": "12px" }}
                    />
                  </Show>
                  <h2>
                    {item().title}
                    <Show when={item().year}> ({item().year})</Show>
                  </h2>
                  <p>
                    {position() + 1} / {deck().length}
                  </p>
                  <div style={{ display: "flex", gap: "1rem", "justify-content": "center" }}>
                    <Button variant="secondary" disabled={voting()} onClick={() => vote(false)}>
                      Pass
                    </Button>
                    <Button disabled={voting()} onClick={() => vote(true)}>
                      Like
                    </Button>
                  </div>
                </section>
              )}
            </Show>
          </>
        )}
      </Show>
    </>
  );
}

/** Movie night: swipe together until everyone likes the same title. */
export default function Matchmaker() {
  const params = useParams();
  return (
    <div class="mbl-page" style={{ "max-width": "640px" }}>
      <h1>Movie night</h1>
      <Show when={params.id} fallback={<Lobby />}>
        {(id) => <Swipe id={id()} />}
      </Show>
    </div>
  );
}
