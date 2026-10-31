/** The public REST error envelope; never stringify it as "[object Object]". */
export function apiErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) return error.message;
  if (typeof error !== "object" || error === null) return fallback;
  const envelope = error as { error?: { message?: unknown } };
  return typeof envelope.error?.message === "string" && envelope.error.message
    ? envelope.error.message
    : fallback;
}
