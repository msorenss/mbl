import { CAPABILITY_NAMESPACE } from "./utils/castProfile";
import {
  installCapabilityChannel,
  type ReceiverContext,
  receiverTelemetry,
  TELEMETRY_NAMESPACE,
} from "./utils/castReceiver";

type Framework = {
  CastReceiverContext: {
    getInstance: () => ReceiverContext & {
      getPlayerManager: () => {
        addEventListener: (event: string, listener: (event: unknown) => void) => void;
      };
      start: (options: unknown) => void;
    };
  };
  events: { EventType: Record<string, string> };
  system: { MessageType: { JSON: string } };
};
// This is a separate receiver entry point; no sender, API client, storage or auth imports.
const framework = (globalThis as unknown as { cast?: { framework?: Framework } }).cast?.framework;
if (framework) {
  const context = framework.CastReceiverContext.getInstance();
  installCapabilityChannel(context);
  const player = context.getPlayerManager();
  const lastSent = new Map<string, number>();
  for (const name of [
    "ERROR",
    "BUFFERING",
    "BITRATE_CHANGED",
    "PLAYING",
    "PAUSE",
    "PLAYER_LOAD_COMPLETE",
  ]) {
    const type = framework.events.EventType[name];
    if (!type) continue;
    player.addEventListener(type, (event) => {
      if (Date.now() - (lastSent.get(name) ?? -Infinity) < 1000) return;
      lastSent.set(name, Date.now());
      const video =
        document.querySelector("video") ??
        document.querySelector("cast-media-player")?.shadowRoot?.querySelector("video");
      const data = receiverTelemetry(name, event, video);
      if (data) {
        try {
          context.sendCustomMessage(TELEMETRY_NAMESPACE, undefined, data);
        } catch {
          /* No connected sender. */
        }
      }
    });
  }
  context.start({
    customNamespaces: {
      [CAPABILITY_NAMESPACE]: framework.system.MessageType.JSON,
      [TELEMETRY_NAMESPACE]: framework.system.MessageType.JSON,
    },
  });
} else {
  document.body.textContent =
    "MBL Cast receiver: Google Cast runtime unavailable. Launch this application from MBL on a registered Cast device.";
}
