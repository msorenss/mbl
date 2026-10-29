import { createComponent, createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import TrickplayPreview from "./TrickplayPreview";

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
const manifest = {
  partId: 7,
  intervalMs: 10000,
  columns: 10,
  rows: 10,
  tileWidth: 320,
  tileHeight: 180,
  tiles: [
    { spriteFile: "sprite_000.jpg", x: 0, y: 0 },
    { spriteFile: "sprite_000.jpg", x: 320, y: 0 },
    { spriteFile: "sprite_001.jpg", x: 0, y: 0 },
  ],
};
let dispose: (() => void) | undefined;
beforeEach(() => {
  vi.stubGlobal(
    "URL",
    Object.assign(URL, { createObjectURL: vi.fn(() => "blob:preview"), revokeObjectURL: vi.fn() }),
  );
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data: String(path).endsWith("index.json")
          ? manifest
          : new Blob(["jpg"], { type: "image/jpeg" }),
        response: new Response(),
      }) as never,
  );
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});
it("crops the selected tile, reuses the current sheet and releases blobs when it changes or closes", async () => {
  const [time, setTime] = createSignal(0);
  dispose = render(
    () =>
      createComponent(TrickplayPreview, {
        partId: 7,
        get timeMs() {
          return time();
        },
      }),
    document.body,
  );
  await tick();
  await tick();
  const image = document.querySelector('[role="img"]') as HTMLElement;
  expect(image.style.backgroundImage).toContain("blob:preview");
  const calls = vi.mocked(client.GET).mock.calls.length;
  setTime(10000);
  await tick();
  expect(image.style.backgroundPosition).toContain("-160px");
  expect(vi.mocked(client.GET).mock.calls).toHaveLength(calls);
  setTime(20000);
  await tick();
  await tick();
  expect(client.GET).toHaveBeenCalledWith(
    "/api/v1/stream/trickplay/{partId}/{sprite}",
    expect.objectContaining({
      params: { path: { partId: 7, sprite: "sprite_001.jpg" } },
      parseAs: "blob",
    }),
  );
  expect(URL.revokeObjectURL).toHaveBeenCalledTimes(1);
  dispose();
  dispose = undefined;
  expect(URL.revokeObjectURL).toHaveBeenCalledTimes(2);
});
it("shows pending idle work without fetching fake images and cancels polling on close", async () => {
  vi.useFakeTimers();
  vi.mocked(client.GET).mockResolvedValue({
    data: { status: "queued" },
    response: new Response(null, { status: 202 }),
  } as never);
  dispose = render(
    () => createComponent(TrickplayPreview, { partId: 7, timeMs: 1000 }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(0);
  expect(document.body.textContent).toContain("queued for idle generation");
  expect(document.querySelector('[role="img"]')).toBeNull();
  dispose();
  dispose = undefined;
  const count = vi.mocked(client.GET).mock.calls.length;
  await vi.advanceTimersByTimeAsync(10000);
  expect(vi.mocked(client.GET).mock.calls).toHaveLength(count);
});
it("aborts a pending sheet and ignores a late response after closing", async () => {
  let resolve!: (value: unknown) => void;
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementation(async (path, options) => {
    if (String(path).endsWith("index.json"))
      return { data: manifest, response: new Response() } as never;
    signal = (options as { signal: AbortSignal }).signal;
    return new Promise((done) => {
      resolve = done;
    }) as never;
  });
  dispose = render(
    () => createComponent(TrickplayPreview, { partId: 7, timeMs: 1000 }),
    document.body,
  );
  await tick();
  dispose();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
  resolve({ data: new Blob(["jpg"]), response: new Response() });
  await tick();
  expect(URL.createObjectURL).not.toHaveBeenCalled();
});
it("explains how to retry a transient preview instead of displaying a fabricated image", async () => {
  vi.mocked(client.GET).mockResolvedValue({
    error: {},
    response: new Response(null, { status: 503 }),
  } as never);
  dispose = render(
    () => createComponent(TrickplayPreview, { partId: 7, timeMs: 1000 }),
    document.body,
  );
  await tick();
  expect(document.body.textContent).toContain("Preview unavailable");
  expect(document.querySelector('[role="img"]')).toBeNull();
  expect(document.body.textContent).toContain("Hover or focus the timeline again to retry");
});

it("discards a late manifest when the player switches media parts", async () => {
  const [part, setPart] = createSignal(7);
  let firstSignal: AbortSignal | undefined;
  let resolve!: (value: unknown) => void;
  vi.mocked(client.GET).mockImplementation(async (path, options) => {
    const request = options as { params: { path: { partId: number } }; signal: AbortSignal };
    if (String(path).endsWith("index.json")) {
      if (request.params.path.partId === 7) {
        firstSignal = request.signal;
        return new Promise((done) => {
          resolve = done;
        }) as never;
      }
      return { data: { ...manifest, partId: 8 }, response: new Response() } as never;
    }
    return { data: new Blob(["jpg"]), response: new Response() } as never;
  });
  dispose = render(
    () =>
      createComponent(TrickplayPreview, {
        get partId() {
          return part();
        },
        timeMs: 0,
      }),
    document.body,
  );
  await tick();
  setPart(8);
  await tick();
  await tick();
  expect(firstSignal?.aborted).toBe(true);
  resolve({ data: manifest, response: new Response() });
  await tick();
  expect(client.GET).not.toHaveBeenCalledWith(
    "/api/v1/stream/trickplay/{partId}/{sprite}",
    expect.objectContaining({ params: { path: { partId: 7, sprite: "sprite_000.jpg" } } }),
  );
  expect(URL.createObjectURL).toHaveBeenCalledTimes(1);
});
