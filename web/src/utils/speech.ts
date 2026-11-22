import client, { getStoredToken } from "../api/client";

/** Where speech is recognized: on the server (Whisper on the NPU) or by the browser. */
export type ListenEngine = "server" | "browser";

interface RecognitionLike {
  lang: string;
  interimResults: boolean;
  continuous: boolean;
  onresult: ((event: { results: ArrayLike<ArrayLike<{ transcript: string }>> }) => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onend: (() => void) | null;
  start: () => void;
  stop: () => void;
}

type RecognitionCtor = new () => RecognitionLike;

function browserRecognition(): RecognitionCtor | undefined {
  const w = window as unknown as {
    SpeechRecognition?: RecognitionCtor;
    webkitSpeechRecognition?: RecognitionCtor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition;
}

interface ServerSpeech {
  recognition: boolean;
  /** Languages the server can read aloud (Piper voices). */
  voices: string[];
}

let serverStatus: Promise<ServerSpeech> | undefined;

function serverSpeech(): Promise<ServerSpeech> {
  serverStatus ??= client
    .GET("/api/v1/speech/status")
    .then((r) => ({
      recognition: r.data?.serverRecognition ?? false,
      voices: r.data?.serverVoices ?? [],
    }))
    .catch(() => ({ recognition: false, voices: [] }));
  return serverStatus;
}

/** Whether the server can recognize speech (asked once per page load). */
export async function serverListens(): Promise<boolean> {
  return (await serverSpeech()).recognition;
}

/** Whether the server can read `language` aloud. */
export async function serverSpeaks(language: string): Promise<boolean> {
  return (await serverSpeech()).voices.includes(language);
}

/** Forget what the server could do, after an administrator installed or removed a voice. */
export function forgetServerSpeech() {
  serverStatus = undefined;
}

/** The engine to use, or `null` when neither the server nor the browser can listen. */
export async function listenEngine(): Promise<ListenEngine | null> {
  if (
    (await serverListens()) &&
    typeof MediaRecorder !== "undefined" &&
    typeof navigator.mediaDevices?.getUserMedia === "function"
  )
    return "server";
  return browserRecognition() ? "browser" : null;
}

/** An active recording; `stop` ends it and gives the text. */
export interface Listening {
  stop: () => Promise<string>;
  cancel: () => void;
}

/** Above this level (RMS of the samples, 0–1) the microphone hears speech. */
export const SPEECH_LEVEL = 0.02;
/** Quiet this long after speech ends the recording. */
export const SILENCE_MS = 1500;
/** Nothing heard this long after starting ends it too. */
export const NOTHING_HEARD_MS = 8000;
/** Never record longer than the server accepts. */
export const LONGEST_MS = 60_000;

/**
 * Decide from the microphone level whether to stop: after `SILENCE_MS` of quiet once speech was
 * heard, after `NOTHING_HEARD_MS` without any, and at `LONGEST_MS`.
 */
export function silenceDetector(now: number) {
  const started = now;
  let heard = false;
  let quietSince = now;
  return (level: number, at: number): boolean => {
    if (level > SPEECH_LEVEL) {
      heard = true;
      quietSince = at;
    }
    return (
      (heard && at - quietSince >= SILENCE_MS) ||
      (!heard && at - started >= NOTHING_HEARD_MS) ||
      at - started >= LONGEST_MS
    );
  };
}

/** Watch the microphone and call `onSilence` once when the speaker has stopped. */
function watchSilence(stream: MediaStream, onSilence: () => void): () => void {
  const Ctx =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  if (!Ctx) return () => undefined;
  const context = new Ctx();
  const analyser = context.createAnalyser();
  analyser.fftSize = 1024;
  context.createMediaStreamSource(stream).connect(analyser);
  const samples = new Float32Array(analyser.fftSize);
  const shouldStop = silenceDetector(Date.now());
  const stop = () => {
    clearInterval(timer);
    void context.close().catch(() => undefined);
  };
  const timer = setInterval(() => {
    analyser.getFloatTimeDomainData(samples);
    let sum = 0;
    for (const v of samples) sum += v * v;
    if (shouldStop(Math.sqrt(sum / samples.length), Date.now())) {
      stop();
      onSilence();
    }
  }, 100);
  return stop;
}

/**
 * Start listening. The microphone is only on until `stop` or `cancel`, or until the speaker has
 * been quiet for a moment: then `onSilence` is called and the caller should `stop`.
 * `language` is a two-letter code such as `sv`.
 */
export async function startListening(
  engine: ListenEngine,
  language: string,
  onSilence?: () => void,
): Promise<Listening> {
  if (engine === "browser") {
    const Ctor = browserRecognition();
    if (!Ctor) throw new Error("This browser cannot recognize speech.");
    const recognition = new Ctor();
    recognition.lang = language;
    recognition.interimResults = false;
    // Without `continuous` the browser ends the recognition when the speaker pauses.
    recognition.continuous = !onSilence;
    let text = "";
    let failure = "";
    let finished: () => void = () => undefined;
    const done = new Promise<void>((resolve) => {
      finished = resolve;
    });
    recognition.onresult = (event) => {
      text = Array.from(event.results)
        .map((r) => r[0]?.transcript ?? "")
        .join(" ")
        .trim();
    };
    recognition.onerror = (event) => {
      failure = event.error;
    };
    let stopping = false;
    recognition.onend = () => {
      finished();
      if (!stopping) onSilence?.();
    };
    recognition.start();
    return {
      stop: async () => {
        stopping = true;
        recognition.stop();
        await done;
        if (!text && failure && failure !== "no-speech")
          throw new Error("Speech could not be recognized.");
        return text;
      },
      cancel: () => {
        stopping = true;
        recognition.onresult = null;
        recognition.stop();
      },
    };
  }

  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  const recorder = new MediaRecorder(stream);
  const chunks: Blob[] = [];
  recorder.ondataavailable = (e) => {
    if (e.data.size > 0) chunks.push(e.data);
  };
  const stopped = new Promise<void>((resolve) => {
    recorder.onstop = () => resolve();
  });
  const unwatch = onSilence ? watchSilence(stream, onSilence) : () => undefined;
  const release = () => {
    unwatch();
    for (const track of stream.getTracks()) track.stop();
  };
  recorder.start();
  return {
    stop: async () => {
      if (recorder.state !== "inactive") recorder.stop();
      await stopped;
      release();
      const blob = new Blob(chunks, { type: recorder.mimeType || "audio/webm" });
      if (blob.size === 0) return "";
      // Only the file header, not one sample: the microphone delivered no sound at all.
      if (blob.size < 1000)
        throw new Error(
          "The microphone sent no sound. Check that the right microphone is chosen and not muted.",
        );
      const token = getStoredToken();
      const response = await fetch(
        `/api/v1/speech/transcribe?language=${encodeURIComponent(language)}`,
        {
          method: "POST",
          headers: {
            "Content-Type": blob.type,
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
          },
          body: blob,
        },
      );
      if (!response.ok) throw new Error("Speech could not be recognized.");
      const result = (await response.json()) as { text?: string };
      return result.text?.trim() ?? "";
    },
    cancel: () => {
      if (recorder.state !== "inactive") recorder.stop();
      release();
    },
  };
}

/** Whether the browser can read text aloud. */
/** Whether replies can be read aloud here: by the browser, or by the server and played. */
export function canSpeak(): boolean {
  return typeof window !== "undefined" && ("speechSynthesis" in window || "Audio" in window);
}

/** The browser's voices. Some browsers load them a moment after the page; wait up to 1.5 s. */
export function browserVoices(): Promise<SpeechSynthesisVoice[]> {
  if (!canSpeak()) return Promise.resolve([]);
  const now = window.speechSynthesis.getVoices();
  if (now.length > 0) return Promise.resolve(now);
  return new Promise((resolve) => {
    const done = () => resolve(window.speechSynthesis.getVoices());
    window.speechSynthesis.addEventListener?.("voiceschanged", done, { once: true });
    setTimeout(done, 1500);
  });
}

/** A voice for `language` (two-letter code), preferring one that works offline. */
export function voiceFor(
  voices: SpeechSynthesisVoice[],
  language: string,
): SpeechSynthesisVoice | undefined {
  const matching = voices.filter((v) => v.lang.toLowerCase().startsWith(language.toLowerCase()));
  return matching.find((v) => v.localService) ?? matching[0];
}

/**
 * Why reading aloud will not work as expected, or `null`. A browser without voices (Chromium
 * on Linux without a speech engine, for example) stays silent without any error.
 */
export async function readAloudProblem(language: string): Promise<string | null> {
  const voices = await browserVoices();
  if (!voiceFor(voices, language) && (await serverSpeaks(language))) return null;
  if (voices.length === 0)
    return "This browser has no voices to read with. Install a speech voice on this device or use another browser.";
  if (!voiceFor(voices, language))
    return "This browser has no voice for your language, so replies are read with another voice.";
  return null;
}

let serverAudio: HTMLAudioElement | undefined;

/**
 * Read `text` aloud in `language`. `onStart`/`onEnd` let the player lower its volume meanwhile.
 * Uses the browser's own voice for the language when it has one, otherwise the server's (Piper),
 * which also works in browsers without any voices.
 */
export function speak(
  text: string,
  language: string,
  hooks: { onStart?: () => void; onEnd?: () => void } = {},
) {
  if (!text.trim()) return;
  const browserVoice =
    canSpeakInBrowser() && voiceFor(window.speechSynthesis.getVoices(), language);
  if (browserVoice) {
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = language;
    utterance.voice = browserVoice;
    utterance.onstart = () => hooks.onStart?.();
    utterance.onend = () => hooks.onEnd?.();
    utterance.onerror = () => hooks.onEnd?.();
    window.speechSynthesis.speak(utterance);
    return;
  }
  void (async () => {
    if (await serverSpeaks(language)) {
      await speakOnServer(text, language, hooks);
    } else if (canSpeakInBrowser()) {
      // No voice for the language anywhere: the browser's default voice is better than silence.
      const utterance = new SpeechSynthesisUtterance(text);
      utterance.lang = language;
      utterance.onstart = () => hooks.onStart?.();
      utterance.onend = () => hooks.onEnd?.();
      utterance.onerror = () => hooks.onEnd?.();
      window.speechSynthesis.speak(utterance);
    }
  })();
}

/** Replies read one after another, like the browser's own queue. */
let serverQueue: Promise<void> = Promise.resolve();

function speakOnServer(
  text: string,
  language: string,
  hooks: { onStart?: () => void; onEnd?: () => void },
): Promise<void> {
  const turn = serverQueue.then(async () => {
    try {
      const token = getStoredToken();
      const response = await fetch("/api/v1/speech/say", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          ...(token ? { Authorization: `Bearer ${token}` } : {}),
        },
        body: JSON.stringify({ text, language }),
      });
      if (!response.ok) return;
      const url = URL.createObjectURL(await response.blob());
      const audio = new Audio(url);
      serverAudio = audio;
      await new Promise<void>((resolve) => {
        const done = () => {
          URL.revokeObjectURL(url);
          if (serverAudio === audio) serverAudio = undefined;
          hooks.onEnd?.();
          resolve();
        };
        audio.onplay = () => hooks.onStart?.();
        audio.onended = done;
        audio.onerror = done;
        audio.onpause = () => {
          if (!audio.ended) done();
        };
        void audio.play().catch(done);
      });
    } catch {
      hooks.onEnd?.();
    }
  });
  serverQueue = turn;
  return turn;
}

function canSpeakInBrowser(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

export function stopSpeaking() {
  if (canSpeakInBrowser()) window.speechSynthesis.cancel();
  serverAudio?.pause();
  serverAudio = undefined;
  serverQueue = Promise.resolve();
}

const SPEAK_KEY = "mbl.speech.readAloud";

/** Whether replies are read aloud in a chat (`player` or `assistant`), off by default. */
export function readAloud(surface: string): boolean {
  try {
    return localStorage.getItem(`${SPEAK_KEY}.${surface}`) === "1";
  } catch {
    return false;
  }
}

export function setReadAloud(surface: string, on: boolean) {
  try {
    localStorage.setItem(`${SPEAK_KEY}.${surface}`, on ? "1" : "0");
  } catch {}
}
