# Hardware Acceleration & Transcoding

MBL features the modular `mbl-hwaccel` hardware probing engine, dynamically discovering available GPUs, NPUs, and media encoders on the host machine.

## Supported Acceleration Frameworks

| Platform | Decoders | Encoders | HDR Tone Mapping | Driver Requirement |
|---|---|---|---|---|
| **Linux (Intel)** | VAAPI / QSV (H.264, HEVC, AV1) | QSV / VAAPI | OpenCL / VAAPI VPP | `intel-media-va-driver-non-free` |
| **Linux (NVIDIA)** | NVDEC (H.264, HEVC, AV1) | NVENC | CUDA / nv-codec-headers | NVIDIA proprietary driver >= 525 |
| **Raspberry Pi 5** | V4L2 M2M (H.264, HEVC) | DRM / Software x264 | DRM tonemap | Kernel 6.6+ default V4L2 drivers |
| **Windows** | D3D11VA, QSV, NVDEC, AMF | NVENC, QSV, AMF | Direct3D 11 Video Process | Vendor GPU drivers |

## Hardware Capabilities Self-Test

MBL includes an automated self-test framework executing real micro-transcode tests on device nodes:

```bash
# Run self-test via CLI:
mbl-server hw test

# Or execute via MCP:
# Tool: run_hw_selftest { "dry_run": false }
```

Results are persisted in the `hw_capabilities` table and visualized in the Web Admin dashboard (`/admin/hardware`).

## ABR Ladder & Adaptive Bitrate Streaming

When transcoding is necessary, MBL dynamically selects an Adaptive Bitrate (ABR) profile according to client device profile, network bandwidth, and hardware encoder constraints:
- `1080p high`: 1080p @ 12 Mbps
- `1080p med`: 1080p @ 6 Mbps
- `720p`: 720p @ 3.5 Mbps
- `480p`: 480p @ 1.5 Mbps
- `360p`: 360p @ 800 kbps
