import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import LiveStreamPlayer from "./LiveStreamPlayer";

type Double = {
  handlers: Map<string, (event: string, data?: unknown) => void>;
  startLoad: ReturnType<typeof vi.fn>;
  stopLoad: ReturnType<typeof vi.fn>;
  attachMedia: ReturnType<typeof vi.fn>;
  destroy: ReturnType<typeof vi.fn>;
};
const sdk = vi.hoisted(() => ({ instance: null as Double | null, immediateFailure: false }));
vi.mock("hls.js", () => ({
  default: class {
    static isSupported = () => true;
    static Events = { MANIFEST_PARSED: "manifest", ERROR: "error", FRAG_BUFFERED: "buffered" };
    handlers = new Map<string, (event: string, data?: unknown) => void>();
    constructor() {
      sdk.instance = this;
    }
    on = vi.fn((event: string, handler: (event: string, data?: unknown) => void) =>
      this.handlers.set(event, handler),
    );
    loadSource = vi.fn(() => {
      if (sdk.immediateFailure)
        this.handlers.get("error")?.("error", { type: "mediaError", fatal: true });
    });
    startLoad = vi.fn();
    stopLoad = vi.fn();
    attachMedia = vi.fn();
    destroy = vi.fn();
  },
}));
let dispose: (() => void) | undefined;
const failure = vi.fn();
const ended = vi.fn();
beforeEach(() => {
  vi.useFakeTimers();
  sdk.immediateFailure = false;
  failure.mockClear();
  ended.mockClear();
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
});
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.useRealTimers();
});
async function mount(url = "/api/v1/stream/live/lease/index.m3u8") {
  dispose = render(
    () =>
      createComponent(LiveStreamPlayer, {
        url,
        title: "Channel",
        onClose: vi.fn(),
        onFailure: failure,
        onEnded: ended,
      }),
    document.body,
  );
  await vi.advanceTimersByTimeAsync(0);
  return sdk.instance!;
}
it("attaches server HLS and releases natural EOF through the owning page", async () => {
  const hls = await mount();
  hls.handlers.get("manifest")?.("manifest");
  expect(hls.attachMedia).toHaveBeenCalled();
  expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
  document.querySelector("video")?.dispatchEvent(new Event("ended"));
  expect(ended).toHaveBeenCalledTimes(1);
});
it("notifies failure once and stops HLS retry and media activity", async () => {
  const hls = await mount();
  hls.handlers.get("error")?.("error", {
    type: "networkError",
    fatal: true,
    response: { code: 404 },
  });
  document.querySelector("video")?.dispatchEvent(new Event("error"));
  hls.handlers.get("manifest")?.("manifest");
  expect(failure).toHaveBeenCalledTimes(1);
  expect(hls.stopLoad).toHaveBeenCalled();
  expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("Select the channel again");
});
it("bounds live network recovery and cancels delayed callbacks on close", async () => {
  const hls = await mount();
  const error = () =>
    hls.handlers.get("error")?.("error", {
      type: "networkError",
      fatal: true,
      details: "fragLoadError",
      response: { code: 503 },
    });
  error();
  await vi.advanceTimersByTimeAsync(1000);
  error();
  await vi.advanceTimersByTimeAsync(3000);
  error();
  expect(hls.startLoad).toHaveBeenCalledTimes(2);
  expect(failure).toHaveBeenCalledTimes(1);
  dispose?.();
  dispose = undefined;
  await vi.advanceTimersByTimeAsync(30000);
  expect(hls.destroy).toHaveBeenCalledTimes(1);
  expect(hls.startLoad).toHaveBeenCalledTimes(2);
});
it("captures synchronous source failures before attaching and rejects unsupported URLs", async () => {
  sdk.immediateFailure = true;
  const hls = await mount();
  expect(failure).toHaveBeenCalledTimes(1);
  expect(hls.attachMedia).not.toHaveBeenCalled();
  dispose?.();
  dispose = undefined;
  failure.mockClear();
  await mount("javascript:invalid");
  expect(failure).toHaveBeenCalledTimes(1);
});
it("respects pause while recovering a live manifest", async () => {
  const hls = await mount();
  hls.handlers.get("error")?.("error", {
    type: "networkError",
    fatal: true,
    details: "manifestLoadError",
    response: { code: 503 },
  });
  document.querySelector("video")?.dispatchEvent(new Event("pause"));
  await vi.advanceTimersByTimeAsync(1000);
  hls.handlers.get("manifest")?.("manifest");
  hls.handlers.get("buffered")?.("buffered");
  await vi.advanceTimersByTimeAsync(30000);
  expect(failure).not.toHaveBeenCalled();
  expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
});
