import { describe, expect, it } from "vitest";
import { detectBrowserCapabilities, getPreferredProfile } from "./deviceProfile";

describe("deviceProfile", () => {
  it("detects default browser capabilities in node/jsdom environment", () => {
    const caps = detectBrowserCapabilities();
    expect(caps).toBeDefined();
    expect(typeof caps.profileName).toBe("string");
    expect(typeof caps.canPlayH264).toBe("boolean");
    expect(typeof caps.isHlsSupported).toBe("boolean");
  });

  it("returns preferred profile name", () => {
    const profile = getPreferredProfile();
    expect(["chrome", "firefox", "safari", "edge", "generic-hls"]).toContain(profile);
  });
});
