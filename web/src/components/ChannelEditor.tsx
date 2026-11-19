import { createSignal, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button, Dialog } from "./ui";

type Channel = components["schemas"]["Channel"];

/** Admin dialog to rename, renumber, link to the guide, hide, favourite or delete a channel. */
export default function ChannelEditor(props: { channel: Channel; onChanged: () => void }) {
  const [open, setOpen] = createSignal(false);
  const [name, setName] = createSignal("");
  const [number, setNumber] = createSignal("");
  const [epg, setEpg] = createSignal("");
  const [favorite, setFavorite] = createSignal(false);
  const [hidden, setHidden] = createSignal(false);
  const [confirmDelete, setConfirmDelete] = createSignal(false);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const start = () => {
    setName(props.channel.name);
    setNumber(props.channel.channelNumber);
    setEpg(props.channel.epgChannelId ?? "");
    setFavorite(props.channel.isFavorite);
    setHidden(props.channel.isHidden);
    setConfirmDelete(false);
    setError("");
    setOpen(true);
  };

  const save = async (event: Event) => {
    event.preventDefault();
    if (!name().trim() || !number().trim()) {
      setError("Name and number are required.");
      return;
    }
    setBusy(true);
    setError("");
    try {
      const result = await client.PATCH("/api/v1/livetv/channels/{id}", {
        params: { path: { id: props.channel.id } },
        body: {
          name: name().trim(),
          channelNumber: number().trim(),
          epgChannelId: epg().trim(),
          isFavorite: favorite(),
          isHidden: hidden(),
        },
      });
      if (!result.data) {
        setError("The channel was not saved.");
        return;
      }
      setOpen(false);
      props.onChanged();
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    setBusy(true);
    setError("");
    try {
      const result = await client.DELETE("/api/v1/livetv/channels/{id}", {
        params: { path: { id: props.channel.id } },
      });
      if (!result.data?.success) {
        setError("The channel was not deleted.");
        return;
      }
      setOpen(false);
      props.onChanged();
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button variant="ghost" onClick={start}>
        Edit
      </Button>
      <Dialog open={open()} onOpenChange={setOpen} title="Edit channel">
        <form
          onSubmit={save}
          style={{ display: "grid", gap: "12px", "min-width": "min(420px, 86vw)" }}
        >
          <label>
            Name
            <input value={name()} onInput={(e) => setName(e.currentTarget.value)} />
          </label>
          <label>
            Number
            <input value={number()} onInput={(e) => setNumber(e.currentTarget.value)} />
          </label>
          <label>
            Guide channel ID
            <input value={epg()} onInput={(e) => setEpg(e.currentTarget.value)} />
          </label>
          <label>
            <input
              type="checkbox"
              checked={favorite()}
              onChange={(e) => setFavorite(e.currentTarget.checked)}
            />{" "}
            Favourite
          </label>
          <label>
            <input
              type="checkbox"
              checked={hidden()}
              onChange={(e) => setHidden(e.currentTarget.checked)}
            />{" "}
            Hidden from the guide
          </label>
          <Show when={error()}>
            <p role="alert">{error()}</p>
          </Show>
          <div style={{ display: "flex", gap: "8px", "justify-content": "space-between" }}>
            <Show
              when={confirmDelete()}
              fallback={
                <Button type="button" variant="danger" onClick={() => setConfirmDelete(true)}>
                  Delete
                </Button>
              }
            >
              <Button type="button" variant="danger" disabled={busy()} onClick={remove}>
                Delete channel for good
              </Button>
            </Show>
            <Button type="submit" disabled={busy()}>
              Save
            </Button>
          </div>
        </form>
      </Dialog>
    </>
  );
}
