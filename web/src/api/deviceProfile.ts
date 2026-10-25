/**
 * Dynamic browser capability detection for MBL playback negotiation.
 * Queries MediaSource and HTMLVideoElement to determine supported codecs and formats.
 */

export interface DetectedCapabilities {
  profileName: "chrome" | "firefox" | "safari" | "edge" | "generic-hls";
  canPlayHevc: boolean;
  canPlayAv1: boolean;
  canPlayVp9: boolean;
  canPlayH264: boolean;
  canPlayAc3: boolean;
  canPlayEac3: boolean;
  canPlayOpus: boolean;
  canPlayFlac: boolean;
  isHlsSupported: boolean;
  isNativeHlsSupported: boolean;
}

export function detectBrowserCapabilities(): DetectedCapabilities {
  if (typeof window === "undefined") {
    return {
      profileName: "generic-hls",
      canPlayHevc: false,
      canPlayAv1: false,
      canPlayVp9: false,
      canPlayH264: true,
      canPlayAc3: false,
      canPlayEac3: false,
      canPlayOpus: true,
      canPlayFlac: true,
      isHlsSupported: false,
      isNativeHlsSupported: false,
    };
  }

  const ua = navigator.userAgent.toLowerCase();
  let profileName: "chrome" | "firefox" | "safari" | "edge" | "generic-hls" = "chrome";

  if (ua.includes("edg/")) {
    profileName = "edge";
  } else if (ua.includes("firefox/")) {
    profileName = "firefox";
  } else if (ua.includes("safari/") && !ua.includes("chrome/")) {
    profileName = "safari";
  } else if (ua.includes("chrome/")) {
    profileName = "chrome";
  } else {
    profileName = "generic-hls";
  }

  const video = document.createElement("video");
  const isNativeHlsSupported =
    video.canPlayType("application/vnd.apple.mpegurl") !== "" ||
    video.canPlayType("application/x-mpegURL") !== "";

  const hasMediaSource = typeof window.MediaSource !== "undefined";
  const checkMse = (mime: string): boolean => {
    if (!hasMediaSource) return false;
    try {
      return MediaSource.isTypeSupported(mime);
    } catch {
      return false;
    }
  };

  const checkVideo = (mime: string): boolean => {
    try {
      return video.canPlayType(mime) !== "";
    } catch {
      return false;
    }
  };

  const canPlayH264 =
    checkMse('video/mp4; codecs="avc1.640028,mp4a.40.2"') ||
    checkVideo('video/mp4; codecs="avc1.640028"');

  const canPlayHevc =
    checkMse('video/mp4; codecs="hvc1.1.6.L120.90"') ||
    checkMse('video/mp4; codecs="hev1.1.6.L120.90"') ||
    checkVideo('video/mp4; codecs="hvc1.1.6.L120.90"');

  const canPlayAv1 =
    checkMse('video/mp4; codecs="av01.0.05M.08"') ||
    checkVideo('video/mp4; codecs="av01.0.05M.08"');

  const canPlayVp9 =
    checkMse('video/webm; codecs="vp9,opus"') || checkVideo('video/webm; codecs="vp9"');

  const canPlayAc3 = checkMse('audio/mp4; codecs="ac-3"') || checkVideo('audio/mp4; codecs="ac-3"');

  const canPlayEac3 =
    checkMse('audio/mp4; codecs="ec-3"') || checkVideo('audio/mp4; codecs="ec-3"');

  const canPlayOpus =
    checkMse('audio/webm; codecs="opus"') || checkVideo('audio/webm; codecs="opus"');

  const canPlayFlac = checkMse('audio/mp4; codecs="flac"') || checkVideo("audio/flac");

  return {
    profileName,
    canPlayHevc,
    canPlayAv1,
    canPlayVp9,
    canPlayH264,
    canPlayAc3,
    canPlayEac3,
    canPlayOpus,
    canPlayFlac,
    isHlsSupported: hasMediaSource || isNativeHlsSupported,
    isNativeHlsSupported,
  };
}

export function getPreferredProfile(): string {
  const caps = detectBrowserCapabilities();
  return caps.profileName;
}
