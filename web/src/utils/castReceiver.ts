import { CAPABILITY_NAMESPACE } from "./castProfile";

export const TELEMETRY_NAMESPACE = "urn:x-cast:com.mbl.telemetry";
export interface ReceiverContext {
  canDisplayType: (
    mime: string,
    codecs?: string,
    width?: number,
    height?: number,
    fps?: number,
  ) => boolean;
  getDeviceCapabilities: () => { display_supported?: boolean };
  addCustomMessageListener: (
    namespace: string,
    listener: (event: { data: unknown; senderId: string }) => void,
  ) => void;
  sendCustomMessage: (namespace: string, senderId: string | undefined, data: unknown) => void;
}
const modes = [
  [1280, 720, 30, 41],
  [1280, 720, 60, 41],
  [1920, 1080, 30, 41],
  [1920, 1080, 60, 42],
  [1920, 1080, 60, 51],
  [3840, 2160, 30, 51],
  [3840, 2160, 60, 52],
];

export function capabilityReport(context: ReceiverContext, nonce: string) {
  const display = context.getDeviceCapabilities().display_supported;
  const supports = (mime: string, codecs?: string, w?: number, h?: number, fps?: number) => {
    try {
      return context.canDisplayType(mime, codecs, w, h, fps) === true;
    } catch {
      return false;
    }
  };
  const supportedModes =
    display === true
      ? modes
          .filter(([w, h, fps, level]) => {
            const hex = level.toString(16).padStart(2, "0");
            // Probe every profile accepted by the server catalogue, with explicit geometry/fps.
            return ["42e0", "4d40", "6400"].every((profile) =>
              supports("video/mp4", `avc1.${profile}${hex}`, w, h, fps),
            );
          })
          .map(([width, height, fps, avcLevel]) => ({
            width,
            height,
            fps,
            avcLevel,
            sdr8Bit: true,
          }))
      : [];
  return {
    schemaVersion: 1,
    revision: 1,
    nonce,
    // Unknown display capability is not a positive audio-only identification.
    video: display !== false,
    mp4: supportedModes.length > 0,
    // CAF implements HLS/fMP4; codec support is checked above, physical validation still required.
    hlsFmp4: supportedModes.length > 0,
    aacStereo: supports("audio/mp4", "mp4a.40.2"),
    mp3Stereo: supports("audio/mpeg"),
    modes: supportedModes,
  };
}

export function installCapabilityChannel(context: ReceiverContext) {
  let lastRequest = -Infinity;
  context.addCustomMessageListener(CAPABILITY_NAMESPACE, ({ data, senderId }) => {
    let request: unknown = data;
    if (typeof data === "string") {
      if (data.length > 256) return;
      try {
        request = JSON.parse(data);
      } catch {
        return;
      }
    }
    if (!request || typeof request !== "object" || Array.isArray(request)) return;
    const r = request as Record<string, unknown>;
    if (
      Object.keys(r).some((key) => !["schemaVersion", "type", "nonce"].includes(key)) ||
      r.schemaVersion !== 1 ||
      r.type !== "probe" ||
      typeof r.nonce !== "string" ||
      !/^[a-f0-9-]{36}$/i.test(r.nonce) ||
      typeof senderId !== "string" ||
      !senderId
    )
      return;
    if (Date.now() - lastRequest < 250) return;
    lastRequest = Date.now();
    try {
      context.sendCustomMessage(CAPABILITY_NAMESPACE, senderId, capabilityReport(context, r.nonce));
    } catch {
      /* Disconnected sender: no retry or unbounded queue. */
    }
  });
}

export function receiverTelemetry(event: string, value: unknown, video?: HTMLVideoElement | null) {
  if (
    !["ERROR", "BUFFERING", "BITRATE_CHANGED", "PLAYING", "PAUSE", "PLAYER_LOAD_COMPLETE"].includes(
      event,
    )
  )
    return null;
  const input = value && typeof value === "object" ? (value as Record<string, unknown>) : {};
  const output: Record<string, unknown> = { schemaVersion: 1, revision: 1, event };
  for (const key of ["detailedErrorCode", "totalBitrate"]) {
    const n = input[key];
    if (typeof n === "number" && Number.isFinite(n) && n >= 0) output[key] = n;
  }
  if (typeof input.isBuffering === "boolean") output.isBuffering = input.isBuffering;
  if (video && video.videoWidth > 0 && video.videoHeight > 0) {
    output.width = video.videoWidth;
    output.height = video.videoHeight;
  }
  return output;
}

/** Companion comments relayed by the sender, drawn over the video on the TV. */
export const COMPANION_NAMESPACE = "urn:x-cast:com.mbl.companion";
const COMPANION_VISIBLE_MS = 15000;

/** A comment from the sender, or null when the message is not one (bounded, text only). */
export function companionComment(data: unknown): { author: string; text: string } | null {
  const value = typeof data === "string" ? safeJson(data) : data;
  if (!value || typeof value !== "object") return null;
  const r = value as Record<string, unknown>;
  if (r.type !== "comment" || typeof r.text !== "string" || !r.text.trim()) return null;
  const author = typeof r.author === "string" ? r.author.slice(0, 40) : "";
  return { author, text: r.text.trim().slice(0, 500) };
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

/** Shows relayed comments top right, at most three, each for 15 seconds. */
export function installCompanionOverlay(context: ReceiverContext, doc: Document = document) {
  const box = doc.createElement("div");
  box.setAttribute("aria-live", "polite");
  box.style.cssText =
    "position:fixed;top:5%;right:4%;max-width:38%;display:flex;flex-direction:column;" +
    "gap:12px;align-items:flex-end;z-index:10;pointer-events:none;font:28px sans-serif";
  doc.body.appendChild(box);
  context.addCustomMessageListener(COMPANION_NAMESPACE, ({ data }) => {
    const comment = companionComment(data);
    if (!comment) return;
    const bubble = doc.createElement("p");
    bubble.style.cssText =
      "margin:0;padding:14px 20px;border-radius:16px;background:rgba(10,12,16,0.82);" +
      "color:#fff;line-height:1.35;box-shadow:0 4px 24px rgba(0,0,0,0.4)";
    if (comment.author) {
      const name = doc.createElement("strong");
      name.textContent = `${comment.author}: `;
      bubble.appendChild(name);
    }
    bubble.appendChild(doc.createTextNode(comment.text));
    box.appendChild(bubble);
    while (box.childElementCount > 3) box.firstElementChild?.remove();
    setTimeout(() => bubble.remove(), COMPANION_VISIBLE_MS);
  });
}
