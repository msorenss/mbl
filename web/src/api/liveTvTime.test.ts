import { expect, it } from "vitest";
import { GUIDE_SLOT_MS, guideWindow } from "./liveTvTime";

it("keeps XMLTV programme timestamps and the guide in the same millisecond interval", () => {
  const now = Date.UTC(2026, 9, 27, 20, 17);
  const window = guideWindow(now);
  expect(window.windowStart).toBe(Date.UTC(2026, 9, 27, 20));
  expect(window.windowEnd - window.windowStart).toBe(2 * 60 * 60 * 1000);
  const programStart = Date.UTC(2026, 9, 27, 20, 30);
  expect((programStart - window.windowStart) / (window.windowEnd - window.windowStart)).toBe(0.25);
  expect(GUIDE_SLOT_MS).toBe(1_800_000);
});
