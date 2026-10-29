import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";
import { Dialog } from "./ui/Dialog";

export default function UserPermissions(props: { user: components["schemas"]["UserDto"] }) {
  const [open, setOpen] = createSignal(false);
  const [ready, setReady] = createSignal(false);
  const [loading, setLoading] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [managed, setManaged] = createSignal(false);
  const [rating, setRating] = createSignal("");
  const [libraries, setLibraries] = createSignal<components["schemas"]["LibraryDto"][]>([]);
  const [selected, setSelected] = createSignal<number[]>([]);
  const [hasPin, setHasPin] = createSignal(false);
  const [pin, setPin] = createSignal("");
  let generation = 0;
  const load = async () => {
    const current = ++generation;
    setReady(false);
    setLoading(true);
    setError("");
    setNotice("");
    setPin("");
    try {
      const [security, libs] = await Promise.all([
        client.GET("/api/v1/users/{id}/permissions", { params: { path: { id: props.user.id } } }),
        client.GET("/api/v1/libraries"),
      ]);
      if (current !== generation) return;
      if (security.error || libs.error || !security.data || !libs.data)
        throw new Error("Could not load permissions. Retry before editing.");
      setManaged(security.data.isManaged);
      setRating(security.data.contentRatingCeiling ?? "");
      setSelected(security.data.libraryIds);
      setHasPin(security.data.hasPin);
      setLibraries(libs.data);
      setReady(true);
    } catch (reason) {
      if (current === generation)
        setError(reason instanceof Error ? reason.message : "Load failed.");
    } finally {
      if (current === generation) setLoading(false);
    }
  };
  const changeOpen = (value: boolean) => {
    if (busy()) return;
    setOpen(value);
    if (value) void load();
    else generation++;
  };
  const save = async (event: Event) => {
    event.preventDefault();
    if (busy() || loading() || !ready()) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PUT("/api/v1/users/{id}/permissions", {
        params: { path: { id: props.user.id } },
        body: {
          isManaged: managed(),
          contentRatingCeiling: rating() || null,
          libraryIds: selected(),
        },
      });
      if (result.error || !result.data)
        throw new Error("Permissions could not be saved. Your choices have been kept.");
      setManaged(result.data.isManaged);
      setRating(result.data.contentRatingCeiling ?? "");
      setSelected(result.data.libraryIds);
      setNotice("Permissions saved.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Save failed.");
    } finally {
      setBusy(false);
    }
  };
  const savePin = async (remove = false) => {
    if (busy()) return;
    setError("");
    setNotice("");
    if (!remove && !/^\d{4,8}$/.test(pin())) {
      setError("PIN must contain 4–8 digits.");
      return;
    }
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/users/{id}/pin", {
        params: { path: { id: props.user.id } },
        body: { pin: remove ? null : pin() },
      });
      if (result.error) throw new Error("PIN could not be saved.");
      setHasPin(!remove);
      setPin("");
      setNotice(remove ? "PIN removed." : "PIN saved.");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "PIN update failed.");
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <Button size="sm" variant="secondary" onClick={() => changeOpen(true)}>
        Permissions
      </Button>
      <Dialog
        open={open()}
        onOpenChange={changeOpen}
        title={`Permissions: ${props.user.displayName || props.user.username}`}
      >
        <Show when={error()}>
          <p role="alert">{error()}</p>
        </Show>
        <Show when={notice()}>
          <p role="status">{notice()}</p>
        </Show>
        <Show
          when={!loading() && ready()}
          fallback={
            <div>
              <Show when={loading()}>
                <p role="status">Loading permissions…</p>
              </Show>
              <Button disabled={loading()} onClick={load}>
                Retry loading permissions
              </Button>
            </div>
          }
        >
          <Button variant="ghost" disabled={busy()} onClick={load}>
            Reload permissions
          </Button>
          <form onSubmit={save} style={{ display: "grid", gap: "12px" }}>
            <Show when={props.user.isAdmin}>
              <p>Administrators bypass library and age restrictions.</p>
            </Show>
            <label>
              <input
                type="checkbox"
                checked={managed()}
                disabled={busy()}
                onChange={(event) => setManaged(event.currentTarget.checked)}
              />{" "}
              Managed profile
            </label>
            <label for={`rating-${props.user.id}`}>Maximum content rating</label>
            <select
              id={`rating-${props.user.id}`}
              value={rating()}
              disabled={busy()}
              onChange={(event) => setRating(event.currentTarget.value)}
            >
              <option value="">No age limit</option>
              <For
                each={Array.from(
                  new Set(["G", "PG", "PG-13", "R", "NC-17", ...(rating() ? [rating()] : [])]),
                )}
              >
                {(value) => <option value={value}>{value}</option>}
              </For>
            </select>
            <fieldset disabled={busy()}>
              <legend>Allowed libraries</legend>
              <p>
                With no libraries selected, a managed profile has no access. An ordinary account has
                access to all libraries.
              </p>
              <For each={libraries()}>
                {(library) => (
                  <label style={{ display: "block" }}>
                    <input
                      type="checkbox"
                      checked={selected().includes(library.id)}
                      onChange={(event) =>
                        setSelected((ids) =>
                          event.currentTarget.checked
                            ? [...ids, library.id]
                            : ids.filter((id) => id !== library.id),
                        )
                      }
                    />
                    {library.name}
                  </label>
                )}
              </For>
            </fieldset>
            <Button type="submit" disabled={busy()}>
              Save permissions
            </Button>
          </form>
          <hr />
          <h3>Profile PIN</h3>
          <p>
            {hasPin() ? "PIN is set." : "No PIN is set."} A PIN does not replace account sign-in.
          </p>
          <label for={`pin-${props.user.id}`}>New PIN</label>{" "}
          <input
            id={`pin-${props.user.id}`}
            type="password"
            inputmode="numeric"
            autocomplete="new-password"
            maxlength={8}
            value={pin()}
            disabled={busy()}
            onInput={(event) => setPin(event.currentTarget.value)}
          />
          <Button disabled={busy() || !pin()} onClick={() => savePin()}>
            Set PIN
          </Button>{" "}
          <Button variant="danger" disabled={busy() || !hasPin()} onClick={() => savePin(true)}>
            Remove PIN
          </Button>
        </Show>
      </Dialog>
    </>
  );
}
