import { afterEach, expect, it, vi } from "vitest";
import { getDeviceProfileInput } from "./deviceProfile";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("reports MP4, WebM and MSE codec support separately", () => {
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Chrome/145 Safari/537.36");
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockImplementation((mime) =>
    mime.includes("ac-3") || (mime.includes("webm") && mime.includes("av01")) ? "probably" : "",
  );
  vi.stubGlobal("MediaSource", {
    isTypeSupported: (mime: string) =>
      mime.includes("mp4a.40.2") || mime.includes("hev1") || mime.includes("ec-3"),
  });
  expect(getDeviceProfileInput()).toEqual({
    name: "chrome",
    browserCodecs: { mp4: ["ac3"], webm: ["av1"], hls: ["hevc", "eac3"] },
  });
});

it("uses native HLS when the Media Source implementation cannot play HLS", () => {
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Version/18 Safari/605.1");
  vi.stubGlobal("MediaSource", { isTypeSupported: () => false });
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockImplementation((mime) =>
    mime.includes("mpegurl") || mime.includes("hvc1") ? "maybe" : "",
  );
  expect(getDeviceProfileInput()).toEqual({
    name: "safari",
    browserCodecs: { mp4: ["hevc"], webm: [], hls: ["hevc"] },
  });
});

it("does not advertise optional codecs when the probes fail", () => {
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Chrome/145 Edg/145");
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockReturnValue("");
  vi.stubGlobal("MediaSource", {
    isTypeSupported: () => {
      throw new Error("codec probe failed");
    },
  });
  expect(getDeviceProfileInput()).toEqual({
    name: "edge",
    browserCodecs: { mp4: [], webm: [], hls: [] },
  });
});
