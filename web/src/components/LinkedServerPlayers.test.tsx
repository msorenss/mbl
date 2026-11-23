import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import LinkedServerPlayers from "./LinkedServerPlayers";

vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: vi.fn() }) }));

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});
const tick = () => new Promise((r) => setTimeout(r, 0));

const view = {
  id: "w1",
  name: "Windows",
  url: "http://192.168.1.20:8420",
  error: null,
  players: [
    {
      id: "tv",
      name: "Living room",
      deviceType: "web",
      state: "playing",
      nowPlayingItemId: 7,
      nowPlayingTitle: "Dark Matter",
      positionMs: 60_000,
      durationMs: 3_600_000,
      volume: 40,
    },
  ],
};

it("shows another server's players and sends commands there", async () => {
  vi.spyOn(client, "GET").mockImplementation(async (path) =>
    path === "/api/v1/linked-servers"
      ? ({ data: [view], response: new Response() } as never)
      : ({
          data: [{ id: 3, title: "Severance", kind: "show", year: 2022 }],
          response: new Response(),
        } as never),
  );
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(() => createComponent(LinkedServerPlayers, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("Other MBL servers");
  expect(document.body.textContent).toContain("Living room");
  expect(document.body.textContent).toContain("Dark Matter");

  const pause = [...document.querySelectorAll("button")].find((b) => b.textContent === "Pause");
  pause?.click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/linked-servers/{id}/players/{player}/command", {
    params: { path: { id: "w1", player: "tv" } },
    body: { action: "pause" },
  });

  // Search the other library and play a title there.
  const input = document.querySelector("input[type=search]") as HTMLInputElement;
  input.value = "sev";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.form?.dispatchEvent(new Event("submit", { cancelable: true, bubbles: true }));
  await tick();
  await tick();
  expect(document.body.textContent).toContain("Severance");
  const play = [...document.querySelectorAll("li button")].find((b) => b.textContent === "Play");
  (play as HTMLButtonElement).click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/linked-servers/{id}/players/{player}/play", {
    params: { path: { id: "w1", player: "tv" } },
    body: { itemId: 3 },
  });
});

it("says when a linked server cannot be reached and shows nothing without links", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: [{ ...view, players: [], error: "could not reach MBL" }],
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(LinkedServerPlayers, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("Not reachable: could not reach MBL");
  dispose();
  document.body.innerHTML = "";
  vi.spyOn(client, "GET").mockResolvedValue({ data: [], response: new Response() } as never);
  dispose = render(() => createComponent(LinkedServerPlayers, {}), document.body);
  await tick();
  expect(document.body.textContent).toBe("");
});
