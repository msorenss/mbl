/**
 * The address of a subtitle file, asking the server to move the cues to where people speak when
 * `matchSpeech` is on. Kept apart so the player, Cast and tests build it the same way.
 */
export function subtitleUrl(url: string, matchSpeech: boolean | undefined): string {
  if (!matchSpeech) return url;
  return `${url}${url.includes("?") ? "&" : "?"}sync=true`;
}
