import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../../api/client";
import AiAgents from "./AiAgents";

const toast = vi.hoisted(() => vi.fn());
vi.mock("../../context/ToastContext", () => ({ useToast: () => ({ showToast: toast }) }));
let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
beforeEach(() => {
  vi.spyOn(client, "GET").mockImplementation(
    async (path) =>
      ({
        data:
          path === "/api/v1/mcp/status"
            ? undefined
            : path === "/api/v1/mcp/presets"
              ? [{ id: "read_only", name: "Read only", description: "Read", scopes: ["read"] }]
              : [],
        response: new Response(),
      }) as never,
  );
});
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  toast.mockClear();
});
async function wizard() {
  dispose = render(() => createComponent(AiAgents, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")]
    .find((b) => b.textContent?.includes("Connect New Agent"))
    ?.click();
  const input = document.getElementById("agentName") as HTMLInputElement;
  input.value = "Agent";
  input.dispatchEvent(new Event("input", { bubbles: true }));
}
function network(label: string) {
  return [...document.querySelectorAll("label")]
    .find((item) => item.textContent?.includes(label))
    ?.querySelector<HTMLInputElement>('input[type="checkbox"]');
}
const submit = () =>
  document
    .querySelector("form")
    ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
it("sends explicit network restrictions and preserves a minted token if snippets fail", async () => {
  const post = vi.spyOn(client, "POST").mockImplementation(async (path) => {
    if (path === "/api/v1/auth/tokens")
      return { data: { token: "mbl_new_secret" }, response: new Response() } as never;
    throw new Error("snippet server unavailable");
  });
  await wizard();
  network("Public internet")?.click();
  submit();
  await tick();
  await tick();
  expect(post).toHaveBeenCalledWith(
    "/api/v1/auth/tokens",
    expect.objectContaining({
      body: expect.objectContaining({ allowedNetworks: ["lan", "tailnet", "wan"], name: "Agent" }),
    }),
  );
  expect(
    [...document.querySelectorAll("input")].some((input) => input.value === "mbl_new_secret"),
  ).toBe(true);
  expect(toast).toHaveBeenCalledWith(
    expect.objectContaining({ title: "Token created", type: "warning" }),
  );
  expect(toast).not.toHaveBeenCalledWith(
    expect.objectContaining({ message: "Failed to create token" }),
  );
});
it("rejects an empty network selection before sending a token request", async () => {
  const post = vi.spyOn(client, "POST");
  await wizard();
  network("Local network")?.click();
  network("Tailscale network")?.click();
  submit();
  await tick();
  expect(post).not.toHaveBeenCalled();
  expect(toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: "Select at least one allowed network" }),
  );
});
it("shows failed revocation and leaves the token visible", async () => {
  vi.mocked(client.GET).mockImplementation(
    async (path) =>
      ({
        data:
          path === "/api/v1/mcp/status"
            ? undefined
            : path === "/api/v1/auth/tokens"
              ? [
                  {
                    id: 3,
                    name: "Existing agent",
                    scopes: ["read"],
                    allowedNetworks: ["lan"],
                    createdAt: 1,
                  },
                ]
              : [],
        response: new Response(),
      }) as never,
  );
  vi.spyOn(window, "confirm").mockReturnValue(true);
  vi.spyOn(client, "DELETE").mockResolvedValue({
    error: {},
    response: new Response(null, { status: 403 }),
  } as never);
  dispose = render(() => createComponent(AiAgents, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")].find((b) => b.textContent?.includes("Revoke"))?.click();
  await tick();
  expect(document.body.textContent).toContain("Existing agent");
  expect(toast).toHaveBeenCalledWith(
    expect.objectContaining({ message: "Failed to revoke token", type: "error" }),
  );
});
