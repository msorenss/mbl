import { createSignal, For, onMount, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import { useToast } from "../../context/ToastContext";

type ApiTokenInfo = components["schemas"]["ApiTokenInfo"];
type McpAuditEntry = components["schemas"]["McpAuditEntry"];
type McpPreset = components["schemas"]["McpPreset"];
type McpStatus = components["schemas"]["McpStatus"];
type McpConfigSnippets = components["schemas"]["McpConfigSnippets"];

export function AiAgents() {
  const { showToast } = useToast();

  // Status & Kill switch
  const [mcpStatus, setMcpStatus] = createSignal<McpStatus | null>(null);
  const [togglingKillSwitch, setTogglingKillSwitch] = createSignal(false);

  // Active Tokens
  const [tokens, setTokens] = createSignal<ApiTokenInfo[]>([]);
  const [loadingTokens, setLoadingTokens] = createSignal(true);

  // Presets
  const [presets, setPresets] = createSignal<McpPreset[]>([]);

  // Token Wizard state
  const [wizardOpen, setWizardOpen] = createSignal(false);
  const [tokenName, setTokenName] = createSignal("");
  const [selectedPreset, setSelectedPreset] = createSignal("read_only");
  const [customScopes, setCustomScopes] = createSignal<string[]>(["read"]);
  const [isCustomScopes, setIsCustomScopes] = createSignal(false);
  const [expiryDays, setExpiryDays] = createSignal<number>(90);
  const [allowedNetworks, setAllowedNetworks] = createSignal<string[]>(["lan", "tailnet"]);
  const [mintingToken, setMintingToken] = createSignal(false);

  // Newly minted token modal
  const [newPlaintextToken, setNewPlaintextToken] = createSignal<string | null>(null);
  const [snippets, setSnippets] = createSignal<McpConfigSnippets | null>(null);
  const [snippetTab, setSnippetTab] = createSignal<"vscode" | "claudeDesktop" | "cursor" | "stdio">(
    "vscode",
  );

  // Audit Log state
  const [auditEntries, setAuditEntries] = createSignal<McpAuditEntry[]>([]);
  const [loadingAudit, setLoadingAudit] = createSignal(true);
  const [filterTool, setFilterTool] = createSignal("");
  const [filterOutcome, setFilterOutcome] = createSignal("");
  const [filterTokenId, setFilterTokenId] = createSignal<string>("");

  const allAvailableScopes = ["read", "playback", "library", "users", "remote", "livetv", "admin"];

  const loadStatus = async () => {
    try {
      const { data, error } = await client.GET("/api/v1/mcp/status");
      if (error) throw new Error("Could not load MCP status");
      if (data) setMcpStatus(data);
    } catch {
      showToast({
        title: "Load failed",
        message: "Server data could not be loaded. Reload to retry.",
        type: "error",
      });
    }
  };

  const loadPresets = async () => {
    try {
      const { data, error } = await client.GET("/api/v1/mcp/presets");
      if (error) throw new Error("Could not load presets");
      if (data) setPresets(data);
    } catch {
      showToast({
        title: "Load failed",
        message: "Server data could not be loaded. Reload to retry.",
        type: "error",
      });
    }
  };

  const loadTokens = async () => {
    setLoadingTokens(true);
    try {
      const { data, error } = await client.GET("/api/v1/auth/tokens", {
        params: { query: { all: true } },
      });
      if (error) throw new Error("Could not load tokens");
      if (data) setTokens(data);
    } catch {
      showToast({
        title: "Load failed",
        message: "Server data could not be loaded. Reload to retry.",
        type: "error",
      });
    } finally {
      setLoadingTokens(false);
    }
  };

  const loadAudit = async () => {
    setLoadingAudit(true);
    try {
      const query: { tool?: string; outcome?: string; tokenId?: number; limit?: number } = {
        limit: 100,
      };
      if (filterTool()) query.tool = filterTool();
      if (filterOutcome()) query.outcome = filterOutcome();
      if (filterTokenId()) query.tokenId = Number.parseInt(filterTokenId(), 10);

      const { data, error } = await client.GET("/api/v1/mcp/audit", {
        params: { query },
      });
      if (error) throw new Error("Could not load audit log");
      if (data) setAuditEntries(data);
    } catch {
      showToast({
        title: "Load failed",
        message: "Server data could not be loaded. Reload to retry.",
        type: "error",
      });
    } finally {
      setLoadingAudit(false);
    }
  };

  onMount(() => {
    loadStatus();
    loadPresets();
    loadTokens();
    loadAudit();
  });

  const handleToggleKillSwitch = async () => {
    const current = mcpStatus();
    if (!current) return;
    const next = !current.enabled;
    setTogglingKillSwitch(true);

    try {
      const { data, error } = await client.PATCH("/api/v1/mcp/status", {
        body: { enabled: next },
      });
      if (data && !error) {
        setMcpStatus(data);
        showToast({
          title: next ? "MCP Enabled" : "MCP Disabled (Kill Switch)",
          message: next ? "Model Context Protocol is active" : "MCP endpoints now returning 503",
          type: next ? "success" : "error",
        });
      }
    } catch {
      showToast({ title: "Error", message: "Failed to toggle kill switch", type: "error" });
    } finally {
      setTogglingKillSwitch(false);
    }
  };

  const handleRevokeToken = async (id: number) => {
    if (
      !confirm(
        "Are you sure you want to revoke this API token? Any connected agent will be disconnected immediately.",
      )
    ) {
      return;
    }

    try {
      const { error } = await client.DELETE("/api/v1/auth/tokens/{id}", {
        params: { path: { id } },
      });
      if (error) throw new Error("Revoke failed");
      if (!error) {
        showToast({
          title: "Token Revoked",
          message: "API token has been revoked",
          type: "success",
        });
        loadTokens();
      }
    } catch {
      showToast({ title: "Error", message: "Failed to revoke token", type: "error" });
    }
  };

  const handleCreateToken = async (e: Event) => {
    e.preventDefault();
    if (mintingToken()) return;
    if (!allowedNetworks().length) {
      showToast({
        title: "Validation Error",
        message: "Select at least one allowed network",
        type: "error",
      });
      return;
    }
    if (!tokenName().trim()) {
      showToast({ title: "Validation Error", message: "Token name is required", type: "error" });
      return;
    }

    setMintingToken(true);
    let scopes: string[] = [];
    if (isCustomScopes()) {
      scopes = customScopes();
    } else {
      const p = presets().find((x) => x.id === selectedPreset());
      scopes = p ? p.scopes : ["read"];
    }

    const expiresAt = expiryDays() > 0 ? Date.now() + expiryDays() * 24 * 60 * 60 * 1000 : null;

    try {
      const { data, error } = await client.POST("/api/v1/auth/tokens", {
        body: {
          name: tokenName().trim(),
          scopes,
          expiresAt,
          allowedNetworks: allowedNetworks(),
        },
      });

      if (data && !error) {
        setSnippets(null);
        setNewPlaintextToken(data.token);
        setWizardOpen(false);
        setTokenName("");
        loadTokens();

        // Fetch pre-filled snippets
        const snippetResult = await client
          .POST("/api/v1/mcp/snippets", {
            body: {
              serverUrl: window.location.origin,
              token: data.token,
            },
          })
          .catch(() => null);
        if (snippetResult?.data && !snippetResult.error) setSnippets(snippetResult.data);
        else
          showToast({
            title: "Token created",
            message: "Configuration snippets could not be loaded. Copy the token shown above.",
            type: "warning",
          });
      } else {
        showToast({ title: "Error", message: "Failed to create token", type: "error" });
      }
    } catch {
      showToast({ title: "Error", message: "Failed to create token", type: "error" });
    } finally {
      setMintingToken(false);
    }
  };

  const copyToClipboard = async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text);
      showToast({ title: "Copied!", message: `${label} copied to clipboard`, type: "success" });
    } catch {
      showToast({
        title: "Copy failed",
        message: "Select and copy the text manually.",
        type: "error",
      });
    }
  };

  return (
    <div>
      {/* Header and Kill Switch */}
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
            Connect an AI Agent
          </h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            Manage Model Context Protocol (MCP) tokens, IDE integrations, and review audit logs.
          </p>
        </div>

        <div style={{ display: "flex", gap: "12px", "align-items": "center" }}>
          {/* Global Kill Switch */}
          <Show when={mcpStatus()}>
            {(status) => (
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "10px",
                  padding: "6px 14px",
                  background: status().enabled ? "var(--bg-surface)" : "var(--danger-light)",
                  border: `1px solid ${status().enabled ? "var(--border)" : "var(--danger)"}`,
                  "border-radius": "var(--radius-md)",
                }}
              >
                <span style={{ "font-size": "13px", color: "var(--text-secondary)" }}>
                  MCP Kill Switch:
                </span>
                <button
                  type="button"
                  onClick={handleToggleKillSwitch}
                  disabled={togglingKillSwitch()}
                  style={{
                    padding: "4px 10px",
                    "font-size": "12px",
                    "font-weight": "600",
                    "border-radius": "var(--radius-sm)",
                    border: "none",
                    background: status().enabled ? "var(--danger)" : "var(--success)",
                    color: "#fff",
                    cursor: "pointer",
                  }}
                >
                  {status().enabled ? "Disable MCP (503)" : "Enable MCP"}
                </button>
              </div>
            )}
          </Show>

          <button
            type="button"
            onClick={() => setWizardOpen(true)}
            style={{
              padding: "9px 18px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-md)",
              "font-weight": "600",
              "font-size": "14px",
              cursor: "pointer",
              display: "flex",
              "align-items": "center",
              gap: "8px",
            }}
          >
            <span>+</span> Connect New Agent
          </button>
        </div>
      </div>

      {/* New Plaintext Token Modal */}
      <Show when={newPlaintextToken()}>
        <div
          style={{
            margin: "0 0 28px 0",
            padding: "24px",
            background: "var(--bg-surface)",
            border: "1px solid var(--accent)",
            "border-radius": "var(--radius-lg)",
            "box-shadow": "0 10px 25px -5px rgba(0, 0, 0, 0.4)",
          }}
        >
          <div
            style={{
              display: "flex",
              "justify-content": "space-between",
              "align-items": "center",
              "margin-bottom": "12px",
            }}
          >
            <h3 style={{ margin: 0, "font-size": "18px", color: "var(--accent)" }}>
              🔑 New Agent Token Created
            </h3>
            <button
              type="button"
              onClick={() => setNewPlaintextToken(null)}
              style={{
                background: "transparent",
                border: "none",
                color: "var(--text-muted)",
                "font-size": "20px",
                cursor: "pointer",
              }}
            >
              &times;
            </button>
          </div>
          <p style={{ margin: "0 0 16px 0", color: "var(--text-secondary)", "font-size": "13px" }}>
            Make sure to copy your token now. You will not be able to see it again!
          </p>

          <div style={{ display: "flex", gap: "10px", "margin-bottom": "20px" }}>
            <input
              type="text"
              readOnly
              aria-label="New API token"
              value={newPlaintextToken() || ""}
              style={{
                flex: 1,
                padding: "10px 14px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--accent)",
                "font-family": "var(--font-mono)",
                "font-size": "14px",
              }}
            />
            <button
              type="button"
              onClick={() => copyToClipboard(newPlaintextToken() || "", "API Token")}
              style={{
                padding: "10px 16px",
                background: "var(--accent)",
                color: "#fff",
                border: "none",
                "border-radius": "var(--radius-sm)",
                "font-weight": "600",
                cursor: "pointer",
              }}
            >
              Copy Token
            </button>
          </div>

          {/* Config Snippets Tabs */}
          <Show when={snippets()}>
            {(snips) => (
              <div>
                <div style={{ "font-size": "14px", "font-weight": "600", "margin-bottom": "8px" }}>
                  Quick Configuration Snippets:
                </div>
                <div style={{ display: "flex", gap: "8px", "margin-bottom": "12px" }}>
                  {(
                    [
                      ["vscode", "VS Code (.vscode/mcp.json)"],
                      ["claudeDesktop", "Claude Desktop"],
                      ["cursor", "Cursor (.cursor/mcp.json)"],
                      ["stdio", "Generic stdio"],
                    ] as const
                  ).map(([tab, title]) => (
                    <button
                      type="button"
                      onClick={() => setSnippetTab(tab)}
                      style={{
                        padding: "6px 12px",
                        "font-size": "12px",
                        "border-radius": "var(--radius-sm)",
                        border: "1px solid var(--border)",
                        background: snippetTab() === tab ? "var(--accent-light)" : "var(--bg-base)",
                        color: snippetTab() === tab ? "var(--accent)" : "var(--text-secondary)",
                        cursor: "pointer",
                      }}
                    >
                      {title}
                    </button>
                  ))}
                </div>

                <div style={{ position: "relative" }}>
                  <pre
                    style={{
                      padding: "14px",
                      background: "var(--bg-base)",
                      border: "1px solid var(--border)",
                      "border-radius": "var(--radius-sm)",
                      overflow: "auto",
                      "font-family": "var(--font-mono)",
                      "font-size": "12px",
                      color: "var(--text-primary)",
                      margin: 0,
                    }}
                  >
                    {snips()[snippetTab()]}
                  </pre>
                  <button
                    type="button"
                    onClick={() => copyToClipboard(snips()[snippetTab()], "Configuration Snippet")}
                    style={{
                      position: "absolute",
                      top: "8px",
                      right: "8px",
                      padding: "4px 10px",
                      background: "var(--bg-overlay)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                      "border-radius": "var(--radius-sm)",
                      "font-size": "11px",
                      cursor: "pointer",
                    }}
                  >
                    Copy Snippet
                  </button>
                </div>
              </div>
            )}
          </Show>
        </div>
      </Show>

      {/* Token Wizard Dialog */}
      <Show when={wizardOpen()}>
        <div
          style={{
            position: "fixed",
            top: 0,
            left: 0,
            width: "100vw",
            height: "100vh",
            background: "rgba(0, 0, 0, 0.7)",
            display: "flex",
            "justify-content": "center",
            "align-items": "center",
            "z-index": 1000,
          }}
        >
          <div
            style={{
              width: "480px",
              "max-height": "90vh",
              overflow: "auto",
              padding: "28px",
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-lg)",
            }}
          >
            <div
              style={{
                display: "flex",
                "justify-content": "space-between",
                "margin-bottom": "20px",
              }}
            >
              <h2 style={{ margin: 0, "font-size": "20px", "font-weight": "700" }}>
                Connect an AI Agent (Token Wizard)
              </h2>
              <button
                type="button"
                onClick={() => setWizardOpen(false)}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "var(--text-muted)",
                  "font-size": "20px",
                  cursor: "pointer",
                }}
              >
                &times;
              </button>
            </div>

            <form
              onSubmit={handleCreateToken}
              style={{ display: "flex", "flex-direction": "column", gap: "16px" }}
            >
              <div>
                <label
                  for="agentName"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                    color: "var(--text-secondary)",
                  }}
                >
                  Agent / Token Name
                </label>
                <input
                  id="agentName"
                  type="text"
                  placeholder="e.g. Claude Desktop (MacBook)"
                  value={tokenName()}
                  onInput={(e) => setTokenName(e.currentTarget.value)}
                  style={{
                    width: "100%",
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                  }}
                />
              </div>

              <div>
                <div
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                    color: "var(--text-secondary)",
                  }}
                >
                  Permission Preset
                </div>
                <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
                  <For each={presets()}>
                    {(p) => (
                      <label
                        style={{
                          display: "flex",
                          padding: "10px 12px",
                          border: `1px solid ${!isCustomScopes() && selectedPreset() === p.id ? "var(--accent)" : "var(--border)"}`,
                          "border-radius": "var(--radius-sm)",
                          background:
                            !isCustomScopes() && selectedPreset() === p.id
                              ? "var(--accent-light)"
                              : "var(--bg-base)",
                          cursor: "pointer",
                          gap: "10px",
                          "align-items": "flex-start",
                        }}
                      >
                        <input
                          type="radio"
                          name="preset"
                          checked={!isCustomScopes() && selectedPreset() === p.id}
                          onChange={() => {
                            setSelectedPreset(p.id);
                            setIsCustomScopes(false);
                          }}
                        />
                        <div>
                          <div style={{ "font-weight": "600", "font-size": "13px" }}>{p.name}</div>
                          <div style={{ color: "var(--text-muted)", "font-size": "11px" }}>
                            {p.description}
                          </div>
                        </div>
                      </label>
                    )}
                  </For>

                  <label
                    style={{
                      display: "flex",
                      padding: "10px 12px",
                      border: `1px solid ${isCustomScopes() ? "var(--accent)" : "var(--border)"}`,
                      "border-radius": "var(--radius-sm)",
                      background: isCustomScopes() ? "var(--accent-light)" : "var(--bg-base)",
                      cursor: "pointer",
                      gap: "10px",
                      "align-items": "flex-start",
                    }}
                  >
                    <input
                      type="radio"
                      name="preset"
                      checked={isCustomScopes()}
                      onChange={() => setIsCustomScopes(true)}
                    />
                    <div>
                      <div style={{ "font-weight": "600", "font-size": "13px" }}>Custom Scopes</div>
                      <div style={{ color: "var(--text-muted)", "font-size": "11px" }}>
                        Fine-tune individual permissions
                      </div>
                    </div>
                  </label>
                </div>
              </div>

              <Show when={isCustomScopes()}>
                <div
                  style={{
                    padding: "10px",
                    background: "var(--bg-base)",
                    "border-radius": "var(--radius-sm)",
                  }}
                >
                  <div
                    style={{ "font-size": "12px", "font-weight": "600", "margin-bottom": "6px" }}
                  >
                    Select Scopes:
                  </div>
                  <div
                    style={{
                      display: "grid",
                      "grid-template-columns": "repeat(2, 1fr)",
                      gap: "6px",
                    }}
                  >
                    <For each={allAvailableScopes}>
                      {(scope) => (
                        <label
                          style={{
                            display: "flex",
                            "align-items": "center",
                            gap: "6px",
                            "font-size": "12px",
                            cursor: "pointer",
                          }}
                        >
                          <input
                            type="checkbox"
                            checked={customScopes().includes(scope)}
                            onChange={(e) => {
                              if (e.currentTarget.checked) {
                                setCustomScopes([...customScopes(), scope]);
                              } else {
                                setCustomScopes(customScopes().filter((s) => s !== scope));
                              }
                            }}
                          />
                          <span>{scope}</span>
                        </label>
                      )}
                    </For>
                  </div>
                </div>
              </Show>

              <fieldset disabled={mintingToken()}>
                <legend>Allowed networks</legend>
                <p>
                  WAN permits token use from the public internet only when server policy also allows
                  it.
                </p>
                <For
                  each={[
                    { id: "lan", label: "Local network (LAN)" },
                    { id: "tailnet", label: "Tailscale network" },
                    { id: "wan", label: "Public internet (WAN)" },
                  ]}
                >
                  {(network) => (
                    <label style={{ display: "block" }}>
                      <input
                        type="checkbox"
                        checked={allowedNetworks().includes(network.id)}
                        onChange={(event) =>
                          setAllowedNetworks((values) =>
                            event.currentTarget.checked
                              ? [...values, network.id]
                              : values.filter((value) => value !== network.id),
                          )
                        }
                      />
                      {network.label}
                    </label>
                  )}
                </For>
              </fieldset>
              <div>
                <label
                  for="expiry"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                    color: "var(--text-secondary)",
                  }}
                >
                  Expiration
                </label>
                <select
                  id="expiry"
                  value={expiryDays()}
                  onChange={(e) => setExpiryDays(Number.parseInt(e.currentTarget.value, 10))}
                  style={{
                    width: "100%",
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                  }}
                >
                  <option value={7}>7 days</option>
                  <option value={30}>30 days</option>
                  <option value={90}>90 days (Recommended)</option>
                  <option value={365}>1 year</option>
                  <option value={0}>Never expires</option>
                </select>
              </div>

              <div
                style={{
                  display: "flex",
                  "justify-content": "flex-end",
                  gap: "10px",
                  "margin-top": "10px",
                }}
              >
                <button
                  type="button"
                  onClick={() => setWizardOpen(false)}
                  style={{
                    padding: "8px 16px",
                    background: "transparent",
                    border: "1px solid var(--border)",
                    color: "var(--text-secondary)",
                    "border-radius": "var(--radius-sm)",
                    cursor: "pointer",
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={mintingToken()}
                  style={{
                    padding: "8px 18px",
                    background: "var(--accent)",
                    color: "#fff",
                    border: "none",
                    "border-radius": "var(--radius-sm)",
                    "font-weight": "600",
                    cursor: "pointer",
                  }}
                >
                  {mintingToken() ? "Generating..." : "Generate Token"}
                </button>
              </div>
            </form>
          </div>
        </div>
      </Show>

      {/* Active Tokens Table */}
      <div style={{ "margin-bottom": "36px" }}>
        <h2 style={{ "font-size": "18px", "font-weight": "700", "margin-bottom": "14px" }}>
          Active Agent Tokens
        </h2>
        <Show
          when={!loadingTokens()}
          fallback={<div style={{ color: "var(--text-muted)" }}>Loading tokens...</div>}
        >
          <div
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-md)",
              overflow: "hidden",
            }}
          >
            <table
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
                  <th style={{ padding: "12px 16px" }}>Name</th>
                  <th style={{ padding: "12px 16px" }}>Scopes</th>
                  <th style={{ padding: "12px 16px" }}>Created</th>
                  <th style={{ padding: "12px 16px" }}>Networks</th>
                  <th style={{ padding: "12px 16px" }}>Expires</th>
                  <th style={{ padding: "12px 16px" }}>Last Used</th>
                  <th style={{ padding: "12px 16px", "text-align": "right" }}>Actions</th>
                </tr>
              </thead>
              <tbody>
                <For each={tokens()}>
                  {(tok) => (
                    <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                      <td style={{ padding: "12px 16px", "font-weight": "600" }}>{tok.name}</td>
                      <td style={{ padding: "12px 16px" }}>
                        <div style={{ display: "flex", gap: "4px", "flex-wrap": "wrap" }}>
                          <For each={tok.scopes}>
                            {(sc) => (
                              <span
                                style={{
                                  padding: "1px 6px",
                                  "border-radius": "10px",
                                  background: "var(--bg-overlay)",
                                  "font-size": "11px",
                                  color: "var(--text-secondary)",
                                }}
                              >
                                {sc}
                              </span>
                            )}
                          </For>
                        </div>
                      </td>
                      <td style={{ padding: "12px 16px", color: "var(--text-muted)" }}>
                        {new Date(tok.createdAt).toLocaleDateString()}
                      </td>
                      <td style={{ padding: "12px 16px" }}>{tok.allowedNetworks.join(", ")}</td>
                      <td style={{ padding: "12px 16px", color: "var(--text-muted)" }}>
                        {tok.expiresAt ? new Date(tok.expiresAt).toLocaleDateString() : "Never"}
                      </td>
                      <td style={{ padding: "12px 16px", color: "var(--text-muted)" }}>
                        {tok.lastUsedAt ? new Date(tok.lastUsedAt).toLocaleDateString() : "Never"}
                      </td>
                      <td style={{ padding: "12px 16px", "text-align": "right" }}>
                        <button
                          type="button"
                          onClick={() => handleRevokeToken(tok.id)}
                          style={{
                            padding: "4px 8px",
                            background: "transparent",
                            border: "1px solid var(--danger)",
                            color: "var(--danger)",
                            "border-radius": "var(--radius-sm)",
                            "font-size": "12px",
                            cursor: "pointer",
                          }}
                        >
                          Revoke
                        </button>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </div>

      {/* Audit Log Table with Filters */}
      <div>
        <div
          style={{
            display: "flex",
            "justify-content": "space-between",
            "align-items": "center",
            "margin-bottom": "14px",
          }}
        >
          <div>
            <h2 style={{ "font-size": "18px", "font-weight": "700", margin: "0 0 4px 0" }}>
              MCP Audit Log
            </h2>
            <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
              Detailed logs of all Model Context Protocol tool invocations
            </p>
          </div>

          {/* Filters */}
          <div style={{ display: "flex", gap: "10px" }}>
            <input
              type="text"
              placeholder="Token ID..."
              value={filterTokenId()}
              onInput={(e) => {
                setFilterTokenId(e.currentTarget.value);
                loadAudit();
              }}
              style={{
                width: "90px",
                padding: "6px 10px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "12px",
              }}
            />
            <input
              type="text"
              placeholder="Filter tool..."
              value={filterTool()}
              onInput={(e) => {
                setFilterTool(e.currentTarget.value);
                loadAudit();
              }}
              style={{
                padding: "6px 10px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "12px",
              }}
            />
            <select
              value={filterOutcome()}
              onChange={(e) => {
                setFilterOutcome(e.currentTarget.value);
                loadAudit();
              }}
              style={{
                padding: "6px 10px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "12px",
              }}
            >
              <option value="">All Outcomes</option>
              <option value="ok">Success (ok)</option>
              <option value="error">Error</option>
              <option value="forbidden">Forbidden</option>
              <option value="rate_limited">Rate Limited</option>
            </select>
            <button
              type="button"
              onClick={loadAudit}
              style={{
                padding: "6px 12px",
                background: "var(--bg-surface)",
                border: "1px solid var(--border)",
                color: "var(--text-secondary)",
                "border-radius": "var(--radius-sm)",
                cursor: "pointer",
                "font-size": "12px",
              }}
            >
              Refresh
            </button>
          </div>
        </div>

        <Show
          when={!loadingAudit()}
          fallback={<div style={{ color: "var(--text-muted)" }}>Loading audit entries...</div>}
        >
          <div
            style={{
              background: "var(--bg-surface)",
              border: "1px solid var(--border)",
              "border-radius": "var(--radius-md)",
              overflow: "hidden",
            }}
          >
            <table
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
                  <th style={{ padding: "10px 14px" }}>Timestamp</th>
                  <th style={{ padding: "10px 14px" }}>Tool</th>
                  <th style={{ padding: "10px 14px" }}>Token ID</th>
                  <th style={{ padding: "10px 14px" }}>Client</th>
                  <th style={{ padding: "10px 14px" }}>Outcome</th>
                  <th style={{ padding: "10px 14px" }}>Duration</th>
                  <th style={{ padding: "10px 14px" }}>Dry Run</th>
                </tr>
              </thead>
              <tbody>
                <For each={auditEntries()}>
                  {(entry) => (
                    <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                      <td
                        style={{
                          padding: "10px 14px",
                          color: "var(--text-muted)",
                          "white-space": "nowrap",
                        }}
                      >
                        {new Date(entry.ts).toLocaleTimeString()}
                      </td>
                      <td
                        style={{
                          padding: "10px 14px",
                          "font-weight": "600",
                          "font-family": "var(--font-mono)",
                        }}
                      >
                        {entry.tool}
                      </td>
                      <td style={{ padding: "10px 14px", color: "var(--text-secondary)" }}>
                        {entry.tokenId || "session"}
                      </td>
                      <td style={{ padding: "10px 14px", color: "var(--text-muted)" }}>
                        {entry.client || "-"}
                      </td>
                      <td style={{ padding: "10px 14px" }}>
                        <span
                          style={{
                            display: "inline-block",
                            padding: "1px 6px",
                            "border-radius": "10px",
                            "font-size": "11px",
                            "font-weight": "600",
                            background:
                              entry.outcome === "ok"
                                ? "var(--success-light)"
                                : "var(--danger-light)",
                            color: entry.outcome === "ok" ? "var(--success)" : "var(--danger)",
                          }}
                        >
                          {entry.outcome}
                        </span>
                      </td>
                      <td style={{ padding: "10px 14px", color: "var(--text-muted)" }}>
                        {entry.durationMs} ms
                      </td>
                      <td style={{ padding: "10px 14px", color: "var(--text-muted)" }}>
                        {entry.dryRun ? "Yes" : "No"}
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </div>
        </Show>
      </div>
    </div>
  );
}

export default AiAgents;
