import { afterEach, expect, it, vi } from "vitest";
import { CAPABILITY_NAMESPACE } from "./utils/castProfile";
import { TELEMETRY_NAMESPACE } from "./utils/castReceiver";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
  document.body.innerHTML = "";
});
it("boots CAF namespaces before start and exports only bounded diagnostic fields", async () => {
  const events = new Map<string, (event: unknown) => void>();
  const context = {
    addCustomMessageListener: vi.fn(),
    sendCustomMessage: vi.fn(),
    start: vi.fn(),
    getPlayerManager: () => ({
      addEventListener: (name: string, listener: (event: unknown) => void) =>
        events.set(name, listener),
    }),
  };
  vi.stubGlobal("cast", {
    framework: {
      CastReceiverContext: { getInstance: () => context },
      events: { EventType: { ERROR: "error" } },
      system: { MessageType: { JSON: "JSON" } },
    },
  });
  await import("./castReceiver");
  expect(context.start).toHaveBeenCalledWith({
    customNamespaces: { [CAPABILITY_NAMESPACE]: "JSON", [TELEMETRY_NAMESPACE]: "JSON" },
  });
  expect(context.addCustomMessageListener.mock.invocationCallOrder[0]).toBeLessThan(
    context.start.mock.invocationCallOrder[0],
  );
  events.get("error")?.({
    detailedErrorCode: 301,
    url: "https://private?token=secret",
    error: { message: "secret" },
  });
  expect(context.sendCustomMessage).toHaveBeenCalledWith(TELEMETRY_NAMESPACE, undefined, {
    schemaVersion: 1,
    revision: 1,
    event: "ERROR",
    detailedErrorCode: 301,
  });
});
it("shows a useful status when opened outside a Cast device", async () => {
  vi.stubGlobal("cast", undefined);
  await import("./castReceiver");
  expect(document.body.textContent).toContain("Google Cast runtime unavailable");
});
