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
const ui = vi.hoisted(() => ({ views: [] as View[], toast: vi.fn(), play: vi.fn(), admin: true }));
vi.mock("../context/AuthContext", () => ({
  useAuth: () => ({ user: () => ({ isAdmin: ui.admin }) }),
}));
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
  ui.admin = true;
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

async function recoveryRecording() {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 31,
              title: "Interrupted archive",
              status: "failed",
              canDelete: true,
              channelId: null,
              startTime: 1,
              endTime: 2,
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
}

it("recovers saved media after the broadcast has ended, with an explicit confirmation", async () => {
  await recoveryRecording();
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings/{id}/recovery"
      ? (Promise.resolve({
          data: { canRecover: true, title: "Interrupted archive", sizeBytes: 4096 },
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Recover saved media")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining("incomplete recording"));
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter(
        (args: unknown[]) => args[0] === "/api/v1/livetv/recordings/{id}/recovery",
      ),
  ).toHaveLength(0);
  confirm.mockReturnValue(true);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Recover saved media")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/recordings/{id}/recovery",
    expect.objectContaining({ params: { path: { id: 31 } }, signal: expect.any(AbortSignal) }),
  );
});

it("explains missing saved media without sending a recovery mutation", async () => {
  await recoveryRecording();
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings/{id}/recovery"
      ? (Promise.resolve({
          data: {
            canRecover: false,
            title: "Interrupted archive",
            reason: "No saved capture file is available",
          },
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Recover saved media")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: "No saved capture file is available" }),
  );
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter(
        (args: unknown[]) => args[0] === "/api/v1/livetv/recordings/{id}/recovery",
      ),
  ).toHaveLength(0);
});

it("aborts a pending recovery preview and ignores its late response after leaving", async () => {
  await recoveryRecording();
  let resolve!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) => {
    if (path !== "/api/v1/livetv/recordings/{id}/recovery") return original(path, options) as never;
    signal = (options as { signal: AbortSignal }).signal;
    return new Promise((done) => {
      resolve = done;
    }) as never;
  });
  const confirm = vi.spyOn(window, "confirm");
  const button = Array.from(document.querySelectorAll("button")).find(
    (b) => b.textContent === "Recover saved media",
  );
  button?.click();
  button?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(
    vi
      .mocked(client.GET)
      .mock.calls.filter(
        (args: unknown[]) => args[0] === "/api/v1/livetv/recordings/{id}/recovery",
      ),
  ).toHaveLength(1);
  dispose?.();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
  resolve({ data: { canRecover: true, title: "Late archive" }, response: new Response() });
  await vi.advanceTimersByTimeAsync(0);
  expect(confirm).not.toHaveBeenCalled();
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter(
        (args: unknown[]) => args[0] === "/api/v1/livetv/recordings/{id}/recovery",
      ),
  ).toHaveLength(0);
});
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
              canDelete: true,
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

it("shares playback without exposing another profile's delete controls", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 9,
              title: "Shared DVR",
              status: "completed",
              itemId: 123,
              canDelete: false,
              startTime: 1,
              endTime: 2,
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.querySelector('[aria-label="Play recording Shared DVR"]')).not.toBeNull();
  expect(document.querySelector('[aria-label="Delete recording Shared DVR"]')).toBeNull();
});

it("submits season and retention choices with an explicit file-deletion warning", async () => {
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent?.includes("New Series Rule"))
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  const input = (selector: string, value: string) => {
    const element = document.querySelector<HTMLInputElement>(selector);
    expect(element).not.toBeNull();
    if (element) {
      element.value = value;
      element.dispatchEvent(new Event("input", { bubbles: true }));
    }
  };
  input("#newRuleTitleInput", "Series");
  input("#newRuleSeason", "2");
  input("#newRuleKeepMax", "3");
  const watched = Array.from(document.querySelectorAll("label"))
    .find((label) => label.textContent?.includes("Delete after my profile watches"))
    ?.querySelector<HTMLInputElement>("input");
  if (watched) {
    watched.checked = true;
    watched.dispatchEvent(new Event("change", { bubbles: true }));
  }
  expect(document.body.textContent).toContain("automatically deletes saved files");
  Array.from(document.querySelectorAll("button"))
    .find((button) => button.textContent === "Create Series Rule")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/rules",
    expect.objectContaining({
      body: expect.objectContaining({
        title: "Series",
        seasonNumber: 2,
        keepMax: 3,
        autoDeleteWatched: true,
      }),
    }),
  );
});

it("disables watching when the current programme is denied by the server", async () => {
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { channels: [{ channel: { ...channels[0], canPlay: false }, programs: [] }] },
    response: new Response(),
  } as never);
  await mount();
  const button = document.querySelector<HTMLButtonElement>('[aria-label="Watch One"]');
  expect(button?.disabled).toBe(true);
  expect(button?.title).toContain("profile");
  button?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(liveGets()).toHaveLength(0);
});

it("closes buffered playback at the programme deadline and releases the lease once", async () => {
  await mount();
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { ...lease(1), accessEndsAt: Date.now() + 5000 },
    response: new Response(),
  } as never);
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.views).toHaveLength(1);
  await vi.advanceTimersByTimeAsync(5000);
  expect(document.querySelector('[aria-label="Test live player"]')).toBeNull();
  expect(client.POST).toHaveBeenCalledTimes(1);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: expect.stringContaining("programme has ended") }),
  );
  await vi.advanceTimersByTimeAsync(15000);
  expect(liveGets()).toHaveLength(1);
});

it("clears the old programme timer when switching channels", async () => {
  await mount();
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { ...lease(1), accessEndsAt: Date.now() + 5000 },
    response: new Response(),
  } as never);
  watch("One");
  await vi.advanceTimersByTimeAsync(0);
  watch("Two");
  await vi.advanceTimersByTimeAsync(0);
  await vi.advanceTimersByTimeAsync(5000);
  expect(document.querySelector('[aria-label="Test live player"]')?.textContent).toContain("Two");
  expect(ui.toast).not.toHaveBeenCalled();
});

it("explains a profile denial without starting a player or retrying", async () => {
  await mount();
  vi.mocked(client.GET).mockResolvedValueOnce({
    error: { error: { message: "forbidden" } },
    response: new Response(null, { status: 403 }),
  } as never);
  watch("One");
  await vi.advanceTimersByTimeAsync(30000);
  expect(ui.views).toHaveLength(0);
  expect(liveGets()).toHaveLength(1);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: expect.stringContaining("permitted TV guide programme") }),
  );
});

it("refreshes programme availability as the live guide changes", async () => {
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: { channels: [{ channel: { ...channels[0], canPlay: false }, programs: [] }] },
    response: new Response(),
  } as never);
  await mount();
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Watch One"]')?.disabled).toBe(
    true,
  );
  await vi.advanceTimersByTimeAsync(30000);
  expect(document.querySelector<HTMLButtonElement>('[aria-label="Watch One"]')?.disabled).toBe(
    false,
  );
});

it("hides source administration controls from non-admin profiles", async () => {
  ui.admin = false;
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Tuners & Sources")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.body.textContent).toContain("Connected Tuners");
  expect(
    Array.from(document.querySelectorAll("button")).some((b) =>
      b.textContent?.includes("Discover HDHomeRun"),
    ),
  ).toBe(false);
  expect(
    Array.from(document.querySelectorAll("button")).some((b) =>
      b.textContent?.includes("Import XMLTV"),
    ),
  ).toBe(false);
});

it("labels and plays a partial recording while preserving retry and deletion controls", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 22,
              title: "Interrupted programme",
              status: "partial",
              itemId: 222,
              canDelete: true,
              channelId: 1,
              programId: 8,
              startTime: Date.now() - 10000,
              endTime: Date.now() + 60000,
              durationMs: 6000,
              errorMessage: "Incomplete recording: captured 6.0 s of 120.0 s scheduled",
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(document.body.textContent).toContain("PARTIAL");
  expect(document.body.textContent).toContain("captured 6.0 s");
  expect(
    document.querySelector('[aria-label="Play partial recording Interrupted programme"]'),
  ).not.toBeNull();
  document
    .querySelector<HTMLButtonElement>('[aria-label="Play partial recording Interrupted programme"]')
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.play).toHaveBeenCalledWith({
    itemId: 222,
    title: "Interrupted programme (Incomplete recording)",
  });
  vi.mocked(client.POST).mockResolvedValueOnce({
    data: {
      title: "Interrupted programme",
      ownerId: 1,
      ruleId: null,
      keepMax: null,
      autoDeleteWatched: false,
      conflict: null,
      storage: { canStart: true },
    },
    response: new Response(),
  } as never);
  const retry = Array.from(document.querySelectorAll("button")).find(
    (b) => b.textContent === "Retry",
  );
  expect(retry).not.toBeUndefined();
  retry?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/recordings",
    expect.objectContaining({
      body: expect.objectContaining({ retryOf: 22, programId: 8, channelId: 1 }),
    }),
  );
  expect(
    document.querySelector('[aria-label="Delete recording Interrupted programme"]'),
  ).not.toBeNull();
  const partial = Array.from(document.querySelectorAll("button")).find(
    (b) => b.textContent === "Partial",
  );
  partial?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.GET).toHaveBeenCalledWith(
    "/api/v1/livetv/recordings",
    expect.objectContaining({ params: { query: { status: "partial" } } }),
  );
});

it("reviews inherited retention before retrying and respects cancellation", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 23,
              title: "Retried episode",
              status: "failed",
              canDelete: true,
              channelId: 1,
              programId: 8,
              startTime: Date.now() - 1000,
              endTime: Date.now() + 60000,
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  const plan = {
    title: "Canonical episode",
    ownerId: 7,
    ruleId: 3,
    keepMax: 2,
    autoDeleteWatched: true,
    conflict: null,
    storage: { canStart: true },
  };
  vi.mocked(client.POST).mockResolvedValueOnce({ data: plan, response: new Response() } as never);
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Retry")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining("profile #7"));
  expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Keep latest 2"));
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter((args: unknown[]) => args[0] === "/api/v1/livetv/recordings"),
  ).toHaveLength(0);
  confirm.mockReturnValue(true);
  vi.mocked(client.POST).mockResolvedValueOnce({ data: plan, response: new Response() } as never);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Retry")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(client.POST).toHaveBeenCalledWith(
    "/api/v1/livetv/recordings",
    expect.objectContaining({ body: expect.objectContaining({ retryOf: 23 }) }),
  );
});

it("shows low disk space from the retry preview before creating another recording", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 24,
              title: "Disk full",
              status: "failed",
              canDelete: true,
              channelId: 1,
              programId: 8,
              startTime: Date.now() - 1000,
              endTime: Date.now() + 60000,
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  vi.mocked(client.POST).mockResolvedValueOnce({
    data: {
      title: "Disk full",
      conflict: null,
      storage: { canStart: false, error: "Insufficient free space for DVR recording" },
    },
    response: new Response(),
  } as never);
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "Retry")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(ui.toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: expect.stringContaining("free space") }),
  );
  expect(
    vi
      .mocked(client.POST)
      .mock.calls.filter((args: unknown[]) => args[0] === "/api/v1/livetv/recordings"),
  ).toHaveLength(0);
});

it("does not duplicate pending retry previews or confirm a late response after leaving", async () => {
  const original = vi.mocked(client.GET).getMockImplementation() as unknown as (
    path: string,
    options?: unknown,
  ) => unknown;
  vi.mocked(client.GET).mockImplementation((path, options) =>
    path === "/api/v1/livetv/recordings"
      ? (Promise.resolve({
          data: [
            {
              id: 25,
              title: "Late retry",
              status: "failed",
              canDelete: true,
              channelId: 1,
              programId: 8,
              startTime: Date.now() - 1000,
              endTime: Date.now() + 60000,
            },
          ],
          response: new Response(),
        }) as never)
      : (original(path, options) as never),
  );
  await mount();
  Array.from(document.querySelectorAll("button"))
    .find((b) => b.textContent === "DVR Recordings")
    ?.click();
  await vi.advanceTimersByTimeAsync(0);
  let finish!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.POST).mockImplementationOnce((_path, options) => {
    signal = (options as unknown as { signal: AbortSignal }).signal;
    return new Promise((resolve) => {
      finish = resolve;
    }) as never;
  });
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  const button = Array.from(document.querySelectorAll("button")).find(
    (b) => b.textContent === "Retry",
  );
  button?.click();
  button?.click();
  await vi.advanceTimersByTimeAsync(0);
  expect(button?.disabled).toBe(true);
  expect(client.POST).toHaveBeenCalledTimes(1);
  dispose?.();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
  finish({
    data: {
      title: "Late retry",
      ownerId: 1,
      ruleId: 2,
      keepMax: 1,
      autoDeleteWatched: true,
      storage: { canStart: true },
    },
    response: new Response(),
  });
  await vi.advanceTimersByTimeAsync(0);
  expect(confirm).not.toHaveBeenCalled();
  expect(client.POST).toHaveBeenCalledTimes(1);
  expect(ui.toast).not.toHaveBeenCalled();
});
