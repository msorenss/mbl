import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { LiveTv } from "./LiveTv";

type View = {
  title: string;
  onClose: () => void;
  onFailure: (message: string) => void;
  onEnded: () => void;
};
const ui = vi.hoisted(() => ({ views: [] as View[], toast: vi.fn(), play: vi.fn() }));
vi.mock("../context/PlayerContext", () => ({ usePlayer: () => ({ playItem: ui.play }) }));
vi.mock("../context/ToastContext", () => ({ useToast: () => ({ showToast: ui.toast }) }));
vi.mock("../components/LiveStreamPlayer", () => ({
  default: (props: View) => {
    ui.views.push(props);
    return (
      <section aria-label="Test live player">
        {props.title}
        <button type="button" onClick={props.onClose}>
          Close player
        </button>
      </section>
    );
  },
}));
const channels = [
  { id: 1, name: "One", channelNumber: "1", isFavorite: false },
  { id: 2, name: "Two", channelNumber: "2", isFavorite: false },
];
const lease = (id: number) => ({
  channelId: id,
  channelName: id === 1 ? "One" : "Two",
  sessionId: `lease-${id}`,
  streamUrl: `/api/v1/stream/live/lease-${id}/index.m3u8`,
  expiresAt: Date.now() + 45000,
  targetFormat: "hls",
  isDeinterlacingRequired: false,
});
let dispose: (() => void) | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  ui.views = [];
  ui.toast.mockClear();
  ui.play.mockClear();
  vi.spyOn(client, "POST").mockResolvedValue({ response: new Response() } as never);
  vi.spyOn(client, "GET").mockImplementation(
    (path, options) =>
      Promise.resolve({
        data:
          path === "/api/v1/livetv/epg"
            ? { channels: channels.map((channel) => ({ channel, programs: [] })) }
            : path === "/api/v1/livetv/channels"
              ? channels
              : path === "/api/v1/livetv/stream/{channelId}"
                ? lease(
                    (options as unknown as { params: { path: { channelId: number } } }).params.path
                      .channelId,
                  )
                : [],
        response: new Response(),
      }) as never,
  );
});
afterEach(async () => {
  dispose?.();
  dispose = undefined;
  await vi.advanceTimersByTimeAsync(0);
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});
async function mount() {
  dispose = render(() => createComponent(LiveTv, {}), document.body);
  await vi.advanceTimersByTimeAsync(0);
}
function watch(name: string) {
  document.querySelector<HTMLButtonElement>(`[aria-label="Watch ${name}"]`)?.click();
}
function liveGets() {
  return vi
    .mocked(client.GET)
    .mock.calls.filter((args: unknown[]) => args[0] === "/api/v1/livetv/stream/{channelId}");
}
it("starts the returned relay, renews its owned lease and releases it on close", async () => {
  await mount();
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.views).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(15000);
  expect(client.GET).toHaveBeenLastCalledWith(
    "/api/v1/livetv/stream/{channelId}",
    expect.objectContaining({
      params: { path: { channelId: 1 }, query: { sessionId: "lease-1" } },
      signal: expect.any(AbortSignal),
    }),
  );
  ui.views[0].onClose();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/stream/{channelId}/release",
    expect.objectContaining({
      params: { path: { channelId: 1 }, query: { sessionId: "lease-1" } },
    }),
  );
  await vi.advanceTimersByTimeAsync(45000);
  expect(liveGets()).toHaveLength(2);
  expect(document.querySelector('[aria-label="Test live player"]')).toBeNull();
});
it("waits for the old encoder release before switching channels and ignores stale callbacks", async () => {
  await mount();
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  const old = ui.views[0];
  let release!: (value: unknown) => void;
  vi.mocked(client.POST).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        release = resolve;
      }) as never,
  );
  watch("Two");
  await vi.advanceTimersByTimeAsync(0);
  expect(liveGets()).toHaveLength(1);
  expect(document.body.textContent).toContain("Preparing live playback");
  release({ response: new Response() });
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.views.at(-1)?.title).toBe("Two");
  old.onFailure("Stale error");
  old.onEnded();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelector('[aria-label="Test live player"]')?.textContent).toContain("Two");
  expect(ui.toast).not.toHaveBeenCalled();
});
it("aborts cancelled startup and releases a late accepted reservation", async () => {
  await mount();
  let answer!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as unknown as { signal: AbortSignal }).signal;
    return new Promise((resolve) => {
      answer = resolve;
    }) as never;
  });
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Cancel")
    ?.click();
  expect(signal?.aborted).toBe(true);
  answer({ data: lease(1), response: new Response() });
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.views).toHaveLength(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/stream/{channelId}/release",
    expect.objectContaining({
      params: { path: { channelId: 1 }, query: { sessionId: "lease-1" } },
    }),
  );
});
it("releases failed playback and stops renewing while retaining the error view until closed", async () => {
  await mount();
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  ui.views[0].onFailure("Encoder failed");
  await vi.advanceTimersByTimeAsync(45000);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: "Encoder failed", type: "error" }),
  );
  expect(liveGets()).toHaveLength(1);
  ui.views[0].onClose();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledTimes(1);
});
it("does not overlap renewals and clears an expired reservation", async () => {
  await mount();
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  let finish!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as unknown as { signal: AbortSignal }).signal;
    return new Promise((resolve) => {
      finish = resolve;
    }) as never;
  });
  await vi.advanceTimersByTimeAsync(30000);
  expect(liveGets()).toHaveLength(2);
  expect(signal?.aborted).toBe(true);
  finish({
    error: { error: { message: "Lease is gone" } },
    response: new Response(null, { status: 404 }),
  });
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelector('[aria-label="Test live player"]')).toBeNull();
  expect(ui.toast).toHaveBeenCalledWith(expect.objectContaining({ message: "Lease is gone" }));
});
it("shows the real capacity error and cancels startup on unmount", async () => {
  await mount();
  vi.mocked(client.GET).mockResolvedValueOnce({
    error: { error: { message: "video encoder capacity unavailable" } },
    response: new Response(null, { status: 503 }),
  } as never);
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: "video encoder capacity unavailable" }),
  );
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as unknown as { signal: AbortSignal }).signal;
    return new Promise(() => {}) as never;
  });
  watch("Two");
  await vi.advanceTimersByTimeAsync(0);
  dispose?.();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
});

it("does not offer an unimplemented virtual channel as a tuner stream", async () => {
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { channels: [{ channel: { ...channels[0], isVirtual: true }, programs: [] }] },
    response: new Response(),
  } as never);
  await mount();
  const button = document.querySelector<HTMLButtonElement>('[aria-label="Watch One"]');
  expect(button?.disabled).toBe(true);
  button?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(liveGets()).toHaveLength(0);
});

it("plays a completed DVR item through the normal player after releasing live capacity", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 10,
              title: "Ready DVR",
              status: "completed",
              itemId: 123,
              durationMs: 65432,
              startTime: 1,
              endTime: 2,
            },
            { id: 11, title: "Old DVR", status: "completed", startTime: 1, endTime: 2 },
            { id: 12, title: "Active DVR", status: "recording", startTime: 1, endTime: 2 },
          ],
          response: new Response(),
        }) as never)
      : (original?.(path, options) as never),
  );
  await mount();
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.body.textContent).toContain("1m 5s");
  expect(document.body.textContent).toContain("Playback unavailable");
  expect(document.querySelector('[aria-label="Play recording Old DVR"]')).toBeNull();
  expect(document.querySelector('[aria-label="Play recording Active DVR"]')).toBeNull();
  let release: (() => void) | undefined;
  vi.mocked(client.POST).mockImplementation(
    () =>
      new Promise((resolve) => {
        release = () => resolve({ response: new Response() } as never);
      }),
  );
  document.querySelector<HTMLButtonElement>('[aria-label="Play recording Ready DVR"]')?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.play).not.toHaveBeenCalled();
  release?.();
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.play).toHaveBeenCalledWith({ itemId: 123, title: "Ready DVR" });
  expect(ui.views).toHaveLength(1);
});

it("refreshes DVR readiness and asks to delete the saved file for completed recordings", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  let ready = false;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 10,
              title: "New DVR",
              status: ready ? "completed" : "recording",
              itemId: ready ? 456 : undefined,
              startTime: 1,
              endTime: 2,
            },
          ],
          response: new Response(),
        }) as never)
      : (original?.(path, options) as never),
  );
  const confirmDelete = vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    data: { success: true },
    response: new Response(),
  } as never);
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelector('[aria-label="Play recording New DVR"]')).toBeNull();
  ready = true;
  await vi.advanceTimersByTimeAsync(10000);
  expect(document.querySelector('[aria-label="Play recording New DVR"]')).not.toBeNull();
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "Delete")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(confirmDelete).toHaveBeenCalledWith('Delete recording "New DVR" and its saved file?');
  expect(client.DELETE).toHaveBeenCalledWith("/api/v1/livetv/recordings/{id}", {
    params: { path: { id: 10 } },
  });
});
