import createClient from "openapi-fetch";
import { browserLogs } from "../utils/browserLogs";
import type { paths } from "./schema";

const TOKEN_KEY = "mbl_token";

export function getStoredToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function setStoredToken(token: string | null): void {
  try {
    if (token) {
      localStorage.setItem(TOKEN_KEY, token);
    } else {
      localStorage.removeItem(TOKEN_KEY);
    }
  } catch {
    // Ignore storage errors (private browsing, etc.)
  }
}

export const client = createClient<paths>({
  baseUrl: "/",
});

const started = new WeakMap<Request, number>();

client.use({
  onRequest({ request }) {
    const token = getStoredToken();
    if (token && !request.headers.has("Authorization")) {
      request.headers.set("Authorization", `Bearer ${token}`);
    }
    started.set(request, performance.now());
    return request;
  },
  onResponse({ request, schemaPath, response }) {
    if (schemaPath === "/api/v1/system/logs") return;
    const start = started.get(request);
    const level = response.status >= 500 ? "error" : response.status >= 400 ? "warn" : "info";
    if (response.ok && schemaPath.endsWith("/progress")) return;
    browserLogs.record(level, "api", `${request.method} ${schemaPath} → ${response.status}`, {
      status: response.status,
      ...(start === undefined ? {} : { elapsedMs: Math.round(performance.now() - start) }),
    });
  },
  onError({ request, schemaPath }) {
    if (request.signal.aborted || schemaPath === "/api/v1/system/logs") return;
    const start = started.get(request);
    browserLogs.record("error", "api", `${request.method} ${schemaPath} → Network request failed`, {
      ...(start === undefined ? {} : { elapsedMs: Math.round(performance.now() - start) }),
    });
  },
});

export default client;
