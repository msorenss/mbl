import type { components } from "./schema";

type BrowserCodec = components["schemas"]["BrowserCodec"];
type DeviceProfileOverlay = components["schemas"]["DeviceProfileOverlay"];

const mp4Codecs: ReadonlyArray<readonly [BrowserCodec, ...string[]]> = [
  ["hevc", 'video/mp4; codecs="hvc1.1.6.L120.90"', 'video/mp4; codecs="hev1.1.6.L120.90"'],
  ["av1", 'video/mp4; codecs="av01.0.05M.08"'],
  ["ac3", 'audio/mp4; codecs="ac-3"'],
  ["eac3", 'audio/mp4; codecs="ec-3"'],
];

// Baseline types used by hls.js to decide whether it can use MSE at all.
const hlsMseTypes = [
  'video/mp4; codecs="avc1.42E01E,mp4a.40.2"',
  'video/mp4; codecs="av01.0.01M.08"',
  'video/mp4; codecs="vp09.00.50.08"',
  'audio/mp4; codecs="mp4a.40.2"',
  'audio/mp4; codecs="fLaC"',
];

function browserProfile(ua: string): string {
  if (ua.includes("edg/")) return "edge";
  if (ua.includes("firefox/")) return "firefox";
  if (ua.includes("chrome/")) return "chrome";
  if (ua.includes("safari/")) return "safari";
  return "generic-hls";
}

export function getDeviceProfileInput(): DeviceProfileOverlay {
  if (typeof window === "undefined") return { name: "generic-hls" };

  const name = browserProfile(navigator.userAgent.toLowerCase());
  const video = document.createElement("video");
  const canPlay = (mime: string) => {
    try {
      return video.canPlayType(mime) !== "";
    } catch {
      return false;
    }
  };
  const mp4 = mp4Codecs.filter(([, ...mimes]) => mimes.some(canPlay)).map(([codec]) => codec);
  const webm: BrowserCodec[] = canPlay('video/webm; codecs="av01.0.05M.08"') ? ["av1"] : [];

  // Match hls.js's preference for Managed Media Source, then standard and prefixed MSE.
  const mediaWindow = window as Window & {
    ManagedMediaSource?: typeof MediaSource;
    WebKitMediaSource?: typeof MediaSource;
    WebKitSourceBuffer?: typeof SourceBuffer;
  };
  const mediaSource =
    mediaWindow.ManagedMediaSource ?? window.MediaSource ?? mediaWindow.WebKitMediaSource;
  const canAppend = (mime: string) => {
    try {
      return mediaSource?.isTypeSupported?.(mime) ?? false;
    } catch {
      return false;
    }
  };
  const sourceBuffer = (window.SourceBuffer ?? mediaWindow.WebKitSourceBuffer)?.prototype;
  const mseHls =
    (!sourceBuffer ||
      (typeof sourceBuffer.appendBuffer === "function" &&
        typeof sourceBuffer.remove === "function")) &&
    hlsMseTypes.some(canAppend);
  const nativeHls = canPlay("application/vnd.apple.mpegurl") || canPlay("application/x-mpegURL");
  const hls = mseHls
    ? mp4Codecs.filter(([, ...mimes]) => mimes.some(canAppend)).map(([codec]) => codec)
    : nativeHls
      ? mp4
      : [];

  return { name, browserCodecs: { mp4, webm, hls } };
}
