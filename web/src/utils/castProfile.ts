/** Sender side of the versioned MBL receiver capability protocol. */
export const CAPABILITY_NAMESPACE = "urn:x-cast:com.mbl.capabilities";
export type CastProfileSelection = {
  profile: string;
  maxHeight: number;
  source: "receiver-probe" | "fallback";
};
export const SAFE_CAST_PROFILE: CastProfileSelection = {
  profile: "cast-safe-720",
  maxHeight: 720,
  source: "fallback",
};
// Each entry requires every combination, not independent width/fps maxima.
const candidates = [
  { profile: "google-tv-streamer", level: 52, modes: [[3840, 2160, 60]] },
  {
    profile: "google-tv-4k",
    level: 51,
    modes: [
      [3840, 2160, 30],
      [1920, 1080, 60],
    ],
  },
  { profile: "chromecast-gen3", level: 42, modes: [[1920, 1080, 60]] },
  {
    profile: "chromecast-legacy",
    level: 41,
    modes: [
      [1920, 1080, 30],
      [1280, 720, 60],
    ],
  },
  { profile: "nest-hub", level: 41, modes: [[1280, 720, 60]] },
  { profile: "cast-safe-720", level: 41, modes: [[1280, 720, 30]] },
];

export function resolveCastProfile(value: unknown, nonce: string): CastProfileSelection | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const report = value as Record<string, unknown>;
  const fields = [
    "schemaVersion",
    "nonce",
    "revision",
    "video",
    "hlsFmp4",
    "mp4",
    "mp3Stereo",
    "aacStereo",
    "modes",
  ];
  if (
    Object.keys(report).some((key) => !fields.includes(key)) ||
    report.schemaVersion !== 1 ||
    report.nonce !== nonce ||
    report.revision !== 1 ||
    typeof report.video !== "boolean"
  )
    return null;
  if (!report.video) throw new Error("This Cast receiver does not support video.");
  if (
    report.hlsFmp4 !== true ||
    report.mp4 !== true ||
    report.mp3Stereo !== true ||
    report.aacStereo !== true ||
    !Array.isArray(report.modes) ||
    report.modes.length > 16
  )
    return null;
  const modes = report.modes as unknown[];
  const candidate = candidates.find((entry) =>
    entry.modes.every(([width, height, fps]) =>
      modes.some((mode) => {
        if (!mode || typeof mode !== "object" || Array.isArray(mode)) return false;
        const m = mode as Record<string, unknown>;
        return (
          Object.keys(m).every((key) =>
            ["width", "height", "fps", "avcLevel", "sdr8Bit"].includes(key),
          ) &&
          m.width === width &&
          m.height === height &&
          m.fps === fps &&
          m.avcLevel === entry.level &&
          m.sdr8Bit === true
        );
      }),
    ),
  );
  return candidate
    ? { profile: candidate.profile, maxHeight: candidate.modes[0][1], source: "receiver-probe" }
    : null;
}

export interface CapabilitySession {
  addMessageListener?: (
    namespace: string,
    listener: (namespace: string, data: string) => void,
  ) => void;
  removeMessageListener?: (
    namespace: string,
    listener: (namespace: string, data: string) => void,
  ) => void;
  sendMessage?: (namespace: string, data: unknown) => Promise<unknown>;
}

/** No persistent cache: every negotiation is tied to this session and nonce. */
export function queryCastProfile(session: CapabilitySession): Promise<CastProfileSelection> {
  if (!session.addMessageListener || !session.removeMessageListener || !session.sendMessage)
    return Promise.resolve({ ...SAFE_CAST_PROFILE });
  return new Promise((resolve, reject) => {
    const nonce = crypto.randomUUID();
    let done = false;
    const finish = (selection: CastProfileSelection, error?: unknown) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      try {
        session.removeMessageListener?.(CAPABILITY_NAMESPACE, listener);
      } catch {
        /* SDK teardown */
      }
      if (error) reject(error);
      else resolve(selection);
    };
    const listener = (namespace: string, data: string) => {
      if (namespace !== CAPABILITY_NAMESPACE || typeof data !== "string" || data.length > 8192)
        return;
      let parsed: unknown;
      try {
        parsed = JSON.parse(data);
      } catch {
        return;
      }
      try {
        const selection = resolveCastProfile(parsed, nonce);
        if (selection) finish(selection);
      } catch (error) {
        finish(SAFE_CAST_PROFILE, error);
      }
    };
    const timer = setTimeout(() => finish({ ...SAFE_CAST_PROFILE }), 3000);
    try {
      session.addMessageListener?.(CAPABILITY_NAMESPACE, listener);
      session
        .sendMessage?.(CAPABILITY_NAMESPACE, { schemaVersion: 1, type: "probe", nonce })
        .catch(() => finish({ ...SAFE_CAST_PROFILE }));
    } catch {
      finish({ ...SAFE_CAST_PROFILE });
    }
  });
}
