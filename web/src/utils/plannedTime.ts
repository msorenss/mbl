/** "Fri 20:00"-style time in the viewer's language. */
export function formatPlannedTime(ms: number, locale?: string): string {
  return new Date(ms).toLocaleString(locale, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}
