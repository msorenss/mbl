import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Badge, Button, Switch } from "./ui";

type Status = components["schemas"]["HailoModelsStatus"];
type Model = components["schemas"]["HailoModel"];
type Activity = components["schemas"]["NpuActivity"];

function size(bytes?: number | null): string {
  if (!bytes) return "";
  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(1)} GB`
    : `${Math.round(bytes / 1024 ** 2)} MB`;
}

function reason(error: unknown, fallback: string): string {
  const text = (error as { message?: string } | undefined)?.message;
  return typeof text === "string" && text ? text.replace(/^[a-z_]+:\s*/, "") : fallback;
}

/**
 * AI models for a Hailo-10H: scene descriptions and speech to text, downloaded from Hailo for the
 * installed HailoRT version with one click.
 */
export default function HailoModels() {
  const [status, setStatus] = createSignal<Status | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [activity, setActivity] = createSignal<Activity | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const loadActivity = async () => {
    try {
      const result = await client.GET("/api/v1/hw/npu");
      if (result.data) setActivity(result.data);
    } catch {}
  };
  const activityTimer = setInterval(() => void loadActivity(), 10_000);
  onCleanup(() => clearInterval(activityTimer));

  const toggle = async (enabled: boolean) => {
    setError("");
    try {
      const result = await client.PUT("/api/v1/hw/npu", { body: { enabled } });
      if (!result.data) throw new Error();
      setActivity(result.data);
    } catch {
      setError("Could not change the NPU setting.");
    }
  };

  const load = async () => {
    try {
      const result = await client.GET("/api/v1/hw/models");
      if (!result.data) throw new Error();
      setStatus(result.data);
      // Follow a running download.
      if (result.data.download && !result.data.download.finished) timer = setTimeout(load, 2000);
    } catch {
      setError("Could not load the NPU models.");
    }
  };
  onMount(() => {
    void load();
    void loadActivity();
  });
  onCleanup(() => clearTimeout(timer));

  const download = async (model: Model) => {
    const free = status()?.freeBytes;
    if (
      !window.confirm(
        `Download ${model.file} (${size(model.sizeBytes) || "size unknown"}) from Hailo's server?` +
          (free ? ` ${size(free)} is free.` : ""),
      )
    )
      return;
    setBusy(true);
    setError("");
    try {
      const result = await client.POST("/api/v1/hw/models/{file}/download", {
        params: { path: { file: model.file } },
      });
      if (!result.response.ok) throw new Error(reason(result.error, "The download did not start."));
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : "The download did not start.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (model: Model) => {
    if (!window.confirm(`Remove ${model.file}?`)) return;
    setBusy(true);
    setError("");
    try {
      const result = await client.DELETE("/api/v1/hw/models/{file}", {
        params: { path: { file: model.file } },
      });
      if (!result.response.ok) throw new Error();
      await load();
    } catch {
      setError("Could not remove the model.");
    } finally {
      setBusy(false);
    }
  };

  const running = () => {
    const d = status()?.download;
    return d && !d.finished ? d : null;
  };

  return (
    <section class="mbl-card-section">
      <h2 style={{ "font-size": "18px", "font-weight": "700", margin: "0 0 8px" }}>
        AI models for the NPU
      </h2>
      <p style={{ margin: "0 0 12px", color: "var(--text-secondary)" }}>
        A Hailo-10H can describe what is on screen for the companion and the assistant, and turn
        speech into text, without anything leaving the server. Download a model here; MBL fetches
        the build that matches the installed HailoRT. Object detection (YOLO) needs no download when
        Frigate's models are present.
      </p>
      <Show when={activity()}>
        {(a) => (
          <div style={{ display: "grid", gap: "6px", margin: "0 0 12px" }}>
            <Switch
              checked={a().enabled}
              onChange={(on: boolean) => void toggle(on)}
              label="Let MBL use the NPU"
            />
            <div style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
              {a().enabled
                ? `Since start: ${a().framesDetected} frames analysed, ${a().descriptions} scene descriptions, ${a().transcriptions} transcriptions, ${a().failures} failures.`
                : "Off: MBL does not use the NPU and has released everything it held. The companion uses dialogue and metadata only."}
            </div>
            <Show when={a().enabled}>
              <div style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
                {`Loaded now: ${
                  [
                    a().detectionLoaded ? (a().detectionModel ?? "detection") : null,
                    a().descriptionLoaded ? "scene description" : null,
                  ]
                    .filter(Boolean)
                    .join(", ") || "nothing (models are released when idle)"
                }`}
              </div>
            </Show>
            <Show when={a().lastError}>
              <div style={{ "font-size": "13px", color: "var(--danger)" }}>
                {`Last problem: ${a().lastError}`}
              </div>
            </Show>
          </div>
        )}
      </Show>
      <Show when={status()}>
        {(s) => (
          <>
            <p style={{ margin: "0 0 12px", "font-size": "13px" }}>
              {`NPU: ${s().npu ?? "none"} · HailoRT: ${s().hailortVersion ?? "not installed"} · ${s().directory}`}
              <Show when={s().freeBytes}>{(free) => ` · ${size(free())} free`}</Show>
            </p>
            <Show when={s().reason}>
              {(why) => (
                <p role="status" style={{ margin: "0 0 12px" }}>
                  {why()} See the setup guide in the documentation (Hailo NPU).
                </p>
              )}
            </Show>
            <ul
              style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "8px" }}
            >
              <For each={s().models}>
                {(m) => (
                  <li
                    style={{
                      display: "flex",
                      gap: "12px",
                      "align-items": "center",
                      padding: "10px",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-md)",
                      "flex-wrap": "wrap",
                    }}
                  >
                    <div style={{ flex: 1, "min-width": "200px" }}>
                      <strong>{m.name}</strong>{" "}
                      <Show when={m.recommended}>
                        <Badge>Recommended</Badge>
                      </Show>{" "}
                      <Show when={m.active}>
                        <Badge>In use</Badge>
                      </Show>
                      <div style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
                        {m.description}
                      </div>
                      <div style={{ "font-size": "12px", color: "var(--text-muted)" }}>
                        {m.file}
                        <Show when={m.sizeBytes}>{` · ${size(m.sizeBytes)}`}</Show>
                      </div>
                    </div>
                    <Show
                      when={running()?.file === m.file}
                      fallback={
                        <Show
                          when={m.installed}
                          fallback={
                            <Button
                              size="sm"
                              disabled={busy() || !s().supported || Boolean(running())}
                              onClick={() => void download(m)}
                            >
                              Download
                            </Button>
                          }
                        >
                          <Button
                            size="sm"
                            variant="secondary"
                            disabled={busy()}
                            onClick={() => void remove(m)}
                          >
                            Remove
                          </Button>
                        </Show>
                      }
                    >
                      {(() => {
                        const d = running();
                        const share = d?.totalBytes
                          ? Math.floor((100 * d.receivedBytes) / d.totalBytes)
                          : null;
                        return (
                          <span role="status">
                            {share === null ? "Downloading…" : `Downloading ${share} %`}
                          </span>
                        );
                      })()}
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={s().download?.finished && s().download?.error}>
              <p role="alert" style={{ color: "var(--danger)" }}>
                {`${s().download?.file}: ${s().download?.error}`}
              </p>
            </Show>
          </>
        )}
      </Show>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)" }}>
          {error()}
        </p>
      </Show>
    </section>
  );
}
