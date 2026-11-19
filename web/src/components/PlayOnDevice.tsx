import { createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { localPlayerId } from "../utils/localRemote";
import { Button } from "./ui";

type PlayerInfo = components["schemas"]["PlayerInfo"];

/** Starts this title on another of the user's devices that is running MBL. */
export default function PlayOnDevice(props: { itemId: number; startMs: number }) {
  const [players, setPlayers] = createSignal<PlayerInfo[] | null>(null);
  const [notice, setNotice] = createSignal("");

  const open = async () => {
    setNotice("");
    try {
      const result = await client.GET("/api/v1/players", {});
      setPlayers((result.data ?? []).filter((p) => p.id !== localPlayerId()));
    } catch {
      setNotice("Could not reach the server.");
    }
  };

  const play = async (player: PlayerInfo) => {
    try {
      const result = await client.POST("/api/v1/players/{id}/command", {
        params: { path: { id: player.id } },
        body: {
          action: "play",
          params: { itemId: props.itemId, startMs: props.startMs },
        },
      });
      setNotice(
        result.response?.status === 204
          ? `Playing on ${player.name}.`
          : `Could not start playback on ${player.name}.`,
      );
      setPlayers(null);
    } catch {
      setNotice("Could not reach the server.");
    }
  };

  return (
    <>
      <Button size="md" variant="secondary" onClick={open}>
        Play on device
      </Button>
      <Show when={players()}>
        {(list) => (
          <div role="dialog" aria-label="Choose a device" class="mbl-card-section">
            <Show
              when={list().length > 0}
              fallback={
                <p>No other device is playing MBL right now. Start playback there first.</p>
              }
            >
              <For each={list()}>
                {(p) => (
                  <Button size="sm" variant="secondary" onClick={() => play(p)}>
                    {p.name}
                  </Button>
                )}
              </For>
            </Show>
            <Button size="sm" variant="ghost" onClick={() => setPlayers(null)}>
              Cancel
            </Button>
          </div>
        )}
      </Show>
      <Show when={notice()}>
        <span role="status">{notice()}</span>
      </Show>
    </>
  );
}
