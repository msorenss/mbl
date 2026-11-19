import { beforeAll, expect, it } from "vitest";
// @ts-expect-error -- plain ESM script without type declarations
import { extractUiStrings } from "../../scripts/i18n-extract.mjs";
import { loadLocaleTables, translations } from "./index";

beforeAll(() => loadLocaleTables());

import { lookupExtendedPhrase, NON_EN_LOCALES } from "./phraseEngine";

/** Names, code and developer messages that stay as they are in every language. */
const KEEP_ENGLISH = new Set([
  "*.ts.net",
  "-----BEGIN PRIVATE KEY-----",
  "Amazon Prime Video",
  "Apple TV+",
  "AsianWiki",
  "BiliBili",
  "Chromecast",
  "Crunchyroll",
  "Ctrl+K",
  "DRM,",
  "Dailymotion",
  "Disney+",
  "GitHub ↗",
  "Google Antigravity",
  "HBO Max",
  "HMAC).",
  "Hailo",
  "Hermes Agent",
  "Hulu",
  "IMDb",
  "Internet Archive",
  "Jellyfin",
  "Lidarr",
  "Max",
  "Media Box Lounge",
  "MyDramaList",
  "Mwmbl",
  "NVIDIA,",
  "Netflix",
  "OpenClaw",
  "Paramount+",
  "Peacock",
  "Plex",
  "Pluto TV",
  "Prime Video",
  "Radarr",
  "Rakuten Viki",
  "Sonarr",
  "Reddit",
  "Rotten Tomatoes",
  "SearXNG",
  "Single’s Inferno",
  "SkyShowtime",
  "Soompi",
  "Tailscale",
  "Tubi",
  "ch)",
  "Viaplay",
  "Vimeo",
  "WeTV",
  "Wikipedia",
  "X-MBL-Signature",
  "YouTube",
  "[metadata.tmdb]",
  "[redacted URL]",
  "admin@example.com",
  "mbl-node pair --server",
  "mbl-node run",
  "mbl-worker pair --server",
  "mbl.sidebarMode",
  "sha256=",
  "themoviedb.org/settings/api",
  "useAuth must be used within AuthProvider",
  "usePlayer must be used within a PlayerProvider",
  "useTheme must be used within a ThemeProvider",
  "useToast must be used within ToastProvider",
]);

/** Templates that are log lines or protocol values, not UI text. */
const isDeveloperTemplate = (t: string) =>
  /^(\[|%c|Bearer )/.test(t) || /^\{0\}$/.test(t) || t.startsWith("· {0}");

const primary = (phrase: string, loc: string): string | undefined => {
  const dict = (translations as unknown as Record<string, Record<string, string>>)[loc];
  if (phrase in dict) return dict[phrase];
  const en = translations.en as Record<string, string>;
  for (const [key, value] of Object.entries(en))
    if (value === phrase || value.toLowerCase() === phrase.toLowerCase()) return dict[key];
  return undefined;
};

it("translates every visible UI string completely in all 22 languages", () => {
  const { strings, templates } = extractUiStrings() as { strings: string[]; templates: string[] };
  const texts = [
    ...strings.filter((s) => !KEEP_ENGLISH.has(s)),
    // Fill placeholders with neutral values so the pattern table is exercised.
    ...templates
      .filter((t) => !isDeveloperTemplate(t))
      .map((t) => t.replace(/\{(\d+)\}/g, (_, n: string) => `X${n}`)),
  ];
  const missing: string[] = [];
  for (const text of texts) {
    const gaps = NON_EN_LOCALES.filter((loc) => !lookupExtendedPhrase(text, loc, primary));
    if (gaps.length) missing.push(`${text}  [${gaps.join(",")}]`);
  }
  expect(missing).toEqual([]);
}, 120_000);

it("never returns a half-translated sentence", () => {
  // Word-by-word composition is only used when every word is known.
  expect(lookupExtendedPhrase("Could not load the frobnicator", "sv", primary)).toBeUndefined();
  expect(lookupExtendedPhrase("Playback started", "sv", primary)).toBe("Uppspelning startad");
  expect(lookupExtendedPhrase("Certificate issued for mbl.example.com.", "sv", primary)).toBe(
    "Certifikat utfärdat för mbl.example.com.",
  );
});
