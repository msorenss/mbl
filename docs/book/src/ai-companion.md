# AI Watch Companion & Hailo NPU

MBL delivers next-generation multimodal media intelligence using on-device neural processing units (NPU) and local LLMs.

## Hailo-8 & Hailo-10H NPU Architecture

On systems equipped with Hailo hardware acceleration (e.g. Raspberry Pi 5 with Hailo AI M.2 HAT):
- **Cooperative Scheduler (`mbl-hailo`):** Manages shared access to `/dev/hailo0` alongside other tenants (such as Frigate NVR) using fair-share time-slicing.
- **Multimodal Video Scene Search:** Generates CLIP visual embeddings for keyframes. Users can search naturally for scenes (e.g., "car chase in the rain", "diner conversation at dusk") with sub-10ms query execution.

## Real-Time AI Watch Companion

The AI Watch Companion protocol (`/api/v1/companion/sessions/{id}/*`) provides real-time co-watching experiences:
- **Live Dialogue Cues:** Extracts WebVTT subtitles and buffers current scene context into a live MCP resource (`mbl://sessions/{id}/live-context`).
- **Scene Explanations & Trivia:** AI models can answer context-aware questions ("Who is the actor that just walked in?") without spoiling future plot twists.
- **Spoiler-Free Recaps:** Generates "Previously on..." episode summaries constrained strictly by the user's recorded watch history.
- **Subtitle Auto-Sync (`mbl-subsync`):** Employs Voice Activity Detection (VAD) and energy envelope correlation to align out-of-sync external subtitle tracks within ±100 ms precision.
