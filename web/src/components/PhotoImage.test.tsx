import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, expect, it, vi } from "vitest";
import client from "../api/client";
import PhotoImage from "./PhotoImage";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it("loads an authenticated binary image and releases its object URL and request on close", async () => {
  const revoke = vi.fn();
  const OriginalURL = URL;
  vi.stubGlobal(
    "URL",
    class extends OriginalURL {
      static createObjectURL() {
        return "blob:photo";
      }
      static revokeObjectURL = revoke;
    },
  );
  let signal: AbortSignal | undefined;
  vi.spyOn(client, "GET").mockImplementation(async (_path, options) => {
    signal = (options as { signal: AbortSignal }).signal;
    return { data: new Blob(["image"], { type: "image/png" }), response: new Response() } as never;
  });
  const ready = vi.fn();
  dispose = render(
    () => createComponent(PhotoImage, { partId: 8, title: "Photo", eager: true, onReady: ready }),
    document.body,
  );
  await tick();
  expect(client.GET).toHaveBeenCalledWith(
    "/api/v1/stream/direct/{partId}",
    expect.objectContaining({ params: { path: { partId: 8 } }, parseAs: "blob" }),
  );
  expect(document.querySelector("img")?.src).toBe("blob:photo");
  document.querySelector("img")?.dispatchEvent(new Event("load"));
  expect(ready).toHaveBeenCalledOnce();
  dispose();
  dispose = undefined;
  expect(signal?.aborted).toBe(true);
  expect(revoke).toHaveBeenCalledWith("blob:photo");
});
it("reports unavailable image bytes rather than showing a successful image", async () => {
  vi.spyOn(client, "GET").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 404 }),
  } as never);
  const failure = vi.fn();
  dispose = render(
    () =>
      createComponent(PhotoImage, { partId: 8, title: "Photo", eager: true, onFailure: failure }),
    document.body,
  );
  await tick();
  expect(document.querySelector("img")).toBeNull();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Photo unavailable");
  expect(failure).toHaveBeenCalledOnce();
});
