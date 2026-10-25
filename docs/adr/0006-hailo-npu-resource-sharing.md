# 0006. Hailo NPU Media & AI Resource Sharing

- Status: Accepted
- Date: 2026-10-25

## Context
Users deploying MBL on a Raspberry Pi 5 (or x86 mini-PC) equipped with a Hailo-8, Hailo-8L, or Hailo-10H AI accelerator rarely dedicate the NPU solely to their media server. In typical home-lab setups, the NPU is shared with latency-sensitive or long-running co-tenants—for example:
- Real-time NVR object detection (e.g. **Frigate**, Scrypted, or `rpicam-apps`).
- Local LLM / GenAI inference servers (e.g. **hailo-ollama**, Open-WebUI backends).
- Home Assistant voice pipelines (wake-word, STT/TTS) or custom GStreamer/Tappas pipelines.

If MBL opens `/dev/h1x-*` (Hailo-10H) or `/dev/hailo*` (Hailo-8/8L) with default exclusive parameters, HailoRT locks the physical device and either MBL or the user's other services fail with `HAILO_OUT_OF_PHYSICAL_DEVICES` / `Device is busy`. Even when multiplexed, unthrottled batch jobs (like transcribing a full season of episodes) can starve real-time camera detectors or voice assistants.

## Decision
MBL treats any detected Hailo NPU as a **shared, cooperative resource** by default:

1. **Shared VDevice & Multi-Process Service Support:**
   - MBL initializes HailoRT `VDevice` instances with a configurable `group_id` (defaulting to `"SHARED"`, or reading `HAILO_VDEVICE_GROUP_ID` / `HAILO_MULTI_PROCESS_SERVICE`).
   - Supports both kernel-level `VDevice` sharing (`hailo1x_pci` on Hailo-10H) and the `hailort_service` daemon (`hailo_pci` on Hailo-8/8L) so multiple containers/processes can multiplex network groups without exclusive locks.
   - If the device is temporarily locked by another process that did not enable sharing, MBL backs off gracefully and falls back to CPU/worker execution rather than crashing the job.

2. **External LLM / Inference Endpoint Reuse:**
   - Rather than loading a second LLM into NPU memory when an inference server is already running on the host, MBL supports connecting to any configurable Ollama/OpenAI-compatible endpoint (e.g. `ai.ollama_url`, auto-probing common local ports such as `11434` and `8000` for `hailo-ollama`).
   - This lets MBL's AI Agent, semantic search, and Co-watching Companion (M7.16) share the already loaded model weights without duplicating NPU context.

3. **Cooperative Duty-Cycle Throttling & Scheduling Priority:**
   - Background MBL tasks (Whisper subtitle generation, intro/credits OCR, offline super-resolution) run with configurable duty-cycle pacing (inter-batch yields and lower HailoRT scheduler priority) so real-time co-tenants (like NVR camera feeds) maintain sub-100ms frame latency.
   - Users can configure NPU usage policy in `mbl.toml`: `auto` (shared + throttled), `exclusive` (full speed when dedicated), `offload_only` (only via HTTP/Ollama API), or `disabled`.

4. **Generalized Hardware Detection in `mbl-hwaccel`:**
   - Probes `/dev/h1x-*`, `/dev/hailo*`, `/sys/class/hailo_chardev/`, and PCI vendor `1e60` (`Hailo-8`, `Hailo-8L`, `Hailo-10H`), and reports detected NPU model, driver version, sharing mode, and active co-tenant status in `/api/v1/system/info` and `/api/v1/hw/report`.

## Consequences
- **Positive:** MBL works out-of-the-box alongside Frigate, `hailo-ollama`, Home Assistant, or any other Hailo workload without device lockouts or frame drops.
- **Positive:** Users with a dedicated NPU can switch `hw.hailo.mode = "exclusive"` for maximum batch throughput, while shared setups stay safe by default.
- **Trade-off:** Default duty-cycle pacing slightly increases wall-clock time for background subtitle/upscaling jobs in exchange for predictable real-time co-tenant latency.
