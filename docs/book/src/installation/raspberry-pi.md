# Raspberry Pi 5 Optimization & Setup

MBL is engineered as the premier media server for the **Raspberry Pi 5**.

## Hardware Prerequisites & Recommendations

1. **Power Supply:** Official Raspberry Pi 27W USB-C Power Supply (delivers 5V @ 5A with active PD negotiation).
2. **Cooling:** Active Cooler or Argon NEO 5 case to ensure sustained turbo clocks during high-bitrate remuxing.
3. **Storage:** NVMe SSD via Raspberry Pi M.2 HAT+ (drastically improves thumbnail seeking and metadata indexing).
4. **NPU Accelerator (Optional):** Hailo-8 or Hailo-10H M.2 AI module on `/dev/hailo0` for local multimodal scene search.

## Pi 5 Specific Optimizations

- **V4L2 M2M & DRM:** Zero-copy hardware accelerated H.264/HEVC decoding and display pipeline via Linux kernel V4L2 M2M drivers.
- **tmpfs Transcode Buffering:** Configured automatically by `mbl-server` to route live segments to `/dev/shm` (RAM), preserving SD card / NVMe lifespan.
- **Thermal & Voltage Guard:** Built-in telemetry samples `/sys/class/thermal/thermal_zone0/temp` and `vcgencmd get_throttled`. If throttling is detected, background transcode jobs and metadata deep analysis are dynamically paused.

## Pre-baked OS Image (`pi-gen`)

Download the official MBL OS flash image from GitHub Releases:
- Flash directly to an SD card or NVMe SSD using **Raspberry Pi Imager**.
- Pre-configured with zero-telemetry, Hailo PCIe overlays, and auto-expanding root filesystem.
