import { render } from "solid-js/web";
import { describe, expect, it, vi } from "vitest";
import client from "../../api/client";
import { ToastProvider } from "../../context/ToastContext";
import { Hardware } from "./Hardware";

describe("Admin Hardware Acceleration UI (M4.15)", () => {
  it("renders detected hardware, Hailo NPU status, self-test results, capacity, and recent failures", async () => {
    vi.spyOn(client, "GET").mockImplementation(async (path: string) => {
      if (path === "/api/v1/hw/report") {
        return {
          data: {
            fingerprint: {
              os: "linux",
              arch: "aarch64",
              profile: "pi5",
              npuProfile: "hailo10h",
              boardModel: "Raspberry Pi 5 Model B Rev 1.0",
              cpuModel: "Cortex-A76",
              cpuCores: 4,
              totalMemoryBytes: 8589934592,
              drmDevices: [
                {
                  node: "/dev/dri/renderD128",
                  driver: "v3d",
                  vendorId: null,
                  deviceId: null,
                },
              ],
              v4l2Devices: [
                {
                  node: "/dev/video19",
                  driver: "rpivid",
                  card: "rpivid-hevc-dec",
                  isM2m: true,
                  isDecoder: true,
                  isEncoder: false,
                },
              ],
              nvidiaGpus: [],
              dxgiAdapters: [],
              hailoDevices: [
                {
                  node: "/dev/h1x-0",
                  boardName: "Hailo-10H",
                  deviceArchitecture: "HAILO10H",
                  profile: "hailo10h",
                  driverVersion: "5.1.1",
                },
              ],
              driverFingerprint: "fp-pi5-hailo10h",
            },
            ffmpeg: {
              binaryPath: "ffmpeg",
              binaryHash: "hash123",
              versionLine: "ffmpeg version 7.1.1-mbl-linux-arm64-rpi",
              hwaccels: ["drm", "vaapi"],
              decoders: ["hevc", "h264"],
              encoders: ["libx264"],
              filters: ["scale"],
            },
            capabilities: [
              {
                backend: "piDrmDecode",
                codec: "hevc",
                operation: "decode",
                ok: true,
                fps: 118.4,
                error: null,
                ffmpegHash: "hash123",
                driverFingerprint: "fp-pi5-hailo10h",
                testedAt: 1700000000000,
              },
              {
                backend: "software",
                codec: "h264",
                operation: "encode",
                ok: true,
                fps: 42.5,
                error: null,
                ffmpegHash: "hash123",
                driverFingerprint: "fp-pi5-hailo10h",
                testedAt: 1700000000000,
              },
            ],
            capacity: {
              profile: "pi5",
              preferredBackend: "piDrmDecode",
              fallbackBackends: ["piDrmDecode", "software"],
              encodeCodecs: ["h264"],
              canTonemap: false,
              supportsAbrLadder: false,
              maxVideoSessions: 1,
              activeVideoSessions: 0,
              availableVideoSessions: 1,
              canTranscodeVideo: true,
            },
            recentFailures: [
              {
                backend: "v4l2m2m",
                codec: "h264",
                reason: "Pi 5 lacks hardware H.264 encoder",
                failedAt: 1700000000000,
                expiresAt: 1700000600000,
              },
            ],
            hailo: {
              npuProfile: "hailo10h",
              sharingMode: "auto",
              vdeviceGroupId: "SHARED",
              dutyCyclePercent: 25,
              deviceBusyOrLocked: false,
              activeCoTenants: ["frigate"],
              externalInferenceUrl: "http://127.0.0.1:8000",
              supportedPipelines: ["whisperAsr", "localLlmGenAi"],
            },
          },
          error: undefined,
          response: new Response(),
        } as any;
      }
      return { data: undefined, error: undefined, response: new Response() } as any;
    });

    const container = document.createElement("div");
    document.body.appendChild(container);

    render(
      () => (
        <ToastProvider>
          <Hardware />
        </ToastProvider>
      ),
      container,
    );

    // Wait for async onMount loadReport
    await new Promise((r) => setTimeout(r, 50));

    expect(container.textContent).toContain("Hardware Acceleration & NPU");
    expect(container.textContent).toContain("pi5");
    expect(container.textContent).toContain("hailo10h");
    expect(container.textContent).toContain("Raspberry Pi 5 Model B Rev 1.0");
    expect(container.textContent).toContain("piDrmDecode");
    expect(container.textContent).toContain("118.4 fps");
    expect(container.textContent).toContain("frigate");
    expect(container.textContent).toContain("Pi 5 lacks hardware H.264 encoder");
  });
});
