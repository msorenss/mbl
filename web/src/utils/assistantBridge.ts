import { createSignal } from "solid-js";

/**
 * Links the command palette (loaded at start) with the assistant chat (loaded later): the
 * chat says when the assistant can be used, and takes questions asked elsewhere.
 */
const [available, setAvailable] = createSignal(false);

export const assistantAvailable = available;
export const setAssistantAvailable = setAvailable;

let handler: ((text: string) => void) | undefined;
let waiting: string | undefined;

/** Opens the assistant chat with `text` as a new question. */
export function askAssistant(text: string): void {
  if (handler) handler(text);
  else waiting = text;
}

/** Registers the chat; a question asked before it loaded is delivered at once. */
export function onAssistantAsk(fn: (text: string) => void): () => void {
  handler = fn;
  if (waiting !== undefined) {
    const text = waiting;
    waiting = undefined;
    fn(text);
  }
  return () => {
    if (handler === fn) handler = undefined;
  };
}
