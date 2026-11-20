import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Badge, Button } from "./ui";

type ProviderKind = components["schemas"]["ProviderKind"];
type Report = components["schemas"]["ConnectionReport"];

interface Preset {
  id: string;
  /** Product name, shown as is in every language. */
  brand: string;
  provider: ProviderKind;
  baseUrl: string;
  model: string;
  local?: boolean;
  /** For agent programs run as a command. */
  command?: string;
}

/** Claude Code without a shell: reads the question on stdin, MBL's MCP tools only. */
const CLAUDE_CODE_COMMAND =
  "claude -p --mcp-config {mcp_config} --strict-mcp-config --allowedTools mcp__mbl --output-format text";

const PRESETS: Preset[] = [
  {
    id: "anthropic",
    brand: "Anthropic (Claude)",
    provider: "anthropic",
    baseUrl: "https://api.anthropic.com",
    model: "claude-opus-5-5",
  },
  {
    id: "openai",
    brand: "OpenAI",
    provider: "openai_compatible",
    baseUrl: "https://api.openai.com/v1",
    model: "",
  },
  {
    id: "gemini",
    brand: "Google Gemini",
    provider: "openai_compatible",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    model: "",
  },
  {
    id: "openrouter",
    brand: "OpenRouter",
    provider: "openai_compatible",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "",
  },
  {
    id: "groq",
    brand: "Groq",
    provider: "openai_compatible",
    baseUrl: "https://api.groq.com/openai/v1",
    model: "",
  },
  {
    id: "mistral",
    brand: "Mistral",
    provider: "openai_compatible",
    baseUrl: "https://api.mistral.ai/v1",
    model: "",
  },
  {
    id: "deepseek",
    brand: "DeepSeek",
    provider: "openai_compatible",
    baseUrl: "https://api.deepseek.com/v1",
    model: "",
  },
  {
    id: "ollama",
    brand: "Ollama",
    provider: "openai_compatible",
    baseUrl: "http://localhost:11434/v1",
    model: "",
    local: true,
  },
  {
    id: "lmstudio",
    brand: "LM Studio",
    provider: "openai_compatible",
    baseUrl: "http://localhost:1234/v1",
    model: "",
    local: true,
  },
  {
    id: "hailo",
    brand: "Hailo-Ollama",
    provider: "openai_compatible",
    baseUrl: "http://127.0.0.1:8000/v1",
    model: "",
    local: true,
  },
  {
    id: "claude-code",
    brand: "Claude Code",
    provider: "command",
    baseUrl: "",
    model: "",
    command: CLAUDE_CODE_COMMAND,
  },
];

const label = {
  display: "block",
  "font-size": "13px",
  "font-weight": "600",
  "margin-bottom": "6px",
} as const;

const field = { width: "100%", "margin-bottom": "14px" } as const;

function presetFor(provider: ProviderKind, baseUrl: string, command = ""): string {
  if (provider === "command")
    return command.trim().startsWith("claude ") ? "claude-code" : "custom-command";
  const found = PRESETS.find((p) => p.provider === provider && p.baseUrl === baseUrl);
  return found ? found.id : provider === "anthropic" ? "custom-anthropic" : "custom";
}

function serverMessage(error: unknown, fallback: string): string {
  const message = (error as { error?: { message?: string } } | undefined)?.error?.message;
  return message?.replace(/^(validation|external service): /, "") || fallback;
}

/**
 * Admin settings for the built-in assistant: which language model provider it uses, with a
 * connection test and a model list.
 */
export default function AssistantSettings() {
  const [enabled, setEnabled] = createSignal(false);
  const [preset, setPreset] = createSignal("custom");
  const [provider, setProvider] = createSignal<ProviderKind>("openai_compatible");
  const [baseUrl, setBaseUrl] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [hasKey, setHasKey] = createSignal(false);
  const [clearKey, setClearKey] = createSignal(false);
  const [model, setModel] = createSignal("");
  const [effort, setEffort] = createSignal("");
  const [command, setCommand] = createSignal("");
  const isCommand = () => provider() === "command";
  const [models, setModels] = createSignal<string[]>([]);
  const [report, setReport] = createSignal<Report>();
  const [loaded, setLoaded] = createSignal(false);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  onMount(async () => {
    const result = await client.GET("/api/v1/system/assistant").catch(() => undefined);
    if (!result?.data) {
      setError("Could not load the assistant settings.");
      return;
    }
    const s = result.data;
    setEnabled(s.enabled);
    setProvider(s.provider);
    setBaseUrl(s.baseUrl);
    setHasKey(s.hasApiKey);
    setModel(s.model);
    setEffort(s.effort ?? "");
    setCommand(s.command ?? "");
    const configured = s.provider === "command" || s.baseUrl;
    setPreset(configured ? presetFor(s.provider, s.baseUrl, s.command) : "anthropic");
    if (!configured) choosePreset("anthropic");
    setLoaded(true);
  });

  const local = () => PRESETS.find((p) => p.id === preset())?.local === true;

  function choosePreset(id: string) {
    setPreset(id);
    setModels([]);
    setReport(undefined);
    const p = PRESETS.find((x) => x.id === id);
    if (p) {
      setProvider(p.provider);
      setBaseUrl(p.baseUrl);
      // A model name from another provider would not work here.
      setModel(p.model);
      if (p.command) setCommand(p.command);
    } else {
      setProvider(
        id === "custom-anthropic"
          ? "anthropic"
          : id === "custom-command"
            ? "command"
            : "openai_compatible",
      );
    }
  }

  const connection = () => ({
    provider: provider(),
    baseUrl: baseUrl().trim(),
    apiKey: apiKey().trim() || undefined,
    model: model().trim(),
    effort: effort() || undefined,
    command: command().trim() || undefined,
  });

  const fetchModels = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.POST("/api/v1/system/assistant/models", { body: connection() });
      if (!result.data) throw new Error(serverMessage(result.error, "Could not fetch models."));
      setModels(result.data);
      if (!model() && result.data.length > 0) setModel(result.data[0]);
      setNotice(`Found ${result.data.length} models.`);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not fetch models.");
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError("");
    setNotice("");
    setReport(undefined);
    try {
      const result = await client.POST("/api/v1/system/assistant/test", { body: connection() });
      if (!result.data) throw new Error(serverMessage(result.error, "Could not connect."));
      setReport(result.data);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not connect.");
    } finally {
      setBusy(false);
    }
  };

  const save = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PUT("/api/v1/system/assistant", {
        body: { ...connection(), enabled: enabled(), clearApiKey: clearKey() },
      });
      if (!result.data)
        throw new Error(serverMessage(result.error, "Could not save the assistant settings."));
      setHasKey(result.data.hasApiKey);
      setBaseUrl(result.data.baseUrl);
      setApiKey("");
      setClearKey(false);
      setNotice(
        result.data.enabled ? "Saved. The assistant is on." : "Saved. The assistant is off.",
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not save the assistant settings.");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      style={{
        background: "var(--bg-surface)",
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-lg)",
        padding: "24px",
        "margin-bottom": "24px",
      }}
    >
      <div
        style={{ display: "flex", "align-items": "center", gap: "10px", "margin-bottom": "12px" }}
      >
        <span style={{ "font-size": "24px" }}>🤖</span>
        <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>AI assistant</h2>
        <Badge variant={enabled() ? "success" : "default"}>{enabled() ? "On" : "Off"}</Badge>
      </div>
      <p
        style={{
          "font-size": "13px",
          color: "var(--text-secondary)",
          "line-height": "1.6",
          "margin-bottom": "16px",
        }}
      >
        The built-in assistant answers in the co-watching chat using the language model you choose
        here. Titles, watch history and the dialogue on screen are sent to that provider. Choose a
        local model if nothing should leave your home network.
      </p>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)", margin: "0 0 12px" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ margin: "0 0 12px" }}>
          {notice()}
        </p>
      </Show>
      <Show when={loaded()}>
        <form onSubmit={save}>
          <label style={label} for="assistant-preset">
            Provider
          </label>
          <select
            id="assistant-preset"
            value={preset()}
            onChange={(e) => choosePreset(e.currentTarget.value)}
            style={field}
          >
            <For each={PRESETS}>{(p) => <option value={p.id}>{p.brand}</option>}</For>
            <option value="custom">Other OpenAI-compatible server</option>
            <option value="custom-anthropic">Other Anthropic-compatible server</option>
            <option value="custom-command">Other agent program</option>
          </select>

          <Show
            when={!isCommand()}
            fallback={
              <>
                <label style={label} for="assistant-command">
                  Command
                </label>
                <input
                  id="assistant-command"
                  value={command()}
                  onInput={(e) => setCommand(e.currentTarget.value)}
                  spellcheck={false}
                  style={{ ...field, "font-family": "var(--font-mono, monospace)" }}
                />
                <p
                  style={{
                    "font-size": "12px",
                    color: "var(--text-secondary)",
                    "line-height": "1.5",
                    margin: "-6px 0 14px",
                  }}
                >
                  The program gets the question on standard input and prints the answer. It runs on
                  the server as the server's user, without a shell, with a temporary read-only token
                  for each answer in MBL_MCP_URL and MBL_MCP_TOKEN. This placeholder becomes the
                  path of an MCP configuration with the token: <code>{"{mcp_config}"}</code>
                </p>
              </>
            }
          >
            <label style={label} for="assistant-url">
              Address
            </label>
            <input
              id="assistant-url"
              type="url"
              value={baseUrl()}
              onInput={(e) => {
                setBaseUrl(e.currentTarget.value);
                setPreset(presetFor(provider(), e.currentTarget.value.trim()));
              }}
              style={field}
            />

            <label style={label} for="assistant-key">
              API key
            </label>
            <input
              id="assistant-key"
              type="password"
              autocomplete="off"
              value={apiKey()}
              onInput={(e) => setApiKey(e.currentTarget.value)}
              placeholder={
                hasKey()
                  ? "Saved – leave empty to keep it"
                  : local()
                    ? "Not needed for a local server"
                    : ""
              }
              style={field}
            />
            <Show when={hasKey()}>
              <label style={{ display: "flex", gap: "8px", "margin-bottom": "14px" }}>
                <input
                  type="checkbox"
                  checked={clearKey()}
                  onChange={(e) => setClearKey(e.currentTarget.checked)}
                />
                Remove the saved key
              </label>
            </Show>

            <label style={label} for="assistant-model">
              Model
            </label>
            <div style={{ display: "flex", gap: "8px", "margin-bottom": "14px" }}>
              <input
                id="assistant-model"
                list="assistant-models"
                value={model()}
                onInput={(e) => setModel(e.currentTarget.value)}
                style={{ flex: 1, "min-width": 0 }}
              />
              <datalist id="assistant-models">
                <For each={models()}>{(m) => <option value={m} />}</For>
              </datalist>
              <Button type="button" variant="secondary" disabled={busy()} onClick={fetchModels}>
                Fetch models
              </Button>
            </div>

            <label style={label} for="assistant-effort">
              Effort
            </label>
            <select
              id="assistant-effort"
              value={effort()}
              onChange={(e) => setEffort(e.currentTarget.value)}
              style={field}
            >
              <option value="">Provider default</option>
              <option value="low">Low</option>
              <option value="medium">Medium</option>
              <option value="high">High</option>
            </select>
          </Show>

          <label style={{ display: "flex", gap: "8px", "margin-bottom": "16px" }}>
            <input
              type="checkbox"
              checked={enabled()}
              onChange={(e) => setEnabled(e.currentTarget.checked)}
            />
            Turn on the assistant
          </label>

          <Show when={report()}>
            {(r) => (
              <div role="status" style={{ "margin-bottom": "16px", "font-size": "13px" }}>
                <Show
                  when={r().ok}
                  fallback={
                    <p style={{ color: "var(--danger)", margin: 0 }}>
                      {`Could not connect: ${r().error ?? ""}`}
                    </p>
                  }
                >
                  <p style={{ margin: "0 0 4px" }}>
                    {`Connected to ${r().model} in ${r().latencyMs} ms. Reply: ${r().reply ?? ""}`}
                  </p>
                  <p style={{ margin: 0 }}>
                    {r().toolsSupported
                      ? "Tools work: the assistant can look things up in your library."
                      : "This model did not use tools. The assistant can still answer, but cannot look things up in your library."}
                  </p>
                </Show>
              </div>
            )}
          </Show>

          <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
            <Button type="button" variant="secondary" disabled={busy()} onClick={test}>
              Test connection
            </Button>
            <Button type="submit" disabled={busy()}>
              Save
            </Button>
          </div>
        </form>
      </Show>
    </div>
  );
}
