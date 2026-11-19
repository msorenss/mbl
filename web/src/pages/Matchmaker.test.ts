import { expect, it } from "vitest";
import { seededShuffle } from "./Matchmaker";

it("gives everyone in a session the same order", () => {
  const list = Array.from({ length: 20 }, (_, i) => i);
  expect(seededShuffle(list, "mm_abc")).toEqual(seededShuffle(list, "mm_abc"));
  expect(seededShuffle(list, "mm_abc")).not.toEqual(seededShuffle(list, "mm_xyz"));
  expect([...seededShuffle(list, "mm_abc")].sort((a, b) => a - b)).toEqual(list);
});
