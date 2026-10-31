import { describe, expect, it } from "vitest";
import { formatRelativeTime } from "./time";

describe("formatRelativeTime", () => {
  it("formats seconds as Just now", () => {
    const now = Date.now();
    expect(formatRelativeTime(now - 10000)).toBe("Just now");
  });

  it("formats minutes ago", () => {
    const now = Date.now();
    expect(formatRelativeTime(now - 5 * 60 * 1000)).toBe("5m ago");
  });

  it("formats hours ago", () => {
    const now = Date.now();
    expect(formatRelativeTime(now - 3 * 3600 * 1000)).toBe("3h ago");
  });

  it("formats days ago", () => {
    const now = Date.now();
    expect(formatRelativeTime(now - 4 * 86400 * 1000)).toBe("4d ago");
  });
});
