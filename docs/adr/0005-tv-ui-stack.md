# 0005. TV client UI stack

- Status: Proposed (decided in M5.0)
- Date: 2026-10-23

## Context
The 10-foot client must run at 60 fps on a Raspberry Pi 5 with no desktop environment (KMS/DRM), play 4K HEVC HDR with hardware decode, and also run on Windows and Linux desktops. There are two candidates:
- **A. Slint (`linuxkms` backend) + libmpv:** native Rust and low RAM. Video and UI composite on separate DRM planes. It needs its own component library (`mbl-ui-kit`), generated from shared design tokens.
- **B. WPE WebKit/Cog kiosk running the web UI + mpv:** one UI codebase, but heavier on RAM/CPU and with weaker video-plane integration.

## Decision
Proposed: **A (Slint + libmpv)**. It will be confirmed or rejected by the M5.0 prototype on a Pi 5, which measures p99 frame time, RAM and cold start for both options using the same screens.

## Consequences
- If A: two UI implementations (web + Slint) share design tokens. More work, but the best performance on the Pi.
- If B: less code, but the performance budgets in PLAN §10.5 are at risk on Pi 4/5.
- This ADR will be updated with the prototype's measurements and status *Accepted*.
