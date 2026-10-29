import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type MediaRequest = components["schemas"]["MediaRequestRow"];
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
  const submit = async (event: Event) => {
    event.preventDefault();
    if (busy()) return;
    setError("");
    setNotice("");
    if (
      !title().trim() ||
      !/^\d+$/.test(tmdb()) ||
      !Number.isSafeInteger(Number(tmdb())) ||
      Number(tmdb()) < 1 ||
      (year() && (!/^\d{4}$/.test(year()) || Number(year()) < 1800))
    ) {
      setError("Enter a title, positive TMDB ID and optional four-digit year.");
      return;
    }
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/requests", {
        body: {
          title: title().trim(),
          kind: kind(),
          tmdbId: Number(tmdb()),
          year: year() ? Number(year()) : undefined,
        },
      });
      if (result.error || !result.data)
        throw new Error("Request could not be submitted. Your entries have been kept.");
      setRequests((previous) => [result.data, ...previous]);
      setTitle("");
      setTmdb("");
      setYear("");
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
      if (result.error) throw new Error("Status could not be saved.");
      setRequests((rows) => rows.map((row) => (row.id === request.id ? { ...row, status } : row)));
      setNotice("Status saved.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Status update failed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div style={{ "max-width": "900px", margin: "0 auto" }}>
      <h1>Media requests</h1>
      <p>
        Request a movie or show by its TMDB ID. Approval records the decision; automatic
        Radarr/Sonarr delivery is not connected yet.
      </p>
      <form onSubmit={submit} style={{ display: "grid", gap: "12px" }}>
        <label for="request-title">Title</label>
        <input
          id="request-title"
          required
          maxlength={300}
          disabled={busy()}
          value={title()}
          onInput={(event) => setTitle(event.currentTarget.value)}
        />
        <label for="request-kind">Media type</label>
        <select
          id="request-kind"
          disabled={busy()}
          value={kind()}
          onChange={(event) => setKind(event.currentTarget.value)}
        >
          <option value="movie">Movie</option>
          <option value="show">TV show</option>
        </select>
        <label for="request-tmdb">TMDB ID</label>
        <input
          id="request-tmdb"
          inputmode="numeric"
          required
          disabled={busy()}
          value={tmdb()}
          onInput={(event) => setTmdb(event.currentTarget.value)}
        />
        <label for="request-year">Year (optional)</label>
        <input
          id="request-year"
          inputmode="numeric"
          disabled={busy()}
          value={year()}
          onInput={(event) => setYear(event.currentTarget.value)}
        />
        <Button type="submit" disabled={busy()}>
          Submit request
        </Button>
      </form>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Show when={notice()}>
        <p role="status">{notice()}</p>
      </Show>
      <h2>{user()?.isAdmin ? "All requests" : "Your requests"}</h2>
      <Button variant="secondary" disabled={loading() || busy()} onClick={load}>
        Refresh
      </Button>
      <Show when={!loading()} fallback={<p role="status">Loading requests…</p>}>
        <Show when={requests().length} fallback={<p>No requests found.</p>}>
          <ul>
            <For each={requests()}>
              {(request) => (
                <li style={{ padding: "12px" }}>
                  <strong>{request.title}</strong> {request.year} · {request.kind} · TMDB{" "}
                  {request.tmdbId}
                  <p>Status: {request.status}</p>
                  <Show when={user()?.isAdmin}>
                    <Button
                      size="sm"
                      disabled={busy() || request.status === "approved"}
                      onClick={() => update(request, "approved")}
                    >
                      Approve
                    </Button>{" "}
                    <Button
                      size="sm"
                      disabled={busy() || request.status === "available"}
                      onClick={() => update(request, "available")}
                    >
                      Mark available
                    </Button>
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
