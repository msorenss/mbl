import createClient from "openapi-fetch";
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

client.use({
  onRequest({ request }) {
    const token = getStoredToken();
    if (token && !request.headers.has("Authorization")) {
      request.headers.set("Authorization", `Bearer ${token}`);
    }
    return request;
  },
});

export default client;
