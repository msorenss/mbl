import { describe, expect, it } from "vitest";
import client, { getStoredToken, setStoredToken } from "./client";

describe("API client", () => {
  it("initializes client with base URL", () => {
    expect(client).toBeDefined();
  });

  it("handles token storage in localStorage", () => {
    setStoredToken("test_token_123");
    expect(getStoredToken()).toBe("test_token_123");

    setStoredToken(null);
    expect(getStoredToken()).toBeNull();
  });
});
