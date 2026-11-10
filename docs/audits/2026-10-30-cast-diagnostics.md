# Cast diagnostics and physical receiver evidence

Implemented for the ongoing Bedroom TV freeze investigation on preview 18421.
This is observability work; no successful TV playback is claimed.

The player now exposes localized **Stats → Cast diagnostics → Copy Cast log**.
A local bounded timeline records chooser, preparation, server status, HLS format,
LOAD/acknowledgement, receiver/media state, commands, heartbeat, disconnection
and SDK error scalar fields. It does not change playback behavior. Redaction
covers URL queries/credentials/fragments and HLS/live capability session paths.
The first 40 and latest 160 events survive overflow; new attempt/reload resets it.
No automatic upload, server storage or new REST/MCP endpoint is introduced.

Computer use verified that the panel renders in Swedish, receives real server
negotiation events and reports successful clipboard copy. During verification,
a real user-selected Bedroom TV attempt was recorded. Masked evidence:
[receiver timeline](assets/2026-10-30-cast/receiver-attempt.json).

Observed elapsed milliseconds from that attempt:

| Time | Observation |
| --- | --- |
| 4,632 | TV chooser accepted. |
| 6,982 | Server returned HTTP 200, Chromecast transcode. |
| 7,030 | LOAD sent with fMP4 audio/video, autoplay true, position 357.866, 44 subtitle tracks. |
| 7,286 | SDK acknowledged LOAD. |
| 9,103 | Receiver reported media loaded. |
| 9,114 | BUFFERING. |
| 13,622 | PLAYING at position 357 (integer diagnostic sample). |
| 55,323 | Media unloaded; last reported position still 357. |
| 55,337 | Media listener received alive=false, IDLE, no idle reason. |
| 65,540 | SDK connection lost; local session cleared. |

No Play command was recorded in this attempt. These observations establish
that LOAD acceptance and PLAYING are insufficient proof of advancing playback.
They do not establish why the receiver unloaded, or whether the final disconnect
was user initiated. Compare future attempts with media request/server timing,
segment/init consistency, seek behavior and the subtitle/track configuration.

## User follow-up: audio continues while picture is frozen

The user initially reported continuing audio despite the frozen picture, then
clarified that the audio comes from the **local computer**, not the TV. There is
no confirmed receiver audio playback. The Cast connection ends when local audio
stops. This is physical observation, not inferred from the sender position. A second [user-supplied timeline](assets/2026-10-30-cast/receiver-audio-continues.json)
records server HTTP 200, fMP4 LOAD acknowledgement and PLAYING at 16.533 s.
Heartbeats remain at 376.775 seconds through 49.086 s. At 58.983–58.995 s,
media unloads and becomes IDLE with alive=false and no idle reason. The last
heartbeat at 59.091 s still reports sdkConnected=true. This log ends before a
transport disconnect, so it cannot establish the exact disconnect time or cause.
No Play/pause/seek command is present in this trace.

The symptom is therefore a frozen TV picture with continuing **local** audio,
followed by media unload. Do not interpret this as advancing audio on the TV. Receiver video decoding, HLS timestamps/init/segments and resume seek
need correlation with server diagnostics. No root cause or playback fix is
claimed from this evidence alone.

## Handoff and audio-channel fixes

User clarified that continuing audio was on the **local computer**. Code review
found that handoff paused only before the chooser and subsequently stopped HLS
loading, leaving buffered media attached. Pending Cast did not guard the Play
shortcut. Google Cast now pauses and releases the local HLS/direct source on
acceptance, blocks late local play/playing events, and ignores Play while the
chooser/negotiation is pending. Native Remote Playback keeps its source because
it uses the local element as the transport. Canceled chooser still resumes
local playback. Added local prepare/handoff/released/pause/heartbeat samples to
the diagnostic trail, with no source URLs or credentials.

A separate real compatibility defect was confirmed: the generic/Chromecast
profile advertises a two-channel limit, but HLS `AudioOp::Encode` used source
channels capped at six rather than the device limit. The inspected generated
fMP4 stream contained six-channel AAC from the six-channel E-AC-3 source.
The pipeline now applies the profile limit, keeping the existing six-channel
encoder ceiling. A real FFmpeg integration test probes the resulting AAC for
stereo, explicit mono with audio offset, and explicit six-channel support.
No conclusion is drawn yet about the cause of the TV video freeze.

The [complete earlier timeline](assets/2026-10-30-cast/receiver-local-audio-full.json)
was preserved from the GUI before reload; it extends the pasted report through
SDK disconnection at 69.188 s and session clearing at 69.192 s.

## Physical retest of local release

The next user-selected Bedroom TV attempt used the updated web player while the
server build was still finishing. The [recorded timeline](assets/2026-10-30-cast/receiver-local-released.json)
shows local.handoff at 11.618 s and local.released at 11.746 s. Local heartbeats
remain paused=true, position=0 and readyState=0. The user confirms the TV image
still freezes. Media unloads around 60.79 s; SDK disconnects at 70.992 s.
Thus the local source release is verified separately from receiver playback.
The continuing local audio issue and TV video freeze must not be conflated.

Verification of the changes: 197 web tests across 39 files; all three real
FFmpeg playback_tracks tests; service-lib Clippy with warnings denied; debug
server build; TypeScript/Vite and 49.08 KB initial JS gzip; fmt/diff checks pass.
The preview server on 18421 was then restarted with the channel fix and returns
HTTP 200. Production 8420 is untouched. A physical stereo-server retest is
pending; successful TV playback is not claimed.

## Stereo-server retest: receiver still freezes

The user confirms the TV picture still freezes with the updated server and
supplied [attempt 2](assets/2026-10-30-cast/receiver-stereo-attempt.json), starting
2026-10-30T16:25:42.138Z. Server negotiation returns HTTP 200, LOAD is accepted,
local media is released/paused, and receiver reports PLAYING at 24.590 s.
Position remains 596.578 seconds. Media unloads at ~66.35 s; SDK disconnects at
76.643 s. No idle reason is reported.

The displayed E-AC-3/six-channel/Matroska reasons concern the ORIGINAL source,
not a probe of delivered output. The [retained session init probe](assets/2026-10-30-cast/stereo-output-probe.json)
confirms H.264/avc1 at 1920x960 and AAC/mp4a at 48 kHz with TWO channels. Channel
negotiation is therefore fixed; it does not resolve the physical video freeze.

A separate HLS timeline defect was found by comparing the [advertised list](assets/2026-10-30-cast/stereo-advertised.m3u8)
with the [FFmpeg-generated list](assets/2026-10-30-cast/stereo-generated.m3u8):
upfront segments are declared 3/3/6/... seconds with TARGETDURATION 6, whereas
generated segments include 7.341, 7.465 and 9.259 seconds with TARGETDURATION 9.
The retained directory was inspected AFTER receiver disconnect and local player
reinitialization, which can restart FFmpeg and rewrite the generated playlist.
These artifacts prove a timeline-generation defect, not exactly which bytes
were delivered to the TV before the freeze. The service unconditionally builds
an estimated fast-start VOD list; the FFmpeg writer uses six-second targets and
copied video cuts only at source keyframes. Even encoded video uses six-second
forced keyframes without implementing the advertised first two three-second
segments. A generic forced-video-encode change alone is not an adequate fix.

[HLS RFC 8216 section 4.3.3.1](https://datatracker.ietf.org/doc/html/rfc8216#section-4.3.3.1)
requires segment durations rounded to integer seconds to fit TARGETDURATION,
and notes that longer segments can cause stalls. Next: implement accurate
segment boundaries/timestamps and stable seek/restart/init handling; capture
receiver media requests to correlate the physical freeze. Do not paper over
this by only raising TARGETDURATION or replacing conversion reasons with a
false claim of successful playback. Receiver freeze remains unresolved.

## 2026-11-08 Cast attempt stops during server preparation

The phase 4 baseline passed physical playback on Bedroom TV with three
synthetic sources covering Direct Play, HLS with AAC, and HLS with E-AC-3
converted to AAC stereo.

In the new sender trace, attempt 4 starts at 14:55:48 UTC. The Cast SDK reaches
`SESSION_STARTED` and the sender requests playback for item 192 at 26.376 s.
The request is still pending when the player is disposed 15.367 s later.
Disposal aborts the request, which appears as `load.failed` with abort code 20.
The trace contains neither an HTTP response nor a receiver `LOAD` request.

The earlier frozen-picture traces reached HTTP 200 and receiver playback.
For this attempt, correlate the server's playback-start log and source format
for item 192 first. The sender trace alone cannot identify the server build
or distinguish slow media preparation from an interrupted connection.

The user identified the source as *Dark Matter: How to Build the Box* on the
Windows v1.3.1 server over HTTPS. The item is Matroska with H.264 and
six-channel E-AC-3. A subsequent local playback on that server advanced to
38 seconds with software transcoding and zero FFmpeg restarts. This verifies
local playback in that session. The physical Cast attempt still needs its own
receiver trace.

## 2026-11-08 Cast plays, then loses its HLS session on Windows

The combined diagnostics for item 192 show that the receiver was playing when
the server evicted an HLS session for exceeding the cache quota at 19:12:20 UTC.
Segment requests began returning 404 at 19:12:26 and a progress request returned
404 at 19:12:28. The receiver still played from its buffer and entered PAUSED
at 19:12:31. It reported `IDLE / ERROR` at 19:12:36.

Windows had no implementation for suspending the FFmpeg child when it ran ahead
of the receiver. The session manager now suspends and resumes the child's
threads on Windows. The receiver error also preserves the last loaded position.

Earlier in the attempt, switching subtitles reloaded HLS at 141.759 s after the
server prepared playback at 124.706 s. The replacement stream now starts at
the prepared position. The exported log also records the selected subtitle
index and the result of paused progress requests. Repeat the same subtitle
and pause sequence on Bedroom TV to verify A/V sync and resume.

## 2026-11-10 Windows capture confirms mismatched segment durations

The advertised media playlist listed four 6-second fragments and a target
duration of 10. FFmpeg listed the corresponding durations as 7.341, 7.465,
3.796 and 5.797 seconds, with a target duration of 8 at capture time. A
separate attempt for item 192 at 914.527 seconds accepted LOAD but stayed
unloaded until the sender timed out.

Cast remux sessions that request playback readiness now serve the generated
EVENT playlist. FFmpeg writes from source zero and prepares the indexed resume
point before LOAD. The server preserves the fragments it advertises while the
session remains active. Physical retesting is needed to check whether this
resolves the receiver's startup failure.

## Broader diagnostic trail — follow-up after Cast

User proposed a shared trail for all functions after Cast works. This is a
follow-up design direction, not implemented in this batch: shared timestamp,
correlation ID, component, action, outcome and duration across UI/API/service/
FFmpeg/worker/scanner; a searchable admin view; secret redaction; bounded
retention/export; explicit distinctions between queued, accepted and completed
operations. Reuse existing audit/logging facilities and document access control
before adding broad telemetry. Do not claim that a client-only Cast log provides
this complete system trail.
