import { afterEach, describe, expect, it, vi } from "vitest";
import {
  __resetPwaForTests,
  type BeforeInstallPromptEvent,
  canInstallPwa,
  initPwaSupport,
  isStandaloneApp,
  promptInstallPwa,
} from "./pwa";

describe("PWA install & standalone runtime", () => {
  afterEach(() => {
    __resetPwaForTests();
    vi.restoreAllMocks();
  });

  it("captures beforeinstallprompt and completes install prompt flow", async () => {
    initPwaSupport();
    expect(canInstallPwa()).toBe(false);
    expect(isStandaloneApp()).toBe(false);

    const promptSpy = vi.fn().mockResolvedValue(undefined);
    const event = new Event("beforeinstallprompt", {
      cancelable: true,
    }) as BeforeInstallPromptEvent;
    Object.defineProperty(event, "prompt", { value: promptSpy });
    Object.defineProperty(event, "userChoice", {
      value: Promise.resolve({ outcome: "accepted" as const, platform: "web" }),
    });

    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    expect(canInstallPwa()).toBe(true);

    const accepted = await promptInstallPwa();
    expect(promptSpy).toHaveBeenCalledTimes(1);
    expect(accepted).toBe(true);
    expect(canInstallPwa()).toBe(false);
  });

  it("marks app as standalone when appinstalled fires", () => {
    initPwaSupport();
    window.dispatchEvent(new Event("appinstalled"));
    expect(isStandaloneApp()).toBe(true);
    expect(canInstallPwa()).toBe(false);
  });
});
