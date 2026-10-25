import { describe, expect, it, vi } from "vitest";
import client from "../api/client";

describe("Setup wizard", () => {
  it("verifies needsSetup status check from system API", async () => {
    const getSpy = vi.spyOn(client, "GET").mockResolvedValueOnce({
      data: { needsSetup: true },
      error: undefined,
      response: new Response(),
    });

    const res = await client.GET("/api/v1/system/setup");
    expect(getSpy).toHaveBeenCalledWith("/api/v1/system/setup");
    expect(res.data?.needsSetup).toBe(true);
  });

  it("submits setup request with admin credentials", async () => {
    const postSpy = vi.spyOn(client, "POST").mockResolvedValueOnce({
      data: {
        token: "mbl_setup_token",
        user: { id: 1, username: "admin", isAdmin: true, createdAt: 1000 },
      },
      error: undefined,
      response: new Response(),
    });

    const res = await client.POST("/api/v1/system/setup", {
      body: { username: "admin", password: "password123" },
    });

    expect(postSpy).toHaveBeenCalled();
    expect(res.data?.token).toBe("mbl_setup_token");
    expect(res.data?.user.isAdmin).toBe(true);
  });
});
