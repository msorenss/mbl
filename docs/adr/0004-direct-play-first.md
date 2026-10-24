# 0004. Direct play first

- Status: Accepted
- Date: 2026-10-23

## Context
The reference platform, the Raspberry Pi 5, has no hardware video encoder and only decodes HEVC in hardware. A software 1080p H.264 encode is barely real-time on it. GPU machines (NVIDIA/Intel) can transcode freely, but they are optional.

## Decision
The playback decision engine prefers, in order:
1. direct play
2. direct stream (remux)
3. audio-only transcode
4. an optimized version
5. a hardware video transcode (local or worker)
6. a software transcode within a capacity limit
7. refusing with a clear reason.

Subtitles are delivered as sidecars or rendered client-side whenever possible to avoid burn-in. Every non-direct decision records human-readable reasons.

## Consequences
- The Pi 5 can serve many concurrent streams because remuxing costs almost no CPU.
- Accurate device profiles are essential, so they need extensive tests.
- Optimized versions and transcode workers become the answer for remote or incompatible clients on Pi servers.
- Users see *why* something transcodes (prediction chip, stats overlay, MCP `explain_session`).
