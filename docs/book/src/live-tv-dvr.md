# Live TV, Electronic Program Guide (EPG) & DVR

MBL provides a comprehensive Live TV platform supporting over-the-air tuners, IPTV playlists, streaming XMLTV guides, and intelligent DVR scheduling.

## Tuner & Source Support

- **HDHomeRun:** Native UDP broadcast discovery (`255.255.255.255:65001`), tuner lock inspection (`/discover.json`), and automatic channel lineup extraction (`/lineup.json`).
- **M3U / M3U8 Playlists:** Import custom IPTV playlists with automatic extraction of `tvg-id`, `tvg-name`, `tvg-logo`, `tvg-chno`, and stream URIs.
- **XMLTV Streaming Import:** Chunked parsing of massive XMLTV guide files via `quick-xml`, indexing titles, categories, star ratings, and `xmltv_ns` episode numbering without exhausting system RAM.

## 2-Axis Virtual EPG TV Guide

The web and TV clients render a responsive, GPU-accelerated 2-axis grid (`/livetv`):
- Horizontal axis: Real-time scrolling timeline with current playhead indicator.
- Vertical axis: Tuner channels with broadcast channel numbers and logos.
- Sub-5ms search across future program airings (`/api/v1/livetv/epg/search`).

## Smart DVR & Series Rules

1. **One-Time Recordings:** Schedule individual broadcasts with configurable start and end padding margins (e.g., 2 minutes before, 5 minutes after).
2. **Series Rules:** Automatically record every episode of a TV show matching criteria such as `new_only` (skips repeats) or specific season bounds.
3. **Tuner Allocation & Concurrency Guard:** The live engine enforces strict physical tuner limits (`tuner_count`). If a scheduled DVR recording conflicts with an active live stream, configurable conflict priority determines tuner reassignment.

## Continuous 24/7 Virtual Channels

Create custom 24/7 linear TV channels from your local media libraries:
- Zero-CPU mathematical playhead calculation (`(now_ms - epoch_ms) % total_duration_ms`).
- Automatically generates synthetic EPG program blocks for TV clients.
- Seamless looping of curated collections (e.g., "90s Sitcoms 24/7", "Classic Anime").
