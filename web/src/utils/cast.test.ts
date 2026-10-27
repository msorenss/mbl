import { beforeEach, describe, expect, it, vi } from "vitest";
import { castManager } from "./cast";

describe("castManager", () => {
  beforeEach(() => {
    castManager.stopCasting();
  });

  it("initializes with default state", () => {
    const state = castManager.getState();
    expect(state.isCasting).toBe(false);
    expect(state.deviceName).toBeNull();
    expect(state.playerState).toBe("idle");
    expect(state.currentTime).toBe(0);
    expect(state.volume).toBe(1);
  });

  it("subscribes and receives state updates", () => {
    const listener = vi.fn();
    const unsubscribe = castManager.subscribe(listener);

    expect(listener).toHaveBeenCalledWith(
      expect.objectContaining({
        isCasting: false,
        playerState: "idle",
      }),
    );

    unsubscribe();
  });

  it("handles stopCasting cleanly", () => {
    castManager.stopCasting();
    const state = castManager.getState();
    expect(state.isCasting).toBe(false);
    expect(state.deviceName).toBeNull();
  });
});
