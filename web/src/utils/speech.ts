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

let serverStatus: Promise<boolean> | undefined;

/** Whether the server can recognize speech (asked once per page load). */
export function serverListens(): Promise<boolean> {
  serverStatus ??= client
    .GET("/api/v1/speech/status")
    .then((r) => r.data?.serverRecognition ?? false)
    .catch(() => false);
  return serverStatus;
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

/**
 * Start listening. The microphone is only on until `stop` or `cancel`.
 * `language` is a two-letter code such as `sv`.
 */
export async function startListening(engine: ListenEngine, language: string): Promise<Listening> {
  if (engine === "browser") {
    const Ctor = browserRecognition();
    if (!Ctor) throw new Error("This browser cannot recognize speech.");
    const recognition = new Ctor();
    recognition.lang = language;
    recognition.interimResults = false;
    recognition.continuous = true;
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
    recognition.onend = () => finished();
    recognition.start();
    return {
      stop: async () => {
        recognition.stop();
        await done;
        if (!text && failure && failure !== "no-speech")
          throw new Error("Speech could not be recognized.");
        return text;
      },
      cancel: () => {
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
  const release = () => {
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
export function canSpeak(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}

/**
 * Read `text` aloud in `language`. `onStart`/`onEnd` let the player lower its volume meanwhile.
 */
export function speak(
  text: string,
  language: string,
  hooks: { onStart?: () => void; onEnd?: () => void } = {},
) {
  if (!canSpeak() || !text.trim()) return;
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = language;
  utterance.onstart = () => hooks.onStart?.();
  utterance.onend = () => hooks.onEnd?.();
  utterance.onerror = () => hooks.onEnd?.();
  window.speechSynthesis.speak(utterance);
}

export function stopSpeaking() {
  if (canSpeak()) window.speechSynthesis.cancel();
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
