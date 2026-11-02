import { describe, expect, it } from "vitest";
import { classifyStreamingProvider } from "./streamingProviders";

describe("classifyStreamingProvider", () => {
  it("classifies Bilibili as free streaming platform", () => {
    const info = classifyStreamingProvider(
      "https://www.bilibili.tv/en/video/4787780269906432",
      {},
      "sv",
    );
    expect(info.siteName).toBe("BiliBili");
    expect(info.isStreaming).toBe(true);
    expect(info.paymentTier).toBe("free");
    expect(info.streamingBadge.text).toContain("Streaming tillgänglig");
    expect(info.paymentBadge.text).toContain("Gratis streaming");
    expect(info.streamingBadge.variant).toBe("success");
    expect(info.paymentBadge.variant).toBe("success");
    expect(info.openUrl).toBe("https://www.bilibili.tv/en/video/4787780269906432");
  });

  it("classifies Netflix as paid subscription streaming", () => {
    const info = classifyStreamingProvider("https://www.netflix.com/title/81405633", {}, "sv");
    expect(info.siteName).toBe("Netflix");
    expect(info.isStreaming).toBe(true);
    expect(info.paymentTier).toBe("paid");
    expect(info.paymentBadge.text).toContain("Kräver prenumeration");
    expect(info.paymentBadge.variant).toBe("warning");
  });

  it("classifies Viki as freemium", () => {
    const info = classifyStreamingProvider("https://www.viki.com/tv/37735c-vincenzo", {}, "sv");
    expect(info.siteName).toBe("Rakuten Viki");
    expect(info.isStreaming).toBe(true);
    expect(info.paymentTier).toBe("freemium");
    expect(info.paymentBadge.text).toContain("Gratis med reklam / Viki Pass");
    expect(info.paymentBadge.variant).toBe("info");
  });

  it("classifies Wikipedia and IMDb as non-streaming info sites", () => {
    const wiki = classifyStreamingProvider(
      "https://en.wikipedia.org/wiki/Vincenzo_(TV_series)",
      {},
      "sv",
    );
    expect(wiki.siteName).toBe("Wikipedia");
    expect(wiki.isStreaming).toBe(false);
    expect(wiki.paymentTier).toBe("info");
    expect(wiki.streamingBadge.text).toContain("ej streaming");

    const imdb = classifyStreamingProvider("https://www.imdb.com/title/tt13433802/", {}, "en");
    expect(imdb.siteName).toBe("IMDb");
    expect(imdb.isStreaming).toBe(false);
    expect(imdb.paymentTier).toBe("info");
    expect(imdb.streamingBadge.text).toContain("no video");
  });

  it("marks directly playable MBL streams with accent badge", () => {
    const info = classifyStreamingProvider(
      "https://example.org/stream.m3u8",
      { availability: "external_player", mediaUrl: "https://example.org/stream.m3u8" },
      "sv",
    );
    expect(info.directPlayable).toBe(true);
    expect(info.streamingBadge.text).toBe("▶ Spelbar direkt i MBL");
    expect(info.streamingBadge.variant).toBe("accent");
  });

  it("handles unknown domain with video detection heuristics", () => {
    const info = classifyStreamingProvider(
      "https://unknown-anime.site/watch/ep1",
      { hasVideo: true, kind: "video" },
      "sv",
    );
    expect(info.isStreaming).toBe(true);
    expect(info.streamingBadge.text).toContain("Streaming upptäckt");
    expect(info.openUrl).toBe("https://unknown-anime.site/watch/ep1");
  });
});
