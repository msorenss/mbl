import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { localPlayerId, localPlayerName } from "../utils/localRemote";
import Remote, { formatClock } from "./Remote";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const button = (text: string) =>
  [...document.querySelectorAll("button")].find((b) => b.textContent === text) as HTMLButtonElement;

it("formats clock times", () => {
  expect(formatClock(0)).toBe("0:00");
  expect(formatClock(65_000)).toBe("1:05");
  expect(formatClock(3_725_000)).toBe("1:02:05");
});

it("names the browser and keeps one id per tab", () => {
  expect(localPlayerName("Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko Firefox/140.0")).toBe(
    "Firefox on Linux",
  );
  expect(
    localPlayerName("Mozilla/5.0 (Linux; Android 15) AppleWebKit Chrome/140.0 Mobile Safari/537"),
  ).toBe("Chrome on Android");
  expect(localPlayerId()).toBe(localPlayerId());
});

it("lists other players, hides this tab and sends commands", async () => {
  vi.spyOn(client, "GET").mockImplementation(((path: string) =>
    Promise.resolve({
      data:
        path === "/api/v1/players"
          ? [
              {
                id: localPlayerId(),
                name: "This tab",
                deviceType: "web",
                state: "playing",
                positionMs: 0,
                durationMs: 0,
                volume: 100,
              },
              {
                id: "tv",
                name: "Bedroom TV",
                deviceType: "chromecast",
                state: "playing",
                nowPlayingItemId: 7,
                positionMs: 60_000,
                durationMs: 600_000,
                volume: 40,
              },
            ]
          : { title: "Heat" },
      response: new Response(),
    })) as never);
  const post = vi
    .spyOn(client, "POST")
    .mockResolvedValue({ response: new Response(null, { status: 204 }) } as never);
  dispose = render(() => createComponent(Remote, {}), document.body);
  await tick();
  await tick();
  expect(document.body.textContent).toContain("Bedroom TV");
  expect(document.body.textContent).not.toContain("This tab");
  expect(document.body.textContent).toContain("Heat");

  button("Pause").click();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/players/{id}/command", {
    params: { path: { id: "tv" } },
    body: { action: "pause" },
  });
  expect(button("Play")).toBeTruthy();

  button("+30 s").click();
  await tick();
  expect(post).toHaveBeenLastCalledWith("/api/v1/players/{id}/command", {
    params: { path: { id: "tv" } },
    body: { action: "seek", params: { positionMs: 90_000 } },
  });
});

it("explains when a device does not listen", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: [
      {
        id: "tv",
        name: "Bedroom TV",
        deviceType: "chromecast",
        state: "paused",
        positionMs: 0,
        durationMs: 0,
        volume: 40,
      },
    ],
    response: new Response(),
  } as never);
  vi.spyOn(client, "POST").mockResolvedValue({
    response: new Response(null, { status: 503 }),
  } as never);
  dispose = render(() => createComponent(Remote, {}), document.body);
  await tick();
  button("Play").click();
  await tick();
  expect(document.body.textContent).toContain(
    "Bedroom TV is not listening for commands right now.",
  );
});
