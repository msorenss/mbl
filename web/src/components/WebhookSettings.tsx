import { createResource, createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { useI18n } from "../i18n";
import { Badge, Button } from "./ui";

type Webhook = components["schemas"]["WebhookRow"];

const EVENTS: { id: string; label: string }[] = [
  { id: "playback.started", label: "Playback started" },
  { id: "playback.stopped", label: "Playback stopped" },
  { id: "media.added", label: "New media added" },
  { id: "library.scan_completed", label: "Library scan finished" },
  { id: "library.unavailable", label: "Library folder unavailable" },
];

const label: Record<string, string> = Object.fromEntries([
  ["*", "All events"],
  ...EVENTS.map((e) => [e.id, e.label]),
]);

const field = {
  display: "grid",
  gap: "6px",
  "font-size": "var(--font-size-xs)",
  "font-weight": "var(--font-weight-semibold)",
  color: "var(--text-secondary)",
} as const;

/** Administration of outgoing webhooks (Home Assistant, Discord, scripts…). */
export default function WebhookSettings() {
  const { t } = useI18n();
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [name, setName] = createSignal("");
  const [url, setUrl] = createSignal("");
  const [secret, setSecret] = createSignal("");
  const [events, setEvents] = createSignal<string[]>(["playback.started", "playback.stopped"]);

  const [hooks, { refetch }] = createResource(async () => {
    const result = await client.GET("/api/v1/webhooks");
    if (!result.data) throw new Error("Could not load webhooks.");
    return result.data;
  });

  const run = async (action: () => Promise<void>) => {
    setError("");
    setNotice("");
    setBusy(true);
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  };

  const serverMessage = (value: unknown, fallback: string) =>
    (value as { error?: { message?: string } } | undefined)?.error?.message || fallback;

  const create = () =>
    run(async () => {
      if (events().length === 0) throw new Error("Choose at least one event.");
      const result = await client.POST("/api/v1/webhooks", {
        body: {
          name: name().trim(),
          url: url().trim(),
          events: events(),
          secret: secret().trim() || undefined,
        },
      });
      if (!result.data) throw new Error(serverMessage(result.error, "Could not add the webhook."));
      setName("");
      setUrl("");
      setSecret("");
      setNotice("Webhook added. Use Test to check that the receiver answers.");
      await refetch();
    });

  const toggle = (hook: Webhook) =>
    run(async () => {
      const result = await client.PATCH("/api/v1/webhooks/{id}", {
        params: { path: { id: hook.id } },
        body: { enabled: !hook.enabled },
      });
      if (result.response?.status !== 204) throw new Error("Could not update the webhook.");
      await refetch();
    });

  const remove = (hook: Webhook) =>
    run(async () => {
      if (!window.confirm(`Delete the webhook "${hook.name}"?`)) return;
      const result = await client.DELETE("/api/v1/webhooks/{id}", {
        params: { path: { id: hook.id } },
      });
      if (result.response?.status !== 204) throw new Error("Could not delete the webhook.");
      await refetch();
    });

  const test = (hook: Webhook) =>
    run(async () => {
      const result = await client.POST("/api/v1/webhooks/{id}/test", {
        params: { path: { id: hook.id } },
      });
      if (!result.data) throw new Error("Could not send the test.");
      if (result.data.ok) setNotice(`"${hook.name}" answered ${result.data.status}.`);
      else throw new Error(`"${hook.name}": ${result.data.error ?? "no answer"}`);
    });

  return (
    <section class="mbl-card-section" style={{ display: "grid", gap: "14px" }}>
      <h2 style={{ margin: 0, "font-size": "18px" }}>Webhooks</h2>
      <p style={{ margin: 0, "font-size": "var(--font-size-sm)", color: "var(--text-secondary)" }}>
        MBL sends a JSON POST to each address when the chosen events happen. With a secret, the body
        is signed in the <code>X-MBL-Signature</code> header (<code>sha256=</code> HMAC).
      </p>
      <Show when={error()}>
        <p role="alert" style={{ margin: 0, color: "var(--danger, #c0392b)" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ margin: 0 }}>
          {notice()}
        </p>
      </Show>
      <Show when={hooks.error}>
        <p role="alert" style={{ margin: 0 }}>
          Could not load webhooks.
        </p>
      </Show>
      <ul style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "10px" }}>
        <For each={hooks() ?? []}>
          {(hook) => (
            <li
              style={{
                display: "grid",
                gap: "6px",
                padding: "10px",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-md)",
              }}
            >
              <div
                style={{
                  display: "flex",
                  gap: "8px",
                  "align-items": "center",
                  "flex-wrap": "wrap",
                }}
              >
                <strong>{hook.name}</strong>
                <Badge variant={hook.enabled ? "success" : "default"}>
                  {hook.enabled ? "Active" : "Paused"}
                </Badge>
                <Show when={hook.secret}>
                  <Badge variant="info">Signed</Badge>
                </Show>
              </div>
              <span style={{ "font-size": "var(--font-size-sm)", "overflow-wrap": "anywhere" }}>
                {hook.url}
              </span>
              <span style={{ "font-size": "var(--font-size-xs)", color: "var(--text-secondary)" }}>
                {hook.events.map((e) => t(label[e] ?? e)).join(", ")}
              </span>
              <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
                <Button size="sm" variant="secondary" disabled={busy()} onClick={() => test(hook)}>
                  Test
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy()}
                  onClick={() => toggle(hook)}
                >
                  {hook.enabled ? "Pause" : "Resume"}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy()}
                  onClick={() => remove(hook)}
                >
                  Delete
                </Button>
              </div>
            </li>
          )}
        </For>
      </ul>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void create();
        }}
        style={{ display: "grid", gap: "12px" }}
      >
        <h3 style={{ margin: 0, "font-size": "15px" }}>Add webhook</h3>
        <label style={field}>
          <span>Name</span>
          <input required value={name()} onInput={(e) => setName(e.currentTarget.value)} />
        </label>
        <label style={field}>
          <span>URL</span>
          <input
            type="url"
            required
            placeholder="http://homeassistant.local:8123/api/webhook/mbl"
            value={url()}
            onInput={(e) => setUrl(e.currentTarget.value)}
          />
        </label>
        <label style={field}>
          <span>Secret (optional)</span>
          <input
            type="password"
            autocomplete="new-password"
            value={secret()}
            onInput={(e) => setSecret(e.currentTarget.value)}
          />
        </label>
        <fieldset style={{ display: "grid", gap: "6px" }}>
          <legend style={{ "font-size": "var(--font-size-xs)", "font-weight": "600" }}>
            Events
          </legend>
          <For each={EVENTS}>
            {(event) => (
              <label style={{ display: "flex", gap: "8px", "font-size": "var(--font-size-sm)" }}>
                <input
                  type="checkbox"
                  checked={events().includes(event.id)}
                  onChange={(e) =>
                    setEvents((current) =>
                      e.currentTarget.checked
                        ? [...current, event.id]
                        : current.filter((id) => id !== event.id),
                    )
                  }
                />
                <span>{event.label}</span>
              </label>
            )}
          </For>
        </fieldset>
        <div>
          <Button type="submit" disabled={busy()}>
            Add webhook
          </Button>
        </div>
      </form>
    </section>
  );
}
