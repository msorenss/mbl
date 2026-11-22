import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Badge, Button } from "./ui";

type Status = components["schemas"]["HailoOllamaStatus"];
type Model = components["schemas"]["HailoOllamaModel"];

function size(bytes?: number | null): string {
  if (!bytes) return "";
  return bytes >= 1024 ** 3
    ? `${(bytes / 1024 ** 3).toFixed(1)} GB`
    : `${Math.round(bytes / 1024 ** 2)} MB`;
}

/**
 * Hailo-Ollama: small language models on the Hailo-10H, for the assistant without a cloud
 * service. MBL starts it, downloads its models and stops it when the NPU switch is off.
 */
export default function HailoOllama() {
  const [status, setStatus] = createSignal<Status | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const load = async () => {
    try {
      const result = await client.GET("/api/v1/hw/ollama");
      if (!result.data) throw new Error();
      setStatus(result.data);
      const pull = result.data.pull;
      if (pull && !pull.finished) timer = setTimeout(load, 1500);
    } catch {
      setError("Could not load Hailo-Ollama.");
    }
  };
  onMount(() => void load());
  onCleanup(() => clearTimeout(timer));

  const act = async (
    call: () => Promise<{ response: Response; error?: unknown }>,
    failure: string,
  ) => {
    setBusy(true);
    setError("");
    try {
      const result = await call();
      if (!result.response.ok) {
        const body = result.error as { error?: { message?: string } } | undefined;
        throw new Error(body?.error?.message?.replace(/^[a-z_]+:\s*/, "") || failure);
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : failure);
    } finally {
      setBusy(false);
    }
  };

  const running = () => {
    const p = status()?.pull;
    return p && !p.finished ? p : null;
  };

  return (
    <section class="mbl-card-section">
      <h2 style={{ "font-size": "18px", "font-weight": "700", margin: "0 0 8px" }}>
        Language models on the NPU (Hailo-Ollama)
      </h2>
      <p style={{ margin: "0 0 12px", color: "var(--text-secondary)" }}>
        Small language models that run on the Hailo-10H, for the assistant without a cloud service.
        They are good at short answers and summaries and weaker at searching the library for you.
        After downloading one, choose Hailo-Ollama and the model under Server settings → Assistant.
      </p>
      <Show when={status()}>
        {(s) => (
          <Show
            when={s().installed}
            fallback={
              <p role="status">
                Hailo-Ollama is not installed. See the setup guide in the documentation (Hailo NPU).
              </p>
            }
          >
            <div
              style={{
                display: "flex",
                gap: "12px",
                "align-items": "center",
                "flex-wrap": "wrap",
                "margin-bottom": "12px",
              }}
            >
              <Show when={s().running} fallback={<Badge>Stopped</Badge>}>
                <Badge>Running</Badge>
              </Show>
              <span translate="no" style={{ "font-size": "13px", color: "var(--text-muted)" }}>
                {s().assistantUrl}
              </span>
              <Show
                when={s().running}
                fallback={
                  <Button
                    size="sm"
                    disabled={busy()}
                    onClick={() =>
                      void act(
                        () => client.POST("/api/v1/hw/ollama/start"),
                        "Hailo-Ollama did not start.",
                      )
                    }
                  >
                    Start
                  </Button>
                }
              >
                <Show when={s().managed}>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={busy() || Boolean(running())}
                    onClick={() =>
                      void act(
                        () => client.POST("/api/v1/hw/ollama/stop"),
                        "Hailo-Ollama did not stop.",
                      )
                    }
                  >
                    Stop
                  </Button>
                </Show>
              </Show>
            </div>
            <ul
              style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "6px" }}
            >
              <For each={s().models}>
                {(model: Model) => (
                  <li style={{ display: "flex", gap: "12px", "align-items": "center" }}>
                    <span translate="no" style={{ flex: 1 }}>
                      {model.name}
                      <Show when={model.sizeBytes}>{` · ${size(model.sizeBytes)}`}</Show>
                    </span>
                    <Show
                      when={running()?.model === model.name}
                      fallback={
                        <Show
                          when={model.installed}
                          fallback={
                            <Button
                              size="sm"
                              variant="secondary"
                              disabled={busy() || !s().running || Boolean(running())}
                              onClick={() => {
                                if (!window.confirm(`Download ${model.name} for the NPU?`)) return;
                                void act(
                                  () =>
                                    client.POST("/api/v1/hw/ollama/models/{name}/pull", {
                                      params: { path: { name: model.name } },
                                    }),
                                  "The download did not start.",
                                );
                              }}
                            >
                              Download
                            </Button>
                          }
                        >
                          <Button
                            size="sm"
                            variant="secondary"
                            disabled={busy() || !s().running}
                            onClick={() => {
                              if (!window.confirm(`Remove ${model.name}?`)) return;
                              void act(
                                () =>
                                  client.DELETE("/api/v1/hw/ollama/models/{name}", {
                                    params: { path: { name: model.name } },
                                  }),
                                "Could not remove the model.",
                              );
                            }}
                          >
                            Remove
                          </Button>
                        </Show>
                      }
                    >
                      <span role="status">
                        {(() => {
                          const p = running();
                          return p?.total
                            ? `Downloading ${Math.floor((100 * p.completed) / p.total)} %`
                            : "Downloading…";
                        })()}
                      </span>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={s().pull?.finished && s().pull?.error}>
              <p role="alert" style={{ color: "var(--danger)" }}>
                {`${s().pull?.model}: ${s().pull?.error}`}
              </p>
            </Show>
          </Show>
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
