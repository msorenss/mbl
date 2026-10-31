import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../api/client";
import { LibraryFolderPicker } from "./LibraryFolderPicker";

vi.mock("../context/ThemeContext", () => ({ useTheme: () => ({ locale: () => "en" }) }));
let dispose: (() => void) | undefined;
const add = vi.fn();
const listing = (path: string | null) => ({ path, parent: path ? "/" : null, entries: [] });
const tick = () => vi.advanceTimersByTimeAsync(0);
const button = (text: string) => {
  const match = [...document.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!match) throw new Error(`Missing button ${text}`);
  return match;
};
async function openPath(path: string) {
  const input = document.getElementById("library-folder-path") as HTMLInputElement;
  input.value = path;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
  );
  await tick();
}
async function mount() {
  dispose = render(
    () => createComponent(LibraryFolderPicker, { paths: [], onAdd: add, onRemove: vi.fn() }),
    document.body,
  );
  await tick();
}
beforeEach(() => {
  vi.useFakeTimers();
  add.mockClear();
  vi.spyOn(client, "GET").mockResolvedValue({
    data: listing("/media"),
    response: new Response(),
  } as never);
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});

it("shows the REST permission error and prevents adding the previous folder as if the request succeeded", async () => {
  await mount();
  vi.mocked(client.GET).mockResolvedValueOnce({
    error: { error: { code: "validation", message: "path is not readable: /home/private" } },
    response: new Response(null, { status: 422 }),
  } as never);
  await openPath("/home/private");
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(
    "path is not readable: /home/private",
  );
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("mbl account");
  expect(button("Add current").disabled).toBe(true);
  button("Add current").click();
  expect(add).not.toHaveBeenCalled();
});

it("browses a manually entered path and adds only the server's canonical path", async () => {
  await mount();
  const input = document.getElementById("library-folder-path") as HTMLInputElement;
  input.value = "/media/../series";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  expect(button("Add current").disabled).toBe(true);
  button("Add current").click();
  expect(add).not.toHaveBeenCalled();
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: listing("/series"),
    response: new Response(),
  } as never);
  await openPath("/media/../series");
  expect(client.GET).toHaveBeenLastCalledWith(
    "/api/v1/system/browse",
    expect.objectContaining({ params: { query: { path: "/media/../series" } } }),
  );
  button("Add current").click();
  expect(add).toHaveBeenCalledWith("/series");
});

it("keeps a manually typed path when the initial root listing arrives", async () => {
  let resolveInitial!: (value: unknown) => void;
  vi.mocked(client.GET).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveInitial = resolve as (value: unknown) => void;
      }) as never,
  );
  await mount();
  const input = document.getElementById("library-folder-path") as HTMLInputElement;
  input.value = "/series";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  resolveInitial({ data: listing(null), response: new Response() });
  await tick();
  expect(input.value).toBe("/series");
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: listing("/series"),
    response: new Response(),
  } as never);
  input.dispatchEvent(
    new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }),
  );
  await tick();
  button("Add current").click();
  expect(add).toHaveBeenCalledWith("/series");
});

it("cancels an older browse and ignores its late result", async () => {
  await mount();
  let oldSignal: AbortSignal | undefined;
  let oldResolve!: (value: unknown) => void;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    oldSignal = (options as { signal?: AbortSignal | null } | undefined)?.signal ?? undefined;
    return new Promise((resolve) => {
      oldResolve = resolve as (value: unknown) => void;
    }) as never;
  });
  await openPath("/old");
  vi.mocked(client.GET).mockResolvedValueOnce({
    data: listing("/new"),
    response: new Response(),
  } as never);
  await openPath("/new");
  expect(oldSignal?.aborted).toBe(true);
  oldResolve({ data: listing("/old"), response: new Response() });
  await tick();
  expect((document.getElementById("library-folder-path") as HTMLInputElement).value).toBe("/new");
  button("Add current").click();
  expect(add).toHaveBeenCalledWith("/new");
});

it("times out a stalled request and cancels an outstanding request on unmount", async () => {
  await mount();
  let signal: AbortSignal | undefined;
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as { signal?: AbortSignal | null } | undefined)?.signal ?? undefined;
    return new Promise((_resolve, reject) => {
      signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), {
        once: true,
      });
    }) as never;
  });
  await openPath("/stalled");
  await vi.advanceTimersByTimeAsync(10000);
  expect(signal?.aborted).toBe(true);
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("timed out");
  vi.mocked(client.GET).mockImplementationOnce((_path, options) => {
    signal = (options as { signal?: AbortSignal | null } | undefined)?.signal ?? undefined;
    return new Promise(() => {}) as never;
  });
  await openPath("/other");
  dispose?.();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
});
