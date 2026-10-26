import { createSignal, For, onMount, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { useToast } from "../../context/ToastContext";

type HwReport = components["schemas"]["HwReport"];
type SystemHealthReport = components["schemas"]["SystemHealthReport"];

export function Hardware() {
  const { showToast } = useToast();
  const [report, setReport] = createSignal<HwReport | null>(null);
  const [sysHealth, setSysHealth] = createSignal<SystemHealthReport | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [testing, setTesting] = createSignal(false);

  const loadReport = async () => {
    setLoading(true);
    try {
      const [{ data }, healthRes] = await Promise.all([
        client.GET("/api/v1/hw/report"),
        client.GET("/api/v1/system/health").catch(() => ({ data: undefined })),
      ]);
      if (data) {
        setReport(data);
      }
      if (healthRes.data) {
        setSysHealth(healthRes.data);
      }
    } catch {
      showToast({
        title: "Error",
        message: "Failed to load hardware acceleration report",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  onMount(() => {
    loadReport();
  });

  const handleRunSelftest = async () => {
    setTesting(true);
    try {
      const { data, error } = await client.POST("/api/v1/hw/test");
      if (data && !error) {
        await loadReport();
        const okCount = data.capabilities.filter((c) => c.ok).length;
        showToast({
          title: "Self-Test Complete",
          message: `${okCount}/${data.capabilities.length} capability checks passed (preferred: ${data.capacity.preferredBackend})`,
          type: "success",
        });
      } else {
        showToast({
          title: "Self-Test Failed",
          message: "Could not complete hardware self-test",
          type: "error",
        });
      }
    } catch {
      showToast({
        title: "Self-Test Failed",
        message: "Error running hardware self-test",
        type: "error",
      });
    } finally {
      setTesting(false);
    }
  };

  const formatBytes = (bytes?: number | null) => {
    if (!bytes) return "0 B";
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(1)} GB`;
    const mb = bytes / (1024 * 1024);
    return `${mb.toFixed(0)} MB`;
  };

  return (
    <div>
      {/* Header */}
      <div
        style={{
          display: "flex",
          "justify-content": "space-between",
          "align-items": "flex-start",
          "margin-bottom": "28px",
          gap: "16px",
        }}
      >
        <div>
          <h1 style={{ margin: "0 0 6px", "font-size": "24px", "font-weight": "700" }}>
            Hardware Acceleration & NPU
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            Inspect detected GPUs, V4L2/DRM decoders, Hailo NPU co-tenancy, and transcode benchmark
            results.
          </p>
        </div>

        <button
          type="button"
          data-testid="run-hw-selftest"
          onClick={handleRunSelftest}
          disabled={testing()}
          style={{
            padding: "9px 18px",
            background: "var(--accent)",
            color: "#fff",
            border: "none",
            "border-radius": "var(--radius-md)",
            "font-weight": "600",
            "font-size": "14px",
            cursor: testing() ? "wait" : "pointer",
            opacity: testing() ? 0.7 : 1,
            display: "flex",
            "align-items": "center",
            gap: "8px",
          }}
        >
          <span>⚡</span>
          <span>{testing() ? "Running Self-Test..." : "Run Self-Test"}</span>
        </button>
      </div>

      <Show
        when={!loading() && report()}
        fallback={<div style={{ color: "var(--text-muted)" }}>Loading hardware report...</div>}
      >
        {(rep) => (
          <div style={{ display: "flex", "flex-direction": "column", gap: "24px" }}>
            {/* Thermal & Under-Voltage Warning Banner (M5.1) */}
            <Show
              when={
                sysHealth()?.thermal.underVoltage ||
                sysHealth()?.thermal.underVoltageOccurred ||
                sysHealth()?.thermal.currentlyThrottled ||
                sysHealth()?.thermal.idleJobsPaused
              }
            >
              <div
                data-testid="hw-thermal-warning"
                style={{
                  padding: "14px 18px",
                  background: "rgba(239, 68, 68, 0.14)",
                  border: "1px solid var(--danger)",
                  "border-radius": "var(--radius-md)",
                  color: "var(--danger)",
                  "font-size": "13px",
                  "font-weight": "600",
                }}
              >
                ⚠️ Power / Thermal Alert:{" "}
                {sysHealth()?.thermal.underVoltage || sysHealth()?.thermal.underVoltageOccurred
                  ? "Under-voltage detected on SoC rail (check 5V/5A USB-C PD power supply). "
                  : ""}
                {sysHealth()?.thermal.idleJobsPaused
                  ? "Background Idle jobs are automatically paused due to high SoC temperature (>75 °C) or active thermal throttling."
                  : ""}
              </div>
            </Show>

            {/* Top summary grid */}
            <div
              style={{
                display: "grid",
                "grid-template-columns": "repeat(auto-fit, minmax(300px, 1fr))",
                gap: "16px",
              }}
            >
              {/* Platform & System Fingerprint Card */}
              <div
                data-testid="hw-fingerprint-card"
                style={{
                  padding: "20px",
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-md)",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                    "margin-bottom": "14px",
                  }}
                >
                  <h2 style={{ margin: 0, "font-size": "16px", "font-weight": "700" }}>
                    Platform Fingerprint
                  </h2>
                  <span
                    data-testid="hw-profile-badge"
                    style={{
                      padding: "3px 10px",
                      "border-radius": "12px",
                      background: "var(--accent-light)",
                      color: "var(--accent)",
                      "font-size": "12px",
                      "font-weight": "700",
                      "text-transform": "uppercase",
                    }}
                  >
                    {rep().fingerprint.profile}
                  </span>
                </div>

                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "8px",
                    "font-size": "13px",
                  }}
                >
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>CPU:</span>
                    <span style={{ "font-weight": "600" }}>
                      {rep().fingerprint.cpuModel} ({rep().fingerprint.cpuCores} cores)
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Memory:</span>
                    <span style={{ "font-weight": "600" }}>
                      {formatBytes(rep().fingerprint.totalMemoryBytes)}
                    </span>
                  </div>
                  <Show when={rep().fingerprint.boardModel}>
                    <div style={{ display: "flex", "justify-content": "space-between" }}>
                      <span style={{ color: "var(--text-secondary)" }}>Board:</span>
                      <span style={{ "font-weight": "600" }}>{rep().fingerprint.boardModel}</span>
                    </div>
                  </Show>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>NPU Profile:</span>
                    <span
                      data-testid="hw-npu-badge"
                      style={{
                        padding: "1px 8px",
                        "border-radius": "10px",
                        background:
                          rep().fingerprint.npuProfile !== "none"
                            ? "var(--success-light, rgba(16, 185, 129, 0.15))"
                            : "var(--bg-overlay)",
                        color:
                          rep().fingerprint.npuProfile !== "none"
                            ? "var(--success)"
                            : "var(--text-secondary)",
                        "font-weight": "600",
                        "font-size": "12px",
                      }}
                    >
                      {rep().fingerprint.npuProfile}
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Device Nodes:</span>
                    <span>
                      {rep().fingerprint.drmDevices.length} DRM,{" "}
                      {rep().fingerprint.v4l2Devices.length} V4L2,{" "}
                      {rep().fingerprint.nvidiaGpus.length} NVIDIA,{" "}
                      {rep().fingerprint.hailoDevices.length} Hailo
                    </span>
                  </div>
                  <Show when={sysHealth()?.thermal.temperatureC != null}>
                    <div style={{ display: "flex", "justify-content": "space-between" }}>
                      <span style={{ color: "var(--text-secondary)" }}>SoC Temp:</span>
                      <span
                        data-testid="hw-soc-temp"
                        style={{
                          "font-weight": "600",
                          color:
                            (sysHealth()?.thermal.temperatureC ?? 0) >= 75
                              ? "var(--danger)"
                              : "var(--text-primary)",
                        }}
                      >
                        {sysHealth()?.thermal.temperatureC?.toFixed(1)} °C
                      </span>
                    </div>
                  </Show>
                </div>
              </div>

              {/* Transcode Capacity Card */}
              <div
                data-testid="hw-capacity-card"
                style={{
                  padding: "20px",
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-md)",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                    "margin-bottom": "14px",
                  }}
                >
                  <h2 style={{ margin: 0, "font-size": "16px", "font-weight": "700" }}>
                    Transcode Capacity
                  </h2>
                  <span
                    data-testid="hw-preferred-backend"
                    style={{
                      padding: "3px 10px",
                      "border-radius": "12px",
                      background: "var(--bg-overlay)",
                      color: "var(--text-primary)",
                      "font-size": "12px",
                      "font-weight": "700",
                    }}
                  >
                    {rep().capacity.preferredBackend}
                  </span>
                </div>

                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "8px",
                    "font-size": "13px",
                  }}
                >
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Video Sessions:</span>
                    <span data-testid="hw-session-slots" style={{ "font-weight": "600" }}>
                      {rep().capacity.activeVideoSessions} / {rep().capacity.maxVideoSessions}{" "}
                      active
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Fallback Chain:</span>
                    <span style={{ "font-family": "var(--font-mono)", "font-size": "12px" }}>
                      {rep().capacity.fallbackBackends.join(" → ") || "software"}
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Encode Codecs:</span>
                    <span>{rep().capacity.encodeCodecs.join(", ") || "h264"}</span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>HDR Tone Mapping:</span>
                    <span
                      style={{
                        color: rep().capacity.canTonemap ? "var(--success)" : "var(--text-muted)",
                        "font-weight": "600",
                      }}
                    >
                      {rep().capacity.canTonemap ? "Supported" : "Disabled on Profile"}
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>ABR Multi-Rendition:</span>
                    <span
                      style={{
                        color: rep().capacity.supportsAbrLadder
                          ? "var(--success)"
                          : "var(--text-muted)",
                        "font-weight": "600",
                      }}
                    >
                      {rep().capacity.supportsAbrLadder ? "GPU Split Ladder" : "Single Rendition"}
                    </span>
                  </div>
                </div>
              </div>

              {/* Hailo NPU & Cooperative Sharing Card */}
              <div
                data-testid="hw-hailo-card"
                style={{
                  padding: "20px",
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-md)",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    "justify-content": "space-between",
                    "align-items": "center",
                    "margin-bottom": "14px",
                  }}
                >
                  <h2 style={{ margin: 0, "font-size": "16px", "font-weight": "700" }}>
                    Hailo NPU Co-Tenancy
                  </h2>
                  <span
                    style={{
                      padding: "3px 10px",
                      "border-radius": "12px",
                      background: "var(--bg-overlay)",
                      color: "var(--text-secondary)",
                      "font-size": "12px",
                      "font-weight": "600",
                    }}
                  >
                    {rep().hailo.sharingMode}
                  </span>
                </div>

                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "8px",
                    "font-size": "13px",
                  }}
                >
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>VDevice Group ID:</span>
                    <span style={{ "font-family": "var(--font-mono)", "font-size": "12px" }}>
                      {rep().hailo.vdeviceGroupId}
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Duty Cycle Budget:</span>
                    <span style={{ "font-weight": "600" }}>{rep().hailo.dutyCyclePercent}%</span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>Active Co-Tenants:</span>
                    <span>
                      {rep().hailo.activeCoTenants.length > 0
                        ? rep().hailo.activeCoTenants.join(", ")
                        : "None detected"}
                    </span>
                  </div>
                  <div style={{ display: "flex", "justify-content": "space-between" }}>
                    <span style={{ color: "var(--text-secondary)" }}>AI Pipelines:</span>
                    <span>
                      {rep().hailo.supportedPipelines.length > 0
                        ? rep().hailo.supportedPipelines.join(", ")
                        : "CPU Fallback"}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            {/* Self-Test Capability Matrix Table */}
            <div>
              <h2 style={{ "font-size": "18px", "font-weight": "700", "margin-bottom": "12px" }}>
                Self-Test Capability Matrix
              </h2>
              <div
                style={{
                  background: "var(--bg-surface)",
                  border: "1px solid var(--border)",
                  "border-radius": "var(--radius-md)",
                  overflow: "hidden",
                }}
              >
                <table
                  data-testid="hw-capabilities-table"
                  style={{
                    width: "100%",
                    "border-collapse": "collapse",
                    "text-align": "left",
                    "font-size": "13px",
                  }}
                >
                  <thead>
                    <tr
                      style={{
                        "border-bottom": "1px solid var(--border)",
                        background: "var(--bg-card)",
                        color: "var(--text-secondary)",
                      }}
                    >
                      <th style={{ padding: "12px 16px" }}>Backend</th>
                      <th style={{ padding: "12px 16px" }}>Codec</th>
                      <th style={{ padding: "12px 16px" }}>Operation</th>
                      <th style={{ padding: "12px 16px" }}>Status</th>
                      <th style={{ padding: "12px 16px" }}>Throughput (FPS)</th>
                      <th style={{ padding: "12px 16px" }}>Details / Error</th>
                    </tr>
                  </thead>
                  <tbody>
                    <Show
                      when={rep().capabilities.length > 0}
                      fallback={
                        <tr>
                          <td
                            colSpan={6}
                            style={{
                              padding: "24px",
                              "text-align": "center",
                              color: "var(--text-muted)",
                            }}
                          >
                            No self-test results recorded yet. Click "Run Self-Test" to benchmark
                            available backends.
                          </td>
                        </tr>
                      }
                    >
                      <For each={rep().capabilities}>
                        {(cap) => (
                          <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                            <td style={{ padding: "12px 16px", "font-weight": "600" }}>
                              {cap.backend}
                            </td>
                            <td
                              style={{
                                padding: "12px 16px",
                                "font-family": "var(--font-mono)",
                                "font-size": "12px",
                              }}
                            >
                              {cap.codec}
                            </td>
                            <td style={{ padding: "12px 16px" }}>{cap.operation}</td>
                            <td style={{ padding: "12px 16px" }}>
                              <span
                                style={{
                                  padding: "2px 8px",
                                  "border-radius": "10px",
                                  "font-size": "11px",
                                  "font-weight": "600",
                                  background: cap.ok
                                    ? "rgba(16, 185, 129, 0.15)"
                                    : "rgba(239, 68, 68, 0.15)",
                                  color: cap.ok ? "var(--success)" : "var(--danger)",
                                }}
                              >
                                {cap.ok ? "PASS" : "FAIL"}
                              </span>
                            </td>
                            <td style={{ padding: "12px 16px" }}>
                              {cap.fps != null ? `${cap.fps.toFixed(1)} fps` : "—"}
                            </td>
                            <td
                              style={{
                                padding: "12px 16px",
                                color: "var(--text-muted)",
                                "font-size": "12px",
                              }}
                            >
                              {cap.error || "OK"}
                            </td>
                          </tr>
                        )}
                      </For>
                    </Show>
                  </tbody>
                </table>
              </div>
            </div>

            {/* Recent Backend Failures */}
            <Show when={rep().recentFailures.length > 0}>
              <div>
                <h2 style={{ "font-size": "18px", "font-weight": "700", "margin-bottom": "12px" }}>
                  Recent Backend Fallback Events
                </h2>
                <div
                  style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--danger)",
                    "border-radius": "var(--radius-md)",
                    overflow: "hidden",
                  }}
                >
                  <table
                    data-testid="hw-failures-table"
                    style={{
                      width: "100%",
                      "border-collapse": "collapse",
                      "text-align": "left",
                      "font-size": "13px",
                    }}
                  >
                    <thead>
                      <tr
                        style={{
                          "border-bottom": "1px solid var(--border)",
                          background: "var(--bg-card)",
                          color: "var(--text-secondary)",
                        }}
                      >
                        <th style={{ padding: "10px 16px" }}>Backend</th>
                        <th style={{ padding: "10px 16px" }}>Codec</th>
                        <th style={{ padding: "10px 16px" }}>Reason</th>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={rep().recentFailures}>
                        {(fail) => (
                          <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                            <td style={{ padding: "10px 16px", "font-weight": "600" }}>
                              {fail.backend}
                            </td>
                            <td style={{ padding: "10px 16px" }}>{fail.codec}</td>
                            <td style={{ padding: "10px 16px", color: "var(--danger)" }}>
                              {fail.reason}
                            </td>
                          </tr>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </div>
            </Show>
          </div>
        )}
      </Show>
    </div>
  );
}

export default Hardware;
