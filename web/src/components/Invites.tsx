import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";
export default function Invites() {
  const [invites, setInvites] = createSignal<components["schemas"]["InviteRow"][]>([]);
  const [libs, setLibs] = createSignal<components["schemas"]["LibraryDto"][]>([]);
  const [ids, setIds] = createSignal<number[]>([]);
  const [rating, setRating] = createSignal("");
  const [hours, setHours] = createSignal(72);
  const [busy, setBusy] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  const [error, setError] = createSignal("");
  const load = async () => {
    setReady(false);
    setError("");
    try {
      const [a, b] = await Promise.all([
        client.GET("/api/v1/invites"),
        client.GET("/api/v1/libraries"),
      ]);
      if (a.error || b.error || !a.data || !b.data) throw new Error("Could not load invitations.");
      setInvites(a.data);
      setLibs(b.data);
      setReady(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Load failed.");
    }
  };
  onMount(() => void load());
  const create = async (e: Event) => {
    e.preventDefault();
    if (busy() || !ready()) return;
    setError("");
    if (!ids().length) {
      setError("Select at least one library for this invitation.");
      return;
    }
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/invites", {
        body: { libraryIds: ids(), contentRatingCeiling: rating() || undefined, ttlHours: hours() },
      });
      if (result.error || !result.data) throw new Error("Invitation could not be created.");
      setInvites((rows) => [result.data, ...rows]);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Creation failed.");
    } finally {
      setBusy(false);
    }
  };
  const revoke = async (id: number) => {
    if (busy()) return;
    setBusy(true);
    setError("");
    try {
      const result = await client.DELETE("/api/v1/invites/{id}", { params: { path: { id } } });
      if (result.error) throw new Error("Invitation could not be revoked.");
      setInvites((rows) => rows.filter((row) => row.id !== id));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Revoke failed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <section>
      <h2>Invitations</h2>
      <p>
        Each link creates one ordinary account with the selected library access. Treat links as
        secrets.
      </p>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
      <Button variant="secondary" disabled={busy()} onClick={load}>
        Reload invitations
      </Button>
      <Show when={ready()}>
        <form onSubmit={create}>
          <fieldset disabled={busy()}>
            <legend>Granted libraries</legend>
            <For each={libs()}>
              {(library) => (
                <label style={{ display: "block" }}>
                  <input
                    type="checkbox"
                    checked={ids().includes(library.id)}
                    onChange={(e) =>
                      setIds((values) =>
                        e.currentTarget.checked
                          ? [...values, library.id]
                          : values.filter((id) => id !== library.id),
                      )
                    }
                  />
                  {library.name}
                </label>
              )}
            </For>
          </fieldset>
          <label for="invite-rating">Maximum rating</label>
          <select
            id="invite-rating"
            disabled={busy()}
            value={rating()}
            onChange={(e) => setRating(e.currentTarget.value)}
          >
            <option value="">No age limit</option>
            <For each={["G", "PG", "PG-13", "R", "NC-17"]}>
              {(value) => <option>{value}</option>}
            </For>
          </select>
          <label for="invite-hours">Valid for hours</label>
          <input
            id="invite-hours"
            type="number"
            min={1}
            max={720}
            required
            disabled={busy()}
            value={hours()}
            onInput={(e) => setHours(Number(e.currentTarget.value))}
          />
          <Button type="submit" disabled={busy()}>
            Create invitation
          </Button>
        </form>
        <ul>
          <For each={invites()}>
            {(invite) => (
              <li>
                <p>
                  Expires {new Date(invite.expiresAt).toLocaleString()} ·{" "}
                  {invite.usedAt
                    ? "Used"
                    : invite.expiresAt <= Date.now()
                      ? "Expired"
                      : "Available"}
                </p>
                <Show when={!invite.usedAt && invite.expiresAt > Date.now()}>
                  <label>
                    Invitation link
                    <input
                      readonly
                      value={`${window.location.origin}/invite/${encodeURIComponent(invite.code)}`}
                      onFocus={(e) => e.currentTarget.select()}
                    />
                  </label>
                </Show>
                <Button variant="danger" disabled={busy()} onClick={() => revoke(invite.id)}>
                  Revoke invitation
                </Button>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </section>
  );
}
