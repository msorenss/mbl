/**
 * Streaming provider and payment tier classification for online web sources.
 */

export type PaymentTier = "paid" | "freemium" | "free" | "info" | "unknown";

export interface ProviderBadge {
  text: string;
  variant: "success" | "accent" | "warning" | "info" | "neutral";
}

export interface StreamingProviderInfo {
  siteName: string;
  hostname: string;
  isStreaming: boolean;
  paymentTier: PaymentTier;
  streamingBadge: ProviderBadge;
  paymentBadge: ProviderBadge;
  directPlayable: boolean;
  openUrl?: string;
}

interface KnownProvider {
  name: string;
  isStreaming: boolean;
  tier: PaymentTier;
  streamingTextEn: string;
  streamingTextSv: string;
  paymentTextEn: string;
  paymentTextSv: string;
}

const PROVIDERS: Record<string, KnownProvider> = {
  // Subscription / Paid streaming
  "netflix.com": {
    name: "Netflix",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "disneyplus.com": {
    name: "Disney+",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "max.com": {
    name: "Max",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "hbomax.com": {
    name: "HBO Max",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "primevideo.com": {
    name: "Prime Video",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "amazon.com": {
    name: "Amazon Prime Video",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription / Purchase",
    paymentTextSv: "🔒 Prenumeration / Hyrfilm",
  },
  "tv.apple.com": {
    name: "Apple TV+",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "hulu.com": {
    name: "Hulu",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "paramountplus.com": {
    name: "Paramount+",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "viaplay.se": {
    name: "Viaplay",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "skyshowtime.com": {
    name: "SkyShowtime",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },
  "peacocktv.com": {
    name: "Peacock",
    isStreaming: true,
    tier: "paid",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🔒 Subscription required",
    paymentTextSv: "🔒 Kräver prenumeration (Betaltjänst)",
  },

  // Freemium / Ad-supported / Passes
  "viki.com": {
    name: "Rakuten Viki",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / Viki Pass",
    paymentTextSv: "✨ Gratis med reklam / Viki Pass",
  },
  "iq.com": {
    name: "iQIYI",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / VIP",
    paymentTextSv: "✨ Gratis med reklam / VIP",
  },
  "iqiyi.com": {
    name: "iQIYI",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / VIP",
    paymentTextSv: "✨ Gratis med reklam / VIP",
  },
  "wetv.vip": {
    name: "WeTV",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / VIP",
    paymentTextSv: "✨ Gratis med reklam / VIP",
  },
  "crunchyroll.com": {
    name: "Crunchyroll",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / Premium",
    paymentTextSv: "✨ Gratis med reklam / Premium",
  },
  "tv4play.se": {
    name: "TV4 Play",
    isStreaming: true,
    tier: "freemium",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "✨ Free with ads / Plus",
    paymentTextSv: "✨ Gratis med reklam / Plus",
  },

  // Free / Community / Public Service streaming
  "bilibili.tv": {
    name: "BiliBili",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available (UGC)",
    streamingTextSv: "📺 Streaming tillgänglig (Video/UGC)",
    paymentTextEn: "🎁 Free streaming",
    paymentTextSv: "🎁 Gratis streaming",
  },
  "bilibili.com": {
    name: "BiliBili",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available (UGC)",
    streamingTextSv: "📺 Streaming tillgänglig (Video/UGC)",
    paymentTextEn: "🎁 Free streaming",
    paymentTextSv: "🎁 Gratis streaming",
  },
  "youtube.com": {
    name: "YouTube",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🎁 Free streaming (ads)",
    paymentTextSv: "🎁 Gratis streaming (med reklam)",
  },
  "youtu.be": {
    name: "YouTube",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🎁 Free streaming (ads)",
    paymentTextSv: "🎁 Gratis streaming (med reklam)",
  },
  "dailymotion.com": {
    name: "Dailymotion",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🎁 Free streaming",
    paymentTextSv: "🎁 Gratis streaming",
  },
  "vimeo.com": {
    name: "Vimeo",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Video hosting",
    streamingTextSv: "📺 Videodelning",
    paymentTextEn: "🎁 Free video",
    paymentTextSv: "🎁 Gratis video",
  },
  "svtplay.se": {
    name: "SVT Play",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🎁 Free (Public Service)",
    paymentTextSv: "🎁 Gratis (Public Service)",
  },
  "pluto.tv": {
    name: "Pluto TV",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Free live & on-demand",
    streamingTextSv: "📺 Gratis live & play",
    paymentTextEn: "🎁 Free streaming (FAST)",
    paymentTextSv: "🎁 Gratis streaming (Annonser)",
  },
  "tubitv.com": {
    name: "Tubi",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Streaming available",
    streamingTextSv: "📺 Streaming tillgänglig",
    paymentTextEn: "🎁 Free streaming (ads)",
    paymentTextSv: "🎁 Gratis streaming (Annonser)",
  },
  "archive.org": {
    name: "Internet Archive",
    isStreaming: true,
    tier: "free",
    streamingTextEn: "📺 Public domain media",
    streamingTextSv: "📺 Allmän egendom / Media",
    paymentTextEn: "🎁 Free (Public Domain)",
    paymentTextSv: "🎁 Gratis (Public Domain)",
  },

  // Databases & Information (Non-streaming)
  "wikipedia.org": {
    name: "Wikipedia",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Encyclopedia (no video)",
    streamingTextSv: "📄 Uppslagsverk (ej streaming)",
    paymentTextEn: "ℹ️ Free information",
    paymentTextSv: "ℹ️ Gratis fakta & info",
  },
  "imdb.com": {
    name: "IMDb",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Movie database (no video)",
    streamingTextSv: "📄 Filmdatabas (ej streaming)",
    paymentTextEn: "ℹ️ Free ratings & info",
    paymentTextSv: "ℹ️ Gratis filmfakta & betyg",
  },
  "mydramalist.com": {
    name: "MyDramaList",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Drama database (no video)",
    streamingTextSv: "📄 Dramadatabas (ej streaming)",
    paymentTextEn: "ℹ️ Free drama info & ratings",
    paymentTextSv: "ℹ️ Gratis dramainfo & betyg",
  },
  "themoviedb.org": {
    name: "TMDB",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Metadata database",
    streamingTextSv: "📄 Metadatabas (ej streaming)",
    paymentTextEn: "ℹ️ Free movie & TV data",
    paymentTextSv: "ℹ️ Gratis metadata",
  },
  "rottentomatoes.com": {
    name: "Rotten Tomatoes",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Review aggregator",
    streamingTextSv: "📄 Recensionsdatabas",
    paymentTextEn: "ℹ️ Free reviews",
    paymentTextSv: "ℹ️ Gratis recensioner",
  },
  "asianwiki.com": {
    name: "AsianWiki",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 Drama wiki (no video)",
    streamingTextSv: "📄 Dramawiki (ej streaming)",
    paymentTextEn: "ℹ️ Free wiki info",
    paymentTextSv: "ℹ️ Gratis wiki-information",
  },
  "soompi.com": {
    name: "Soompi",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 News & articles (no video)",
    streamingTextSv: "📄 Nyheter & artiklar",
    paymentTextEn: "ℹ️ Free news",
    paymentTextSv: "ℹ️ Gratis artiklar",
  },
  "allkpop.com": {
    name: "allkpop",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "📄 News & articles (no video)",
    streamingTextSv: "📄 Nyheter & artiklar",
    paymentTextEn: "ℹ️ Free news",
    paymentTextSv: "ℹ️ Gratis artiklar",
  },
  "reddit.com": {
    name: "Reddit",
    isStreaming: false,
    tier: "info",
    streamingTextEn: "💬 Discussion forum",
    streamingTextSv: "💬 Diskussionsforum",
    paymentTextEn: "ℹ️ Community discussions",
    paymentTextSv: "ℹ️ Användardiskussioner",
  },
};

function matchKnownProvider(hostname: string): KnownProvider | undefined {
  const lower = hostname.toLowerCase();
  for (const [domain, provider] of Object.entries(PROVIDERS)) {
    if (lower === domain || lower.endsWith(`.${domain}`)) {
      return provider;
    }
  }
  return undefined;
}

export function classifyStreamingProvider(
  rawUrl: string,
  source?: {
    availability?: string;
    mediaUrl?: string | null;
    hasVideo?: boolean | null;
    kind?: string;
    title?: string;
  },
  locale = "sv",
): StreamingProviderInfo {
  let hostname = "";
  let openUrl: string | undefined;
  try {
    const parsed = new URL(rawUrl);
    if (["http:", "https:"].includes(parsed.protocol) && !parsed.username && !parsed.password) {
      hostname = parsed.hostname;
      openUrl = parsed.href;
    }
  } catch {
    // Leave hostname empty for invalid/non-web URLs
  }

  const isSv = locale === "sv";
  const directPlayable =
    (source?.availability === "media_detected" && !!source.mediaUrl) ||
    source?.availability === "external_player";

  const known = hostname ? matchKnownProvider(hostname) : undefined;

  if (known) {
    const isStreaming = known.isStreaming;
    const streamingText = directPlayable
      ? isSv
        ? "▶ Spelbar direkt i MBL"
        : "▶ Direct play in MBL"
      : isSv
        ? known.streamingTextSv
        : known.streamingTextEn;

    const streamingVariant = directPlayable ? "accent" : isStreaming ? "success" : "neutral";

    const paymentVariant =
      known.tier === "paid"
        ? "warning"
        : known.tier === "freemium"
          ? "info"
          : known.tier === "free"
            ? "success"
            : "neutral";

    return {
      siteName: known.name,
      hostname,
      isStreaming,
      paymentTier: known.tier,
      streamingBadge: {
        text: streamingText,
        variant: streamingVariant,
      },
      paymentBadge: {
        text: isSv ? known.paymentTextSv : known.paymentTextEn,
        variant: paymentVariant,
      },
      directPlayable,
      openUrl,
    };
  }

  // Heuristic classification for unknown domains
  const videoDetected =
    source?.hasVideo === true ||
    source?.kind === "video" ||
    source?.kind === "episode" ||
    directPlayable;

  const isRestricted = source?.availability === "restricted";

  const isStreaming = videoDetected || isRestricted;

  const streamingText = directPlayable
    ? isSv
      ? "▶ Spelbar direkt i MBL"
      : "▶ Direct play in MBL"
    : isStreaming
      ? isSv
        ? "📺 Streaming upptäckt"
        : "📺 Streaming detected"
      : isSv
        ? "📄 Webbsida (ej streaming)"
        : "📄 Webpage (no streaming)";

  const paymentTier: PaymentTier = isRestricted ? "paid" : "unknown";
  const paymentText = isRestricted
    ? isSv
      ? "🔒 Kräver inloggning / Prenumeration"
      : "🔒 Sign-in or subscription required"
    : isSv
      ? "🌐 Extern webbplats"
      : "🌐 External website";

  return {
    siteName: hostname || "Webbkälla",
    hostname,
    isStreaming,
    paymentTier,
    streamingBadge: {
      text: streamingText,
      variant: directPlayable ? "accent" : isStreaming ? "success" : "neutral",
    },
    paymentBadge: {
      text: paymentText,
      variant: isRestricted ? "warning" : "neutral",
    },
    directPlayable,
    openUrl,
  };
}
