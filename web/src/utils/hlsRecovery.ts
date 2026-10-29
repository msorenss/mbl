import type { ErrorData } from "hls.js";

// Let hls.js handle one short HTTP retry; application recovery below owns the outer limit.
export const hlsLoadPolicy = () => ({
  default: {
    maxTimeToFirstByteMs: 20000,
    maxLoadTimeMs: 25000,
    timeoutRetry: null,
    errorRetry: { maxNumRetry: 1, retryDelayMs: 1000, maxRetryDelayMs: 1000 },
  },
});

export type HlsFailure = "expired" | "access" | "network" | "media" | "unsupported";

export function createHlsRecovery(options: {
  active: () => boolean;
  stop: () => void;
  retry: (reloadManifest: boolean) => void;
  notice: (message: string) => void;
  fail: (reason: HlsFailure) => void;
}) {
  let attempts = 0;
  let stopped = false;
  let retryTimer: number | undefined;
  let deadline: number | undefined;
  const clearTimers = () => {
    window.clearTimeout(retryTimer);
    window.clearTimeout(deadline);
    retryTimer = deadline = undefined;
  };
  const fail = (reason: HlsFailure) => {
    if (stopped || !options.active()) return;
    stopped = true;
    clearTimers();
    options.notice("");
    options.stop();
    options.fail(reason);
  };
  return {
    error(data: ErrorData) {
      if (stopped || !options.active() || !data.fatal) return;
      if (data.type !== "networkError") {
        fail(data.type === "mediaError" ? "media" : "unsupported");
        return;
      }
      const status = data.response?.code ?? 0;
      if (status === 404 || status === 410) return fail("expired");
      if (status === 401 || status === 403) return fail("access");
      // Permanent client errors (including rate limiting) need an explicit user retry.
      if (status >= 400 && status < 500 && status !== 408) return fail("network");
      if (retryTimer !== undefined) return;
      if (attempts >= 2) return fail("network");
      attempts++;
      options.stop();
      options.notice(`Connection interrupted. Retrying (${attempts}/2)…`);
      if (deadline === undefined) deadline = window.setTimeout(() => fail("network"), 30000);
      const reloadManifest =
        data.details === "manifestLoadError" || data.details === "manifestLoadTimeOut";
      retryTimer = window.setTimeout(
        () => {
          retryTimer = undefined;
          if (stopped || !options.active()) return;
          try {
            options.retry(reloadManifest);
          } catch {
            fail("network");
          }
        },
        attempts === 1 ? 1000 : 3000,
      );
    },
    // Real media progress (playback or a buffered fragment) ends the deadline, never a playlist/playing event.
    progress() {
      if (stopped || !options.active() || retryTimer !== undefined) return;
      window.clearTimeout(deadline);
      deadline = undefined;
      options.notice("");
    },
    dispose() {
      stopped = true;
      clearTimers();
      options.notice("");
    },
  };
}
