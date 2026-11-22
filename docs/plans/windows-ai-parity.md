# Same AI features on Windows (NVIDIA, Intel, AMD)

Goal: a Windows server without a Hailo NPU does what a Raspberry Pi with a Hailo-10H does today,
on the graphics card where there is one and on the processor otherwise, without the user
picking engines by hand.

| Feature on the Pi | What it does in MBL | Windows plan |
|---|---|---|
| YOLO on the NPU | Objects and people in the frame (companion scene) | ONNX Runtime with DirectML |
| Qwen2-VL on the NPU | A sentence about the scene | Ollama with a vision model |
| Whisper on the NPU | Talk (speech to text) | whisper.cpp with Vulkan, or Ollama-free CPU fallback |
| Piper | Read replies aloud | Piper for Windows (already in the voice installer) |

## Why these engines

- **Ollama for the scene sentence.** MBL already talks to Ollama for the assistant
  (OpenAI-compatible API). Ollama runs vision models (Qwen2.5-VL 3B/7B, Gemma 3 4B) on NVIDIA
  (CUDA), AMD (ROCm on supported cards, otherwise Vulkan) and the processor, and handles
  downloads and memory itself. MBL sends the frame as an image and gets the sentence back. One
  install, one model choice, the same code path on Linux PCs and Macs.
- **ONNX Runtime + DirectML for objects.** DirectML runs on every DirectX 12 card (NVIDIA,
  AMD, Intel, including integrated graphics). YOLO is small; a frame takes milliseconds. Ollama
  is the wrong tool for detection (it does not run YOLO).
- **whisper.cpp for speech.** One small program, Vulkan backend for all three vendors and a fast
  CPU path; models of 75–500 MB. Ollama has no speech-to-text API today.
- **Piper for reading aloud** already has a Windows build in the voice installer; it runs on the
  processor and needs no graphics card.

## Phases

1. **W0 – Engine abstraction.** A `SceneEngine` and `SpeechEngine` interface in `mbl-service`
   with the Hailo implementation behind it; status on the Hardware page says which engine does
   what. No behaviour change on the Pi.
2. **W1 – Ollama scene sentences.** Use the assistant's Ollama connection (or a separate address)
   with a vision model; same prompt and 60-second reuse as on the NPU; one request at a time,
   skipped while transcoding is busy. Works on any platform with Ollama, including a Pi without
   Hailo (slowly).
3. **W2 – Objects with ONNX Runtime.** Ship or download `onnxruntime` with the DirectML provider
   on Windows (CUDA/ROCm optional on Linux), YOLOv11s ONNX (AGPL-3.0, Ultralytics, which
   matches MBL's licence), same output as the Hailo path.
4. **W3 – whisper.cpp.** Installer on the Hardware page like the NPU models (program + `ggml`
   model per size), Vulkan when a card is present, CPU otherwise; same `/speech/transcribe`.
5. **W4 – Piper on Windows.** Test the existing installer with the `.zip` build and the voices;
   add the Microsoft speech voices as a second browser-side option where present.
6. **W5 – Acceptance.** RTX 4060 (the user's Windows server), an Intel iGPU and an AMD card if
   available: frame→sentence time, objects per frame, 60 s of speech, GPU memory next to NVENC
   transcoding.

## Things to settle

- Transcoding and AI share the graphics card: give transcoding priority (pause scene sentences
  while NVENC sessions run at full capacity), like the NPU gives way to Frigate.
- Licences: Ollama (MIT), Qwen2.5-VL 3B (Qwen Research licence: check before recommending;
  7B is Apache-2.0), Gemma 3 (Gemma terms), whisper.cpp (MIT), Whisper models (MIT),
  ONNX Runtime (MIT), YOLOv11 (AGPL-3.0). Prefer Apache/MIT models in the defaults.
- Downloads are always started by an administrator, with sizes shown, as for the NPU models.
