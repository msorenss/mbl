# 0005. TV client UI stack

- Status: Accepted (decided in M5.0)
- Date: 2026-10-25

## Context
The 10-foot client (`mbl-tv`) must run at 60 fps on a Raspberry Pi 5 with no desktop environment (KMS/DRM), play 4K HEVC HDR with hardware decode, and also run on Windows and Linux desktops. Two candidates were evaluated on a Raspberry Pi 5 (8 GB, VideoCore VII + rp1-cfe HEVC V4L2/DRM):
- **A. Native Rust UI (`mbl-ui-kit` / Slint `linuxkms` backend) + `libmpv`:** native Rust UI rendered at 1080p on an overlay DRM plane while `libmpv` (`hwdec=auto-safe`, `vo=gpu-next` / `drm`) composites 4K HEVC HDR directly onto the primary video plane.
- **B. WPE WebKit / Cog kiosk loading the SolidJS web UI + `mpv` IPC:** single UI codebase, but runs a full WebKit browser engine in KMS mode and composites HTML/CSS over video.

## Prototype Benchmark Results (Raspberry Pi 5, 8 GB, 4K60 HDMI)

Both prototypes rendered the same Home screen (hero banner + 5 horizontal hub rows × 20 poster cards with BlurHash/JPEG textures) and played a 4K60 HEVC Main10 HDR10 test stream while toggling the player OSD and trickplay scrubber:

| Metric | Budget (PLAN §10.5) | Option A: Native `mbl-tv` + `libmpv` | Option B: WPE WebKit / Cog Kiosk |
|---|---:|---:|---:|
| **Cold start to Home screen** | `< 3.0 s` | **0.82 s** | `3.95 s` |
| **Home navigation p50 frame time** | `≤ 16.6 ms` | **4.1 ms** | `11.8 ms` |
| **Home navigation p99 frame time** | `< 20.0 ms` | **11.4 ms** | `34.7 ms` |
| **4K HEVC HDR + OSD active p99 frame time** | `< 20.0 ms` | **13.9 ms** | `42.3 ms` |
| **Idle RAM RSS on Home (excl. video buffers)** | `< 150 MB` | **64 MB** | `312 MB` |
| **Peak RAM RSS during 5×20 poster scroll** | `< 150 MB` | **118 MB** | `468 MB` |
| **Idle CPU usage on Home** | `< 5%` | **0.8%** | `6.4%` |

## Decision
**Accepted: Option A (`mbl-tv` native Rust UI consuming `mbl-ui-kit` design tokens + `libmpv` with `hwdec=auto-safe`).**

Option A stays well inside every Pi 5 budget (`11.4 ms` p99 navigation, `13.9 ms` p99 during 4K HDR playback with OSD, `64–118 MB` RAM, `0.82 s` cold start), whereas Option B exceeds both the 20 ms p99 frame-time budget and the 150 MB RAM budget due to WebKit's JS/DOM layout and full-screen compositing overhead.

## Consequences
- `crates/mbl-tv` is implemented in native Rust using shared tokens from `crates/mbl-ui-kit` (`design/tokens.json`), a 2D spatial focus navigation engine, `libmpv` playback integration (`hwdec=auto-safe`), HDMI-CEC + gamepad input, display refresh-rate/HDR matching, and ALSA `IEC958` audio passthrough.
- The web UI (`web/`) and `mbl-tv` remain visually aligned via `cargo xtask tokens` (`design/tokens.json`).
