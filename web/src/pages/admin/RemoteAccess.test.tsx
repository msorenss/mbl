import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import client from "../../api/client";
import RemoteAccess from "./RemoteAccess";

let dispose: (() => void) | undefined;
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

beforeEach(() => {
  vi.spyOn(client, "GET").mockResolvedValue({
    data: {
      tailscale: {
        backendState: "Running",
        magicDnsName: "hallon.tail1234.ts.net.",
        tailnetIps: ["100.64.0.5"],
        peers: [],
      },
      isAdminLanOnly: true,
      isTailscaleAuthEnabled: false,
      wanUploadLimitMbps: 20,
      activeWanSessions: 0,
    },
    response: new Response(),
  } as never);
});
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

it("shows each diagnostic check and the remote address", async () => {
  const post = vi.spyOn(client, "POST").mockResolvedValue({
    data: {
      isCgnat: false,
      isDirectReachable: true,
      detectedWanIp: "203.0.113.7",
      detectedPublicIp: "203.0.113.7",
      recommendation: "portForwardWithTls",
      details: "The server is reachable from the internet over HTTPS.",
      checks: [
        { id: "publicIp", status: "ok", detail: "The internet sees this server as 203.0.113.7." },
        { id: "dns", status: "fail", detail: "mbl.example.com points to 198.51.100.1." },
        { id: "port", status: "warn", detail: "Could not connect." },
      ],
      publicHostnames: ["mbl.example.com"],
      httpsPort: 8443,
    },
    response: new Response(),
  } as never);
  dispose = render(() => createComponent(RemoteAccess, {}), document.body);
  await tick();
  expect(document.body.textContent).toContain("hallon.tail1234.ts.net");
  [...document.querySelectorAll("button")]
    .find((b) => b.textContent?.includes("Run diagnostics"))
    ?.click();
  await tick();
  await tick();
  expect(post).toHaveBeenCalledWith("/api/v1/system/reachability-test", { body: {} });
  const text = document.body.textContent ?? "";
  expect(text).toContain("Reachable from the internet");
  expect(text).toContain("Public address: OK");
  expect(text).toContain("DNS: Problem");
  expect(text).toContain("HTTPS port: Check");
  expect(text).toContain("https://mbl.example.com:8443");
  expect(text).not.toContain('"isCgnat"');
});

it("reports a failed diagnostic", async () => {
  vi.spyOn(client, "POST").mockResolvedValue({
    error: { error: { code: "internal", message: "boom" } },
    response: new Response(null, { status: 500 }),
  } as never);
  dispose = render(() => createComponent(RemoteAccess, {}), document.body);
  await tick();
  [...document.querySelectorAll("button")]
    .find((b) => b.textContent?.includes("Run diagnostics"))
    ?.click();
  await tick();
  await tick();
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(
    "Could not run diagnostics.",
  );
});
