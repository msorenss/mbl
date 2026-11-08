import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { expect, it, vi } from "vitest";
import client from "../api/client";
import { LibraryScanControl } from "./LibraryScanControl";

it("tracks a queued scan through completion and reloads the library", async () => {
  vi.useFakeTimers();
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  const get = vi
    .spyOn(client, "GET")
    .mockResolvedValueOnce({ response: new Response(null, { status: 204 }) } as never);
  get.mockResolvedValueOnce({
    response: new Response(),
    data: { jobId: 16, status: "running", progress: 0.4 },
  } as never);
  get.mockResolvedValue({
    response: new Response(),
    data: { jobId: 16, status: "done", progress: 1 },
  } as never);
  vi.spyOn(client, "POST").mockResolvedValue({
    response: new Response(),
    data: { jobId: 16 },
  } as never);
  const complete = vi.fn();
  const root = document.createElement("div");
  document.body.append(root);
  const dispose = render(
    () => createComponent(LibraryScanControl, { id: 2, onComplete: complete }),
    root,
  );
  try {
    await vi.advanceTimersByTimeAsync(0);
    root.querySelector("button")?.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(root.textContent).toContain("Scanning library 40%");
    expect(root.querySelector("button")?.disabled).toBe(true);
    await vi.advanceTimersByTimeAsync(3000);
    expect(root.textContent).toContain("Scan complete");
    expect(root.querySelector("button")?.disabled).toBe(false);
    expect(complete).toHaveBeenCalledOnce();
  } finally {
    dispose();
    root.remove();
    vi.restoreAllMocks();
    vi.useRealTimers();
  }
});
