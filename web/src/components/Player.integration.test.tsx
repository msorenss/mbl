import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import client from "../api/client";
import { Player } from "./Player";

vi.mock("../context/ThemeContext", () => ({
  useTheme: () => ({
    subtitleStyle: () => ({ size: "md", background: "dim", color: "white" }),
    nightMode: () => false,
    setNightMode: vi.fn(),
  }),
}));
vi.mock("../api/deviceProfile", () => ({ getPreferredProfile: () => "firefox" }));
const playback = {
  sessionId: "session",
  method: "directPlay",
  url: "/movie.mp4",
  startMs: 0,
  subtitles: [],
  markers: [],
  durationMs: 120000,
};
let dispose: (() => void) | undefined;
let container: HTMLDivElement;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(client, "POST").mockResolvedValue({ data: playback, response: new Response() } as never);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  container.remove();
  vi.restoreAllMocks();
});
describe("rendered player lifecycle", () => {
  it("sends explicit start-over and selected tracks to playback negotiation", async () => {
    dispose = render(
      () =>
        createComponent(Player, {
          itemId: 7,
          initialStartMs: 0,
          audioStreamIndex: 2,
          subtitleStreamIndex: null,
          onClose: vi.fn(),
        }),
      container,
    );
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({
        body: expect.objectContaining({
          itemId: 7,
          startMs: 0,
          audioStreamIndex: 2,
          profile: "firefox",
        }),
      }),
    );
  });
  it("preserves a requested start position under ten seconds", async () => {
    dispose = render(
      () => createComponent(Player, { itemId: 7, initialStartMs: 5000, onClose: vi.fn() }),
      container,
    );
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback",
      expect.objectContaining({ body: expect.objectContaining({ startMs: 5000 }) }),
    );
  });
  it("releases exactly one session when unmounted", async () => {
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    await tick();
    dispose();
    dispose = undefined;
    await tick();
    expect(
      vi
        .mocked(client.POST)
        .mock.calls.filter((call: unknown[]) => call[0] === "/api/v1/playback/{session}/stop"),
    ).toHaveLength(1);
  });
  it("releases a session returned after the player was closed", async () => {
    let resolve!: (value: unknown) => void;
    vi.mocked(client.POST).mockImplementationOnce(
      () =>
        new Promise((done) => {
          resolve = done;
        }) as never,
    );
    dispose = render(() => createComponent(Player, { itemId: 7, onClose: vi.fn() }), container);
    dispose();
    dispose = undefined;
    resolve({ data: playback, response: new Response() });
    await tick();
    expect(client.POST).toHaveBeenCalledWith(
      "/api/v1/playback/{session}/stop",
      expect.objectContaining({ params: { path: { session: "session" } } }),
    );
    expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
  });
});
