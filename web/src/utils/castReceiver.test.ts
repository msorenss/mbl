import { afterEach, describe, expect, it, vi } from "vitest";
import { CAPABILITY_NAMESPACE, resolveCastProfile } from "./castProfile";
import {
  capabilityReport,
  installCapabilityChannel,
  type ReceiverContext,
  receiverTelemetry,
} from "./castReceiver";
import { receiverConfiguration, saveReceiverConfiguration } from "./castReceiverConfig";

const nonce = "6ee4db53-7ff9-46b7-90b8-90c97452ff48";
function context(maxWidth = 1280, maxFps = 60): ReceiverContext {
  return {
    canDisplayType: vi.fn(
      (mime, _codec, width, _height, fps) =>
        mime.startsWith("audio/") || (!!width && width <= maxWidth && !!fps && fps <= maxFps),
    ),
    getDeviceCapabilities: () => ({ display_supported: true }),
    addCustomMessageListener: vi.fn(),
    sendCustomMessage: vi.fn(),
  };
}
afterEach(() => {
  vi.useRealTimers();
  localStorage.removeItem("mbl.cast.receiver.v1");
});
describe("CAF receiver", () => {
  it("probes explicit combinations and resolves Hub versus Google TV limits", () => {
    const hub = context();
    expect(resolveCastProfile(capabilityReport(hub, nonce), nonce)?.profile).toBe("nest-hub");
    expect(resolveCastProfile(capabilityReport(context(1920), nonce), nonce)?.profile).toBe(
      "chromecast-gen3",
    );
    expect(resolveCastProfile(capabilityReport(context(3840), nonce), nonce)?.profile).toBe(
      "google-tv-streamer",
    );
    expect(hub.canDisplayType).toHaveBeenCalledWith("video/mp4", "avc1.640029", 1280, 720, 60);
    expect(hub.canDisplayType).toHaveBeenCalledWith("video/mp4", "avc1.42e029", 1280, 720, 60);
  });
  it("does not infer video capability from missing display metadata", () => {
    const unknown = context();
    unknown.getDeviceCapabilities = () => ({});
    expect(capabilityReport(unknown, nonce).modes).toEqual([]);
    expect(resolveCastProfile(capabilityReport(unknown, nonce), nonce)).toBeNull();
    unknown.getDeviceCapabilities = () => ({ display_supported: false });
    expect(() => resolveCastProfile(capabilityReport(unknown, nonce), nonce)).toThrow(
      "does not support video",
    );
  });
  it("requires all H264 profiles and audio support; failed probes stay conservative", () => {
    const ctx = context();
    ctx.canDisplayType = vi.fn(() => {
      throw new Error("not ready");
    });
    expect(resolveCastProfile(capabilityReport(ctx, nonce), nonce)).toBeNull();
  });
  it("replies only to the requesting sender and rejects malformed requests", () => {
    const ctx = context();
    installCapabilityChannel(ctx);
    const callback = vi.mocked(ctx.addCustomMessageListener).mock.calls[0][1];
    for (const data of [
      null,
      "x".repeat(300),
      { type: "probe", schemaVersion: 2, nonce },
      { type: "probe", schemaVersion: 1, nonce, token: "secret" },
    ])
      callback({ data, senderId: "sender" });
    expect(ctx.sendCustomMessage).not.toHaveBeenCalled();
    callback({ data: { type: "probe", schemaVersion: 1, nonce }, senderId: "sender" });
    expect(ctx.sendCustomMessage).toHaveBeenCalledWith(
      CAPABILITY_NAMESPACE,
      "sender",
      expect.objectContaining({ nonce, revision: 1 }),
    );
    callback({ data: { type: "probe", schemaVersion: 1, nonce }, senderId: "sender" });
    expect(ctx.sendCustomMessage).toHaveBeenCalledTimes(1);
  });
  it("exports numeric telemetry without URLs, tokens or SDK errors", () => {
    const payload = receiverTelemetry("ERROR", {
      detailedErrorCode: 301,
      totalBitrate: NaN,
      url: "https://secret",
      error: { token: "secret" },
    });
    expect(payload).toEqual({
      schemaVersion: 1,
      revision: 1,
      event: "ERROR",
      detailedErrorCode: 301,
    });
    expect(receiverTelemetry("UNRECOGNIZED", {})).toBeNull();
  });
  it("supports explicit custom and standard receiver applications", () => {
    saveReceiverConfiguration("a1b2c3d4");
    expect(receiverConfiguration()).toEqual({ appId: "A1B2C3D4", custom: true });
    saveReceiverConfiguration("CC1AD845");
    expect(receiverConfiguration()).toEqual({ appId: "CC1AD845", custom: false });
    expect(() => saveReceiverConfiguration("https://bad")).toThrow();
    saveReceiverConfiguration(null);
  });
});
