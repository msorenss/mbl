import { expect, it, vi } from "vitest";
import { LONGEST_MS, NOTHING_HEARD_MS, SILENCE_MS, silenceDetector, speechChunks } from "./speech";

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

it("picks a voice for the language and says when the browser has none", async () => {
  const { readAloudProblem, voiceFor } = await import("./speech");
  const voice = (lang: string, localService: boolean) =>
    ({ lang, localService, name: lang }) as SpeechSynthesisVoice;
  const voices = [voice("en-US", true), voice("sv-SE", false), voice("sv-SE", true)];
  expect(voiceFor(voices, "sv")).toBe(voices[2]);
  expect(voiceFor(voices, "de")).toBeUndefined();

  let installed: SpeechSynthesisVoice[] = [];
  Object.defineProperty(window, "speechSynthesis", {
    configurable: true,
    value: { getVoices: () => installed, addEventListener: () => undefined },
  });
  vi.useFakeTimers();
  const none = readAloudProblem("sv");
  await vi.advanceTimersByTimeAsync(1500);
  expect(await none).toContain("no voices");
  vi.useRealTimers();
  installed = voices;
  expect(await readAloudProblem("de")).toContain("no voice for your language");
  expect(await readAloudProblem("sv")).toBeNull();
});

it("reads the first sentence on its own so the voice starts soon", () => {
  const reply =
    "Det är Jason. Han har just vaknat i en annan värld! Kvinnan bredvid honom är Amanda, som hjälper honom att förstå lådan. Vill du veta mer?";
  const chunks = speechChunks(reply);
  expect(chunks[0]).toBe("Det är Jason.");
  expect(chunks.join(" ")).toBe(reply);
  expect(chunks.every((c) => c.length < 260)).toBe(true);
  expect(speechChunks("Utan punkt")).toEqual(["Utan punkt"]);
  expect(speechChunks("   ")).toEqual([]);
});

it("starts reading finished sentences while the reply is still coming", async () => {
  const { streamingSpeaker } = await import("./speech");
  const spoken: string[] = [];
  Object.defineProperty(window, "speechSynthesis", {
    configurable: true,
    value: {
      getVoices: () => [{ lang: "sv-SE", localService: true, name: "sv" }],
      speak: (u: { text: string }) => spoken.push(u.text),
      addEventListener: () => undefined,
    },
  });
  (globalThis as { SpeechSynthesisUtterance?: unknown }).SpeechSynthesisUtterance = class {
    text: string;
    constructor(text: string) {
      this.text = text;
    }
  };
  const speaker = streamingSpeaker("sv");
  speaker.update("Det är Jason");
  expect(spoken).toEqual([]);
  speaker.update("Det är Jason. Han vaknar");
  expect(spoken).toEqual(["Det är Jason."]);
  speaker.update("Det är Jason. Han vaknar i en annan värld. Sedan");
  expect(spoken).toEqual(["Det är Jason.", "Han vaknar i en annan värld."]);
  speaker.finish("Det är Jason. Han vaknar i en annan värld. Sedan går han.");
  expect(spoken[2]).toBe("Sedan går han.");
});
