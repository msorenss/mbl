interface BrowserLogEntry {
  timestampMs: number;
  level: "info" | "warn" | "error";
  target: string;
  message: string;
  details: Record<string, number>;
}

const entries: BrowserLogEntry[] = [];
const listeners = new Set<() => void>();

export const browserLogs = {
  record(
    level: BrowserLogEntry["level"],
    target: string,
    message: string,
    details: BrowserLogEntry["details"] = {},
  ) {
    entries.push({ timestampMs: Date.now(), level, target, message, details });
    if (entries.length > 400) entries.shift();
    for (const listener of listeners) listener();
  },
  snapshot: () => entries.slice().reverse(),
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
};

globalThis.addEventListener?.("online", () =>
  browserLogs.record("info", "network", "Browser online"),
);
globalThis.addEventListener?.("offline", () =>
  browserLogs.record("warn", "network", "Browser offline"),
);
