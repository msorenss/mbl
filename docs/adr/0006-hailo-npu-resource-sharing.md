# 0006. Hailo NPU Media & AI Resource Sharing

- Status: Accepted
- Date: 2026-10-25

## Context
When running MBL on a Raspberry Pi 5 equipped with a Hailo-10H (or Hailo-8) AI accelerator, the NPU is frequently shared with existing edge workloads. On this host system:
1. **Frigate NVR** (`msorenss79/hailo-frigate-h10`) runs real-time object detection across active RTSP cameras using `/dev/h1x-0` and `yolov11l.hef`.
2. **hailo-ollama** runs in Docker on port `8000` (`HAILO_OLLAMA_VDEVICE_GROUP_ID=SHARED`), serving local LLM models (DeepSeek, Llama 3.2, Qwen).

If MBL opens `/dev/h1x-0` or `/dev/hailo0` without multi-process coordination, HailoRT takes an exclusive hardware lock and other processes fail with `HAILO_OUT_OF_PHYSICAL_DEVICES` or `Device is busy`.

## Decision
MBL adopts a three-tiered resource-sharing strategy for Hailo accelerators:

1. **VDevice Group ID (`group_id = "SHARED"`):**
   Whenever MBL accesses the HailoRT driver directly for dedicated inference models (such as `hailo-whisper` for subtitle generation or super-resolution CNNs), it must initialize the `VDevice` with `group_id = "SHARED"` (or inherit `HAILO_VDEVICE_GROUP_ID`). This enables driver-level queue multiplexing so Frigate never loses hardware access.

2. **Reuse Local `hailo-ollama` Service for LLM & MCP Agents:**
   Rather than loading large language models into a separate raw device instance, MBL's AI Agent and "Co-watching Companion" (M7.16) connect directly to the already running Ollama-compatible endpoint at `http://127.0.0.1:8000`. This completely avoids raw device contention for LLM workloads.

3. **Cooperative Low-Priority Batching for Offline Tasks:**
   Offline background jobs (Whisper speech-to-text and AI super-resolution) must run in small inference chunks with inter-batch yields to guarantee that Frigate's real-time camera inference maintains sub-100ms latency.

4. **Hardware Detection in `mbl-hwaccel`:**
   Platform fingerprinting detects both `/dev/h1x*` (Hailo-10 / Hailo-10H) and `/dev/hailo*` (Hailo-8 / Hailo-8L), checks whether `hailo1x_pci` or `hailo_pci` kernel drivers are loaded, and probes `http://127.0.0.1:8000` to report local NPU LLM availability in `/api/v1/system/info`.

## Consequences
- **Positive:** MBL coexists seamlessly with Frigate and existing home surveillance on the same Raspberry Pi 5 without dropping camera frames or causing device lockouts.
- **Positive:** Zero cloud API costs or internet dependency for voice-to-text, intro OCR, and AI co-watching features.
- **Trade-off:** Offline batch jobs (transcribing entire libraries) must yield time to real-time workloads, taking slightly longer than on a dedicated, unshared accelerator.
