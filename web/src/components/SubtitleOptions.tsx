import { Show } from "solid-js";
import type { SubtitleStyle } from "../context/ThemeContext";

/**
 * How subtitles look and whether they are moved to the speech: next to quality and audio before
 * playback and in the player's settings. The language itself is chosen in the subtitle menu.
 */
export default function SubtitleOptions(props: {
  style: SubtitleStyle;
  onChange: (patch: Partial<SubtitleStyle>) => void;
  /** Hide the timing option where it cannot apply (for example while casting). */
  hideMatchSpeech?: boolean;
  disabled?: boolean;
}) {
  return (
    <div class="mbl-player-sub-style">
      <label>
        Subtitle size
        <select
          value={props.style.size}
          disabled={props.disabled}
          onChange={(e) => props.onChange({ size: e.currentTarget.value as SubtitleStyle["size"] })}
        >
          <option value="sm">Small</option>
          <option value="md">Medium</option>
          <option value="lg">Large</option>
          <option value="xl">Extra large</option>
        </select>
      </label>
      <label>
        Font
        <select
          value={props.style.font ?? "sans"}
          disabled={props.disabled}
          onChange={(e) => props.onChange({ font: e.currentTarget.value as SubtitleStyle["font"] })}
        >
          <option value="sans">Sans-serif</option>
          <option value="serif">Serif</option>
          <option value="mono">Monospace</option>
        </select>
      </label>
      <label>
        Color
        <select
          value={props.style.color}
          disabled={props.disabled}
          onChange={(e) =>
            props.onChange({ color: e.currentTarget.value as SubtitleStyle["color"] })
          }
        >
          <option value="white">White</option>
          <option value="yellow">Yellow</option>
          <option value="cyan">Cyan</option>
        </select>
      </label>
      <label>
        Background
        <select
          value={props.style.background}
          disabled={props.disabled}
          onChange={(e) =>
            props.onChange({ background: e.currentTarget.value as SubtitleStyle["background"] })
          }
        >
          <option value="none">None</option>
          <option value="dim">Dimmed</option>
          <option value="solid">Solid</option>
        </select>
      </label>
      <Show when={!props.hideMatchSpeech}>
        <label
          title="For subtitles timed for another release or frame rate. The first time takes as long as reading the film once."
          style={{ display: "flex", gap: "6px", "align-items": "center" }}
        >
          <input
            type="checkbox"
            checked={props.style.matchSpeech ?? false}
            disabled={props.disabled}
            onChange={(e) => props.onChange({ matchSpeech: e.currentTarget.checked })}
          />
          Match subtitles to the speech
        </label>
      </Show>
    </div>
  );
}
