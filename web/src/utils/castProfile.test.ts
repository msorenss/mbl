import { afterEach, describe, expect, it, vi } from "vitest";
import {
  CAPABILITY_NAMESPACE,
  queryCastProfile,
  resolveCastProfile,
  SAFE_CAST_PROFILE,
} from "./castProfile";

const mode = (width = 1280, height = 720, fps = 30, avcLevel = 41) => ({
  width,
  height,
  fps,
  avcLevel,
  sdr8Bit: true,
});
const report = (modes = [mode()], nonce = "test") => ({
  schemaVersion: 1,
  revision: 1,
  nonce,
  video: true,
  hlsFmp4: true,
  mp4: true,
  mp3Stereo: true,
  aacStereo: true,
  modes,
});
afterEach(() => vi.useRealTimers());
describe("receiver capability selection", () => {
  it("selects conservative 720p without guessing a model", () => {
    expect(resolveCastProfile(report(), "test")).toEqual({
      ...SAFE_CAST_PROFILE,
      source: "receiver-probe",
    });
  });
  it("selects Hub and Google TV compatible limits from combinations", () => {
    expect(resolveCastProfile(report([mode(1280, 720, 60)]), "test")?.profile).toBe("nest-hub");
    expect(resolveCastProfile(report([mode(1920, 1080, 60, 42)]), "test")?.profile).toBe(
      "chromecast-gen3",
    );
  });
  it("never combines independent maxima", () => {
    expect(
      resolveCastProfile(report([mode(1920, 1080, 30), mode(1280, 720, 60)]), "test")?.profile,
    ).toBe("chromecast-legacy");
    expect(resolveCastProfile(report([mode(3840, 2160, 30, 51)]), "test")).toBeNull();
  });
  it("rejects stale, unknown and excessive reports", () => {
    for (const value of [
      report([], "old"),
      { ...report(), revision: 2 },
      { ...report(), model: "Bedroom TV" },
      report(Array(17).fill(mode())),
      { ...report(), aacStereo: false },
    ])
      expect(resolveCastProfile(value, "test")).toBeNull();
  });
  it("rejects audio-only receivers but ignores stale negative reports", () => {
    expect(() => resolveCastProfile({ ...report(), video: false }, "test")).toThrow(
      "does not support video",
    );
    expect(resolveCastProfile({ ...report(), video: false }, "new")).toBeNull();
  });
  it("uses safe fallback when no channel exists", async () => {
    expect(await queryCastProfile({})).toEqual(SAFE_CAST_PROFILE);
  });
  it("times out and removes listeners; late replies cannot raise limits", async () => {
    vi.useFakeTimers();
    let listener!: (ns: string, data: string) => void;
    let nonce = "";
    const session = {
      addMessageListener: vi.fn((_ns, cb) => {
        listener = cb;
      }),
      removeMessageListener: vi.fn(),
      sendMessage: vi.fn(async (_ns, data) => {
        nonce = data.nonce;
      }),
    };
    const pending = queryCastProfile(session);
    listener(CAPABILITY_NAMESPACE, JSON.stringify(report([mode(1920, 1080, 60, 42)], "old")));
    await vi.advanceTimersByTimeAsync(3000);
    expect(await pending).toEqual(SAFE_CAST_PROFILE);
    listener(CAPABILITY_NAMESPACE, JSON.stringify(report([mode(1920, 1080, 60, 42)], nonce)));
    expect(await pending).toEqual(SAFE_CAST_PROFILE);
    expect(session.removeMessageListener).toHaveBeenCalledTimes(1);
  });
  it("accepts only the current nonce and cleans up after success", async () => {
    let listener!: (ns: string, data: string) => void;
    const session = {
      addMessageListener: vi.fn((_ns, cb) => {
        listener = cb;
      }),
      removeMessageListener: vi.fn(),
      sendMessage: vi.fn(async (_ns, data) => {
        listener(CAPABILITY_NAMESPACE, JSON.stringify(report([mode(1280, 720, 60)], data.nonce)));
      }),
    };
    expect((await queryCastProfile(session)).profile).toBe("nest-hub");
    expect(session.removeMessageListener).toHaveBeenCalledTimes(1);
  });
  it("falls back after unsupported channel rejection", async () => {
    expect(
      await queryCastProfile({
        addMessageListener: vi.fn(),
        removeMessageListener: vi.fn(),
        sendMessage: vi.fn().mockRejectedValue(new Error("unsupported")),
      }),
    ).toEqual(SAFE_CAST_PROFILE);
  });
});
