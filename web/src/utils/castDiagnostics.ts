export interface CastDiagnosticEvent {
  at: string;
  elapsedMs: number;
  stage: string;
  details: Record<string, string | number | boolean | null>;
}

function safeText(value: string): string {
  return value
    .replace(/https?:\/\/[^\s"<>]+/g, (raw) => {
      try {
        const url = new URL(raw);
        return `${url.origin}${url.pathname}${url.search ? "?[redacted]" : ""}`;
      } catch {
        return "[redacted URL]";
      }
    })
    .replace(/\/api\/v1\/stream\/[^\s"<>]+/g, (raw) => {
      const url = new URL(raw, "http://redaction.invalid");
      const path = url.pathname.replace(/(\/stream\/(?:hls|live)\/)[^/]+/, "$1[session]");
      return `${path}${url.search ? "?[redacted]" : ""}`;
    })
    .replace(/\bBearer\s+\S+/gi, "Bearer [redacted]")
    .replace(/\b(token|password|authorization|cookie)\s*[:=]\s*[^\s,;]+/gi, "$1=[redacted]")
    .slice(0, 512);
}

class CastDiagnostics {
  private events: CastDiagnosticEvent[] = [];
  private history: CastDiagnosticEvent[] = [];
  private startedAt = Date.now();
  private attempt = 0;
  private listeners = new Set<() => void>();

  start() {
    this.events = [];
    this.startedAt = Date.now();
    this.attempt++;
    this.record("attempt.start", { attempt: this.attempt });
  }

  record(stage: string, details: Record<string, unknown> = {}) {
    const clean: CastDiagnosticEvent["details"] = {};
    // Explicit scalar fields only: never traverse SDK objects, requests or credentials.
    for (const [key, value] of Object.entries(details).slice(0, 24)) {
      if (/token|password|authorization|cookie/i.test(key)) continue;
      if (typeof value === "string") clean[key] = safeText(value);
      else if (typeof value === "boolean" || value === null) clean[key] = value;
      else if (typeof value === "number") clean[key] = Number.isFinite(value) ? value : null;
    }
    const event: CastDiagnosticEvent = {
      at: new Date().toISOString(),
      elapsedMs: Date.now() - this.startedAt,
      stage,
      details: clean,
    };
    this.events.push(event);
    this.history.push(event);
    if (this.history.length > 200) this.history.shift();
    try {
      console.info(`[MBL Cast] ${stage}`, clean);
    } catch {
      // Resilience against restricted console environments
    }
    // Keep the initial negotiation and the latest receiver events.
    if (this.events.length > 200) this.events.splice(40, this.events.length - 200);
    for (const listener of this.listeners) listener();
  }

  getEvents(): CastDiagnosticEvent[] {
    return [...this.events];
  }

  getHistory(): CastDiagnosticEvent[] {
    return [...this.history];
  }

  error(stage: string, error: unknown) {
    const err = error && typeof error === "object" ? (error as Record<string, unknown>) : {};
    this.record(stage, {
      message:
        error instanceof Error ? error.message : typeof error === "string" ? error : undefined,
      code: err.code,
      description: err.description,
      detailedErrorCode: err.detailedErrorCode,
    });
  }

  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  export() {
    return JSON.stringify({ version: 1, attempt: this.attempt, events: this.events }, null, 2);
  }
}

export const castDiagnostics = new CastDiagnostics();
