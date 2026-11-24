import { createRoot } from "solid-js";
import { afterEach, expect, it, vi } from "vitest";
import { createTimedSignal } from "./timedSignal";

afterEach(() => vi.useRealTimers());

it("clears a message after its time, restarts the time when set again", () => {
  vi.useFakeTimers();
  createRoot((dispose) => {
    const [message, setMessage] = createTimedSignal<string>("", 1000);
    setMessage("Receiver error");
    vi.advanceTimersByTime(800);
    expect(message()).toBe("Receiver error");
    setMessage("Another error");
    vi.advanceTimersByTime(800);
    expect(message()).toBe("Another error");
    vi.advanceTimersByTime(300);
    expect(message()).toBe("");
    setMessage("x");
    setMessage("");
    expect(message()).toBe("");
    dispose();
  });
});
