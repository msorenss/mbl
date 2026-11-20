import { afterEach, expect, it, vi } from "vitest";
import client, { setStoredToken } from "../api/client";
import { startLocalRemote } from "./localRemote";

afterEach(() => {
  setStoredToken(null);
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("connects the player's command socket with the stored token as a subprotocol", async () => {
  setStoredToken("abc123");
  vi.spyOn(client, "POST").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    response: new Response(null, { status: 204 }),
  } as never);
  const opened: { url: string; protocols?: string[] }[] = [];
  vi.stubGlobal(
    "WebSocket",
    class {
      onmessage = null;
      onclose = null;
      constructor(url: URL, protocols?: string[]) {
        opened.push({ url: String(url), protocols });
      }
      close() {}
    },
  );
  const stop = startLocalRemote(
    () => ({ state: "playing", itemId: 3, positionMs: 0, durationMs: 1000, volume: 50 }),
    () => {},
  );
  await vi.waitFor(() => expect(opened).toHaveLength(1));
  expect(opened[0].url).toMatch(/^ws:\/\/.+\/api\/v1\/players\/.+\/connect$/);
  // A WebSocket cannot send the Authorization header; the token goes as a subprotocol so the
  // session matches the one that registered the player.
  expect(opened[0].protocols).toEqual(["mbl.v1", "bearer.abc123"]);
  stop();
});
