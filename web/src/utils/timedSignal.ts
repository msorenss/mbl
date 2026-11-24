import { type Accessor, createSignal, onCleanup } from "solid-js";

/**
 * A message signal that clears itself `ms` after it was last set, so notices and errors over
 * the player do not stay on screen forever. Setting it again restarts the time; setting an
 * empty value clears it at once.
 */
export function createTimedSignal<T extends string | null>(
  empty: T,
  ms: number,
): [Accessor<T>, (value: T) => void] {
  const [value, setValue] = createSignal<T>(empty);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const set = (next: T) => {
    clearTimeout(timer);
    setValue(() => next);
    if (next) timer = setTimeout(() => setValue(() => empty), ms);
  };
  onCleanup(() => clearTimeout(timer));
  return [value, set];
}
