# Dark Matter — local-library browser acceptance

Tested with computer use on the isolated preview at http://127.0.0.1:18421
on 2026-10-30 after the user added a library. Installed port 8420 was untouched.
No application source, library configuration or file permissions were changed.
Playback updated the disposable preview account's viewing history normally.

## Verified

- Scanner produced Show 1 → Season 2 → Episode 3, with a readable local MKV,
  H.264 1920×960, E-AC-3 5.1, and embedded subtitle tracks.
- Opening `/item/3` directly and pressing Play started actual video through
  HLS/hls.js. Duration 3609.523 seconds; time advanced, readyState 4, media error
  null. Visible screenshot confirms an actual video frame. Audible sound was not
  assessed by the automation.
- Ten-second forward seek, pause and play worked.
- Stats reported transcode, software backend, chrome profile, LAN and no restart.
  Reasons were unsupported Matroska container and E-AC-3 audio. FFmpeg paused
  ahead of playback with 46–48 seconds buffered.
- Stop saved 96,761 ms in watch_state. After reloading the detail page, Resume
  appeared, prompted at 1:36, and successfully played onward past 1:59.
- Delivery state: paused at 124.98472 seconds, readyState 4, no media error.

Evidence: [paused video](assets/2026-10-30-dark-matter/paused.png),
[DOM metrics](assets/2026-10-30-dark-matter/paused.json).

## Defects and incomplete acceptance

1. **Series navigation blocks normal playback (high priority).** Home's Play Now
   sends Show 1 directly to playback despite it having no media part. The show
   detail page labels its season as an episode, offers Play on Season 2, and has
   no season navigation link. Season Play returns `not found`. `Home.tsx` sends
   the featured item without kind resolution; `ItemDetail.tsx` renders every
   child with the same Play action. Direct episode navigation was necessary to
   continue this acceptance test.
2. **Detail page stays stale after player close.** Closing at 1:36 saved position,
   but pressing the same detail page's Play again started at zero without a
   resume prompt. Reloading displayed Resume and restored 1:36 correctly.
   `ItemDetail.tsx` reloads on item-ID changes, not on player close. Saved history
   works; the mounted view does not refresh it.
3. **Swedish subtitle was not delivered within the observed test.** Selecting
   `swe` created a showing TextTrack, but it retained zero cues and its HTML track
   stayed in LOADING. The preview cache held a zero-byte `1_38.1.vtt.tmp` after
   more than 30 seconds. No HTTP failure was observed, so this establishes slow
   or stuck extraction, not a confirmed permanent failure. Subtitle conversion
   awaits FFmpeg completion without a timeout in `mbl-stream/src/subtitles.rs`;
   investigate extraction latency, process cleanup and explicit loading/error UX.
   Auto-hiding controls also made the long subtitle menu difficult to operate
   while playing; selecting after pause worked.
4. **Many dropped frames in this browser.** Stats showed 1,171 and later 1,316
   dropped frames during roughly the first 90 seconds despite a substantial
   buffer. Investigate browser/software decoding and Pi load before judging
   smoothness. Advancing playback and no media error do not prove smooth video.

No fix, commit, release or new automated-test run is claimed for this audit.
The paused episode tab is left open for the user.
