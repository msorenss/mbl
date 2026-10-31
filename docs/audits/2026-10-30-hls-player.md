# WebDrama batch 5 — HLS player and resume

Implemented locally and tested on ARM64/Linux on 2026-10-30. Preview:
http://127.0.0.1:18421/livetv with disposable account `admin` / `admin-password`.
The installed service on port 8420 and its data have not been updated.

## Try it

1. Open **Live TV → Virtual & Plugins → WebDrama → Search the web**.
2. Check a public source, then choose **Play with resume**. Start playback with
   the video's own controls. Preparation validates the playlist and decodes a
   real sample before a supported HLS session opens.
3. Pause or close; reopen from **Recently played online** to restore position.
   Merely loading a player does not create watch history.

The public test source used for acceptance was
https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8.

## Actual browser acceptance

Computer use exercised the GUI, without scripting playback state changes.
The browser loaded a local authenticated relay manifest and played H.264/AAC
VOD (duration 634.584 seconds). Native HLS was used in this browser.

| Action | Observed result |
| --- | --- |
| Play and seek with native controls | Playback continued at 82.482441 seconds, readyState 4, no media error. |
| Pause | Position 101.801857 seconds, readyState 4, no media error. |
| Close | Video removed; personal history displayed 1:41. |
| Reopen history entry | New checked session restored exactly 101.801857 seconds. |
| Play again | Time advanced to 101.971191 seconds, unpaused, no media error. |
| Delivery state | Paused at 111.059246 seconds, readyState 4, no media error. |

Raw read-only DOM observations: [paused](assets/2026-10-30-hls/paused.json),
[restored](assets/2026-10-30-hls/resumed.json),
[delivery](assets/2026-10-30-hls/delivery.json).
Screenshot capture timed out repeatedly; no screenshot is claimed for this batch.
The test browser remains open. Native HLS is browser-verified; hls.js fallback is
covered by a unit regression and still needs browser acceptance.

## Verification

- Full workspace nextest: **440 passed, 0 skipped**. A final narrow check rejecting
  extra quoted MAP attributes was added afterward; all **24 online service tests**
  and service Clippy passed again.
- Full web suite: **184 passed** across 36 files, including seven player/history
  tests. Earlier parallel tests under Rust-linker load hit two existing 5-second
  timeouts; the full single-worker run with a 15-second deadline passed.
- TypeScript/Vite and initial JS budget: **48.79 KB gzip**.
- Affected-crate Clippy/all targets with `-D warnings`, formatting and diff checks
  pass. Biome: no errors, five existing warnings and four infos.
- OpenAPI and TypeScript regenerated; MCP snapshots pass. **78 tools**, scoped
  counts **33/38/78**, parity **156/156** with 56 excluded paths.

## Scope and remaining work

New migration 0013 stores private history independently of the discovery cache:
200 entries retained per account, latest 20 shown. Changed title/series/episode
identity resets resume; completion starts at zero next time. History is separate
from local-library Continue Watching and Up Next.

HLS is limited to public, unencrypted, closed VOD playlists: one rendition,
at most three playlist levels, optional single initialization map, and bounded
segments. Live/low-latency, encryption/DRM, byte ranges and external audio/subtitle
tracks are rejected. Every fetch validates public addresses and pinned DNS;
cookies/credentials/referrers are not forwarded upstream. Sessions check current
login identity and profile, expire, and are released on Close. Resource transfers
are capped at 16 MiB each, with bounded concurrency and timeouts.

Real-series discovery and full-episode acceptance, site adapters, subtitle and
Cast support, transcoding, automatic next episode and ML title matching remain
unfinished. This successful test stream does not prove Singles Inferno or any
particular streaming service works. LIVE-01 remains partially implemented.
See the [user guide](../book/src/online-discovery.md) for exact limits.

Source changes remain local. No commit, push, tag or release was created.
