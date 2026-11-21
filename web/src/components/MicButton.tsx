import { createResource, createSignal, onCleanup, Show } from "solid-js";
import { type Listening, listenEngine, startListening } from "../utils/speech";

/**
 * Talk instead of typing: hold the button (or click once to start and once to stop). The
 * microphone is only on while it says "Listening…". Hidden when neither the server nor the
 * browser can recognize speech.
 */
export default function MicButton(props: {
  language: string;
  onText: (text: string) => void;
  onError?: (message: string) => void;
  style?: Record<string, string>;
}) {
  const [engine] = createResource(listenEngine);
  const [listening, setListening] = createSignal(false);
  const [working, setWorking] = createSignal(false);
  let active: Listening | undefined;
  let heldSince = 0;
  let starting = false;
  // Released while the microphone was still starting: stop as soon as it has.
  let stopWhenStarted = false;

  const begin = async () => {
    const which = engine();
    if (!which || active || starting || working()) return;
    starting = true;
    stopWhenStarted = false;
    try {
      active = await startListening(which, props.language);
      setListening(true);
      if (stopWhenStarted) void finish();
    } catch {
      active = undefined;
      props.onError?.("The microphone could not be used.");
    } finally {
      starting = false;
    }
  };

  const finish = async () => {
    const current = active;
    if (!current) return;
    active = undefined;
    setListening(false);
    setWorking(true);
    try {
      const text = await current.stop();
      if (text) props.onText(text);
      else props.onError?.("No speech was heard.");
    } catch (e) {
      props.onError?.(e instanceof Error ? e.message : "Speech could not be recognized.");
    } finally {
      setWorking(false);
    }
  };

  onCleanup(() => active?.cancel());

  return (
    <Show when={engine()}>
      {(which) => (
        <button
          type="button"
          aria-pressed={listening()}
          title={
            which() === "server"
              ? "Hold to talk. Speech is recognized on this server."
              : "Hold to talk. Speech is recognized by your browser's service."
          }
          disabled={working()}
          onPointerDown={(e) => {
            e.preventDefault();
            // A second press ends a recording that a short click started.
            if (listening()) {
              void finish();
              return;
            }
            heldSince = Date.now();
            void begin();
          }}
          onPointerUp={() => {
            // Holding for a moment means push-to-talk: releasing ends it.
            if (heldSince === 0 || Date.now() - heldSince <= 400) return;
            heldSince = 0;
            if (starting) stopWhenStarted = true;
            else void finish();
          }}
          onKeyDown={(e) => {
            if (e.key !== "Enter" && e.key !== " ") return;
            e.preventDefault();
            if (listening()) void finish();
            else void begin();
          }}
          style={{
            display: "inline-flex",
            gap: "6px",
            "align-items": "center",
            padding: "6px 10px",
            "border-radius": "8px",
            border: "1px solid rgba(255,255,255,0.3)",
            background: listening() ? "#c62828" : "transparent",
            color: "inherit",
            cursor: "pointer",
            ...props.style,
          }}
        >
          <Show when={listening()} fallback={<span>{working() ? "Recognizing…" : "Talk"}</span>}>
            <span aria-hidden="true">●</span>
            <span>Listening…</span>
          </Show>
        </button>
      )}
    </Show>
  );
}
