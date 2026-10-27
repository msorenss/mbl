// EPG programmes and recordings use Unix milliseconds throughout the API.
export const GUIDE_SLOT_MS = 30 * 60 * 1000;
export function guideWindow(nowMs = Date.now()) {
  const windowStart = Math.floor(nowMs / GUIDE_SLOT_MS) * GUIDE_SLOT_MS;
  return { windowStart, windowEnd: windowStart + 4 * GUIDE_SLOT_MS };
}
