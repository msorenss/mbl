import { createSignal, onCleanup, onMount, Show } from "solid-js";
import type { components } from "../api/schema";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";
import { readAloud, readAloudProblem, setReadAloud, speak } from "../utils/speech";

type PlayerComments = components["schemas"]["PlayerComments"];

const SURFACE = "remote";
const [speaking, setSpeaking] = createSignal(readAloud(SURFACE));

/**
 * Watches what companions say about what one player plays and shows it on this remote control:
 * as a toast, or read aloud when "Read comments aloud" is on. Renders nothing itself.
 */
export function CommentWatcher(props: {
  player: string;
  fetch: (afterId: number | undefined) => Promise<PlayerComments | undefined>;
}) {
  const { showToast } = useToast();
  const { locale } = useI18n();
  let afterId: number | undefined;
  let busy = false;
  const poll = async () => {
    if (busy) return;
    busy = true;
    try {
      const result = await props.fetch(afterId);
      if (!result) return;
      // The first answer only says where the feed stands: show what is said from now on.
      const first = afterId === undefined;
      afterId = result.lastId;
      if (first) return;
      for (const comment of result.comments) {
        if (speaking()) speak(comment.commentary, locale());
        else
          showToast({
            title: `${comment.botName} · ${props.player}`,
            message: comment.commentary,
            type: "info",
          });
      }
    } catch {
    } finally {
      busy = false;
    }
  };
  onMount(() => {
    void poll();
    const timer = setInterval(() => void poll(), 3000);
    onCleanup(() => clearInterval(timer));
  });
  return null;
}

/** "Read comments aloud" for the remote control; otherwise they show as toasts. */
export function ReadCommentsAloud() {
  const { locale } = useI18n();
  const [problem, setProblem] = createSignal("");
  return (
    <>
      <label style={{ display: "flex", gap: "8px", "align-items": "center" }}>
        <input
          type="checkbox"
          checked={speaking()}
          onChange={(e) => {
            const on = e.currentTarget.checked;
            setSpeaking(on);
            setReadAloud(SURFACE, on);
            setProblem("");
            if (on) void readAloudProblem(locale()).then((p) => p && setProblem(p));
          }}
        />
        Read companion comments aloud (otherwise they pop up here)
      </label>
      <Show when={problem()}>
        <p role="alert">{problem()}</p>
      </Show>
    </>
  );
}
