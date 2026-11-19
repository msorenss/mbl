import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import ArrSettings from "../components/ArrSettings";
import StorageCheck from "../components/StorageCheck";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type MediaRequest = components["schemas"]["MediaRequestRow"];
type TmdbSearchHit = components["schemas"]["TmdbSearchHit"];
type MusicSearchHit = components["schemas"]["MusicSearchHit"];

const KIND_LABEL: Record<string, string> = {
  movie: "Movie",
  show: "TV Show",
  artist: "Artist",
  album: "Album",
};

const SERVICE: Record<string, string> = {
  movie: "Radarr",
  show: "Sonarr",
  artist: "Lidarr",
  album: "Lidarr",
};

const isMusicKind = (kind: string) => kind === "artist" || kind === "album";

const STATUS_LABEL: Record<string, string> = {
  pending: "Pending",
  approved: "Approved",
  declined: "Declined",
  available: "Available",
};

export default function Requests() {
  const { user } = useAuth();
  const [requests, setRequests] = createSignal<MediaRequest[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [title, setTitle] = createSignal("");
  const [kind, setKind] = createSignal("movie");
  const [tmdb, setTmdb] = createSignal("");
  const [year, setYear] = createSignal("");
  const [searchResults, setSearchResults] = createSignal<TmdbSearchHit[]>([]);
  const [musicResults, setMusicResults] = createSignal<MusicSearchHit[]>([]);
  const [mbid, setMbid] = createSignal("");
  const [artist, setArtist] = createSignal("");
  const isMusic = () => isMusicKind(kind());
  const [searching, setSearching] = createSignal(false);
  const [selectedPoster, setSelectedPoster] = createSignal<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/requests");
      if (result.error || !result.data) throw new Error("Could not load requests. Retry below.");
      setRequests(result.data);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not load requests.");
    } finally {
      setLoading(false);
    }
  };

  onMount(() => void load());

  const searchTmdb = async () => {
    const q = title().trim();
    if (!q) {
      setError("Please enter a title to search on TMDB.");
      return;
    }
    setError("");
    setSearching(true);
    try {
      const res = await client.GET("/api/v1/system/tmdb/search", {
        params: { query: { query: q, kind: kind() } },
      });
      if (res.data?.results) {
        setSearchResults(res.data.results);
        if (res.data.results.length === 0) {
          setError(
            res.data.configured
              ? "No matches found on TMDB for this query."
              : "TMDB API key is not configured in Server Settings.",
          );
        }
      }
    } catch {
      setError("Failed to query TMDB API.");
    } finally {
      setSearching(false);
    }
  };

  const findMusic = async (query: string) => {
    const res = await client.GET("/api/v1/requests/music/search", {
      params: { query: { query, kind: kind() } },
    });
    if (!res.data) {
      const message = (res.error as { error?: { message?: string } } | undefined)?.error?.message;
      throw new Error(message || "Music search failed.");
    }
    return res.data;
  };

  const searchMusic = async () => {
    const q = title().trim();
    if (!q) {
      setError("Enter a name to search for.");
      return;
    }
    setError("");
    setSearching(true);
    try {
      const hits = await findMusic(q);
      setMusicResults(hits);
      if (hits.length === 0) setError("No matches found.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Music search failed.");
    } finally {
      setSearching(false);
    }
  };

  const selectMusic = (hit: MusicSearchHit) => {
    setTitle(hit.title);
    setMbid(hit.musicbrainzId);
    setArtist(hit.artist ?? "");
    setYear(hit.year ? String(hit.year) : "");
    setSelectedPoster(hit.imageUrl || null);
    setMusicResults([]);
    setError("");
  };

  const submitMusic = async () => {
    let id = mbid();
    // Without a picked result, use the best search match.
    if (!id && title().trim()) {
      setBusy(true);
      try {
        const hit = (await findMusic(title().trim()))[0];
        if (hit) {
          selectMusic(hit);
          id = hit.musicbrainzId;
        }
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : "Music search failed.");
        return;
      } finally {
        setBusy(false);
      }
    }
    if (!title().trim() || !id) {
      setError("Search for the artist or album and pick a match.");
      return;
    }
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/requests", {
        body: {
          title: title().trim(),
          kind: kind(),
          musicbrainzId: id,
          artist: artist() || undefined,
          year: year().trim() ? Number(year()) : undefined,
        },
      });
      if (result.error || !result.data)
        throw new Error("Request could not be submitted. Your entries have been kept.");
      setRequests((previous) => [result.data, ...previous]);
      setTitle("");
      setMbid("");
      setArtist("");
      setYear("");
      setSelectedPoster(null);
      setNotice("Request submitted for review.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Submission failed.");
    } finally {
      setBusy(false);
    }
  };

  const selectHit = (hit: TmdbSearchHit) => {
    setTitle(hit.title);
    setTmdb(String(hit.id));
    setKind(hit.kind === "show" ? "show" : "movie");
    setYear(hit.year ? String(hit.year) : "");
    setSelectedPoster(hit.posterUrl || null);
    setSearchResults([]);
    setError("");
  };

  const submit = async (event: Event) => {
    event.preventDefault();
    if (busy()) return;
    setError("");
    setNotice("");
    if (isMusic()) {
      await submitMusic();
      return;
    }

    let resolvedTmdb = tmdb().trim();
    let resolvedYear = year().trim();

    // If TMDB ID was omitted, try searching for the title automatically
    if (!resolvedTmdb && title().trim()) {
      setBusy(true);
      try {
        const res = await client.GET("/api/v1/system/tmdb/search", {
          params: { query: { query: title().trim(), kind: kind() } },
        });
        const hit = res.data?.results?.[0];
        if (hit) {
          resolvedTmdb = String(hit.id);
          if (!resolvedYear && hit.year) resolvedYear = String(hit.year);
          setTmdb(resolvedTmdb);
          if (resolvedYear) setYear(resolvedYear);
          if (hit.posterUrl) setSelectedPoster(hit.posterUrl);
        }
      } catch {
        // Fall through to validation
      } finally {
        setBusy(false);
      }
    }

    if (
      !title().trim() ||
      !/^\d+$/.test(resolvedTmdb) ||
      !Number.isSafeInteger(Number(resolvedTmdb)) ||
      Number(resolvedTmdb) < 1 ||
      (resolvedYear && (!/^\d{4}$/.test(resolvedYear) || Number(resolvedYear) < 1800))
    ) {
      setError(
        "Enter a title, positive TMDB ID (or click Search TMDB) and optional four-digit year.",
      );
      return;
    }

    setBusy(true);
    try {
      const result = await client.POST("/api/v1/requests", {
        body: {
          title: title().trim(),
          kind: kind(),
          tmdbId: Number(resolvedTmdb),
          year: resolvedYear ? Number(resolvedYear) : undefined,
        },
      });
      if (result.error || !result.data)
        throw new Error("Request could not be submitted. Your entries have been kept.");
      setRequests((previous) => [result.data, ...previous]);
      setTitle("");
      setTmdb("");
      setYear("");
      setSelectedPoster(null);
      setSearchResults([]);
      setNotice("Request submitted for review.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Submission failed.");
    } finally {
      setBusy(false);
    }
  };

  const update = async (request: MediaRequest, status: string) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PATCH("/api/v1/requests/{id}/status", {
        params: { path: { id: request.id } },
        body: { status },
      });
      if (result.error) {
        const message = (result.error as { error?: { message?: string } }).error?.message;
        throw new Error(
          message?.replace(/^(external service|validation|unavailable|conflict): /, "") ||
            "Status could not be saved.",
        );
      }
      await load();
      const saved = requests().find((row) => row.id === request.id);
      const service = SERVICE[request.kind] ?? "Radarr";
      setNotice(
        status !== "approved"
          ? "Status saved."
          : saved?.arrId
            ? `"${request.title}" was sent to ${service}.`
            : `Approved. Nothing was sent to ${service} because it is turned off.`,
      );
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Status update failed.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div style={{ "max-width": "900px", margin: "0 auto", padding: "16px 0" }}>
      <h1 style={{ "margin-bottom": "6px" }}>Media Requests</h1>
      <p style={{ color: "var(--text-secondary)", "margin-bottom": "20px" }}>
        Request a movie or TV show. Search TMDB directly by title or enter a known TMDB ID. When
        Radarr, Sonarr or Lidarr is connected, approving a request starts the download. Music is
        searched through Lidarr.
      </p>

      <form
        onSubmit={submit}
        style={{
          display: "grid",
          gap: "14px",
          background: "var(--bg-surface)",
          padding: "20px",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-md)",
          "margin-bottom": "24px",
        }}
      >
        <div>
          <label
            for="request-title"
            style={{
              display: "block",
              "font-weight": "600",
              "font-size": "13px",
              "margin-bottom": "6px",
            }}
          >
            Title
          </label>
          <div style={{ display: "flex", gap: "8px" }}>
            <input
              id="request-title"
              required
              maxlength={300}
              disabled={busy() || searching()}
              value={title()}
              onInput={(event) => setTitle(event.currentTarget.value)}
              placeholder={isMusic() ? "e.g. ABBA or Arrival" : "e.g. Inception or Severance"}
              style={{
                flex: 1,
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            />
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={busy() || searching() || !title().trim()}
              onClick={() => (isMusic() ? searchMusic() : searchTmdb())}
            >
              {searching() ? "Searching…" : isMusic() ? "🔍 Search music" : "🔍 Search TMDB"}
            </Button>
          </div>
        </div>

        {/* TMDB Search Results Dropdown/Grid */}
        <Show when={searchResults().length > 0}>
          <div
            style={{
              background: "var(--bg-base)",
              border: "1px solid var(--accent)",
              "border-radius": "var(--radius-md)",
              padding: "12px",
              "max-height": "280px",
              overflow: "auto",
            }}
          >
            <div
              style={{
                "font-size": "12px",
                "font-weight": "700",
                "margin-bottom": "8px",
                color: "var(--accent)",
              }}
            >
              Select a matching title from TMDB:
            </div>
            <div style={{ display: "grid", gap: "8px" }}>
              <For each={searchResults()}>
                {(hit) => (
                  <button
                    type="button"
                    onClick={() => selectHit(hit)}
                    style={{
                      display: "flex",
                      gap: "10px",
                      "align-items": "center",
                      padding: "8px",
                      background: "var(--bg-surface)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-sm)",
                      cursor: "pointer",
                      "text-align": "left",
                    }}
                  >
                    <Show when={hit.posterUrl}>
                      <img
                        src={hit.posterUrl!}
                        alt={hit.title}
                        style={{
                          width: "32px",
                          height: "48px",
                          "object-fit": "cover",
                          "border-radius": "2px",
                        }}
                      />
                    </Show>
                    <div style={{ flex: 1 }}>
                      <div style={{ "font-weight": "600", "font-size": "13px" }}>
                        {hit.title} {hit.year ? `(${hit.year})` : ""}
                      </div>
                      <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                        {hit.kind === "show" ? "TV Show" : "Movie"} · TMDB ID: {hit.id}
                      </div>
                    </div>
                  </button>
                )}
              </For>
            </div>
          </div>
        </Show>

        <Show when={musicResults().length > 0}>
          <div
            style={{
              background: "var(--bg-base)",
              border: "1px solid var(--accent)",
              "border-radius": "var(--radius-md)",
              padding: "12px",
              "max-height": "280px",
              overflow: "auto",
              display: "grid",
              gap: "8px",
            }}
          >
            <For each={musicResults()}>
              {(hit) => (
                <button
                  type="button"
                  onClick={() => selectMusic(hit)}
                  style={{
                    display: "flex",
                    gap: "10px",
                    "align-items": "center",
                    padding: "8px",
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    cursor: "pointer",
                    "text-align": "left",
                  }}
                >
                  <Show when={hit.imageUrl}>
                    <img
                      src={hit.imageUrl!}
                      alt=""
                      style={{ width: "40px", height: "40px", "object-fit": "cover" }}
                    />
                  </Show>
                  <div style={{ flex: 1 }}>
                    <div style={{ "font-weight": "600", "font-size": "13px" }}>
                      {hit.title} {hit.year ? `(${hit.year})` : ""}
                    </div>
                    <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                      {hit.artist ?? KIND_LABEL[hit.kind]}
                      <Show when={hit.inLidarr}> · In Lidarr</Show>
                    </div>
                  </div>
                </button>
              )}
            </For>
          </div>
        </Show>

        <div
          style={{
            display: "grid",
            "grid-template-columns": "repeat(auto-fit, minmax(160px, 1fr))",
            gap: "12px",
          }}
        >
          <div>
            <label
              for="request-kind"
              style={{
                display: "block",
                "font-weight": "600",
                "font-size": "13px",
                "margin-bottom": "6px",
              }}
            >
              Media type
            </label>
            <select
              id="request-kind"
              disabled={busy()}
              value={kind()}
              onChange={(event) => {
                setKind(event.currentTarget.value);
                setMbid("");
                setArtist("");
                setSearchResults([]);
                setMusicResults([]);
              }}
              style={{
                width: "100%",
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            >
              <option value="movie">Movie</option>
              <option value="show">TV show</option>
              <option value="artist">Artist</option>
              <option value="album">Album</option>
            </select>
          </div>

          <Show
            when={!isMusic()}
            fallback={
              <div style={{ "font-size": "12px", color: "var(--text-muted)", "align-self": "end" }}>
                <Show when={mbid()} fallback={"Search to pick the artist or album."}>
                  <Show when={artist()}>
                    <div>{artist()}</div>
                  </Show>
                  <div>{`MusicBrainz ID: ${mbid()}`}</div>
                </Show>
              </div>
            }
          >
            <div>
              <label
                for="request-tmdb"
                style={{
                  display: "block",
                  "font-weight": "600",
                  "font-size": "13px",
                  "margin-bottom": "6px",
                }}
              >
                TMDB ID (optional if searching)
              </label>
              <input
                id="request-tmdb"
                inputmode="numeric"
                disabled={busy()}
                value={tmdb()}
                onInput={(event) => setTmdb(event.currentTarget.value)}
                placeholder="e.g. 157336"
                style={{
                  width: "100%",
                  padding: "8px 12px",
                  background: "var(--bg-base)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-sm)",
                  color: "var(--text-primary)",
                }}
              />
            </div>
          </Show>

          <div>
            <label
              for="request-year"
              style={{
                display: "block",
                "font-weight": "600",
                "font-size": "13px",
                "margin-bottom": "6px",
              }}
            >
              Year (optional)
            </label>
            <input
              id="request-year"
              inputmode="numeric"
              disabled={busy()}
              value={year()}
              onInput={(event) => setYear(event.currentTarget.value)}
              placeholder="e.g. 2024"
              style={{
                width: "100%",
                padding: "8px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
              }}
            />
          </div>
        </div>

        <Show when={selectedPoster()}>
          <div style={{ display: "flex", "align-items": "center", gap: "10px" }}>
            <img
              src={selectedPoster()!}
              alt="Selected Poster"
              style={{
                width: "40px",
                height: "60px",
                "object-fit": "cover",
                "border-radius": "4px",
              }}
            />
            <span style={{ "font-size": "12px", color: "var(--text-secondary)" }}>
              Selected from TMDB metadata
            </span>
          </div>
        </Show>

        <div style={{ "margin-top": "6px" }}>
          <Button type="submit" variant="primary" disabled={busy()}>
            {busy() ? "Submitting…" : "Submit Request"}
          </Button>
        </div>
      </form>

      <Show when={user()?.isAdmin}>
        <ArrSettings />
        <StorageCheck />
      </Show>

      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)", "margin-bottom": "14px" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ color: "var(--success)", "margin-bottom": "14px" }}>
          {notice()}
        </p>
      </Show>

      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "center",
          "margin-bottom": "14px",
        }}
      >
        <h2 style={{ margin: 0, "font-size": "18px" }}>
          {user()?.isAdmin ? "All Requests" : "Your Requests"}
        </h2>
        <Button variant="secondary" size="sm" disabled={loading() || busy()} onClick={load}>
          Refresh
        </Button>
      </div>

      <Show when={!loading()} fallback={<p role="status">Loading requests…</p>}>
        <Show when={requests().length} fallback={<p>No requests found.</p>}>
          <ul style={{ "list-style": "none", padding: 0, display: "grid", gap: "10px" }}>
            <For each={requests()}>
              {(request) => (
                <li
                  style={{
                    padding: "14px 18px",
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-md)",
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                  }}
                >
                  <div>
                    <div style={{ "font-weight": "700", "font-size": "15px" }}>
                      {request.title} {request.year ? `(${request.year})` : ""}
                    </div>
                    <div
                      style={{
                        "font-size": "12px",
                        color: "var(--text-muted)",
                        "margin-top": "2px",
                      }}
                    >
                      {KIND_LABEL[request.kind] ?? request.kind}
                      {isMusicKind(request.kind)
                        ? request.artist
                          ? ` · ${request.artist}`
                          : ""
                        : ` · TMDB ID: ${request.tmdbId}`}{" "}
                      · Status:{" "}
                      <span
                        style={{
                          "font-weight": "600",
                          color:
                            request.status === "available"
                              ? "var(--success)"
                              : request.status === "approved"
                                ? "var(--accent)"
                                : "var(--text-secondary)",
                        }}
                      >
                        {STATUS_LABEL[request.status] ?? request.status}
                      </span>
                      <Show when={request.arrId}>
                        {" · "}
                        {`Sent to ${SERVICE[request.kind] ?? "Radarr"}`}
                      </Show>
                      <Show when={request.status === "approved" && !request.arrId}>
                        {" · "}
                        {`Not sent to ${SERVICE[request.kind] ?? "Radarr"}`}
                      </Show>
                    </div>
                  </div>
                  <Show when={user()?.isAdmin}>
                    <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
                      <Show when={request.status === "approved" && !request.arrId}>
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={busy()}
                          onClick={() => update(request, "approved")}
                        >
                          {`Send to ${SERVICE[request.kind] ?? "Radarr"}`}
                        </Button>
                      </Show>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy() || request.status === "approved"}
                        onClick={() => update(request, "approved")}
                      >
                        Approve
                      </Button>
                      <Button
                        size="sm"
                        variant="secondary"
                        disabled={busy() || request.status !== "pending"}
                        onClick={() => update(request, "declined")}
                      >
                        Decline
                      </Button>
                      <Button
                        size="sm"
                        variant="primary"
                        disabled={busy() || request.status === "available"}
                        onClick={() => update(request, "available")}
                      >
                        Mark available
                      </Button>
                    </div>
                  </Show>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </div>
  );
}
