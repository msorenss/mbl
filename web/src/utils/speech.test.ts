import { expect, it } from "vitest";
import { LONGEST_MS, NOTHING_HEARD_MS, SILENCE_MS, silenceDetector } from "./speech";

const speech = 0.1;
const quiet = 0.005;

it("stops after a moment of quiet once something was said", () => {
  const shouldStop = silenceDetector(0);
  expect(shouldStop(quiet, 500)).toBe(false);
  expect(shouldStop(speech, 1000)).toBe(false);
  expect(shouldStop(speech, 2000)).toBe(false);
  expect(shouldStop(quiet, 2000 + SILENCE_MS - 100)).toBe(false);
  expect(shouldStop(quiet, 2000 + SILENCE_MS)).toBe(true);
});

it("gives up when nothing is said, and never runs past the longest recording", () => {
  const silent = silenceDetector(0);
  expect(silent(quiet, NOTHING_HEARD_MS - 100)).toBe(false);
  expect(silent(quiet, NOTHING_HEARD_MS)).toBe(true);
  const talker = silenceDetector(0);
  for (let at = 0; at < LONGEST_MS; at += 1000) expect(talker(speech, at)).toBe(false);
  expect(talker(speech, LONGEST_MS)).toBe(true);
});
