import { createSignal, For, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Badge, Button } from "./ui";

type Kind = "radarr" | "sonarr" | "lidarr";
type Report = components["schemas"]["ArrConnectionReport"];

const NAMES: Record<Kind, { name: string; toggle: string; savedOn: string; example: string }> = {
  radarr: {
    name: "Radarr",
    toggle: "Send approved movies to Radarr",
    savedOn: "Saved. Approved movies are sent to Radarr.",
    example: "http://192.168.1.10:7878",
  },
  sonarr: {
    name: "Sonarr",
    toggle: "Send approved TV shows to Sonarr",
    savedOn: "Saved. Approved TV shows are sent to Sonarr.",
    example: "http://192.168.1.10:8989",
  },
  lidarr: {
    name: "Lidarr",
    toggle: "Send approved music to Lidarr",
    savedOn: "Saved. Approved music is sent to Lidarr.",
    example: "http://192.168.1.10:8686",
  },
};

const field = {
  display: "grid",
  gap: "6px",
  "font-size": "var(--font-size-xs)",
  "font-weight": "var(--font-weight-semibold)",
  color: "var(--text-secondary)",
} as const;

const reason = (value: unknown, fallback: string) =>
  (value as { error?: { message?: string } } | undefined)?.error?.message?.replace(
    /^(external service|validation|unavailable|conflict): /,
    "",
  ) || fallback;

/** Address, API key, root folder and quality profile for one download service. */
function ArrService(props: { kind: Kind }) {
  const info = NAMES[props.kind];
  const [enabled, setEnabled] = createSignal(false);
  const [url, setUrl] = createSignal("");
  const [apiKey, setApiKey] = createSignal("");
  const [hasKey, setHasKey] = createSignal(false);
  const [root, setRoot] = createSignal("");
  const [profile, setProfile] = createSignal<number | null>(null);
  const [metadata, setMetadata] = createSignal<number | null>(null);
  const [report, setReport] = createSignal<Report | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  onMount(async () => {
    const result = await client.GET("/api/v1/requests/arr/{service}", {
      params: { path: { service: props.kind } },
    });
    if (!result.data) {
      setError(`Could not load ${info.name} settings.`);
      return;
    }
    setEnabled(result.data.enabled === true);
    setUrl(result.data.url ?? "");
    setHasKey(result.data.hasApiKey === true);
    setRoot(result.data.rootFolder ?? "");
    setProfile(result.data.qualityProfileId ?? null);
    setMetadata(result.data.metadataProfileId ?? null);
  });

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Request failed.");
    } finally {
      setBusy(false);
    }
  };

  const test = () =>
    run(async () => {
      const result = await client.POST("/api/v1/requests/arr/{service}/test", {
        params: { path: { service: props.kind } },
        body: { url: url().trim(), apiKey: apiKey().trim() || undefined },
      });
      if (!result.data) throw new Error(reason(result.error, `Could not reach ${info.name}.`));
      setReport(result.data);
      if (!root() && result.data.rootFolders[0]) setRoot(result.data.rootFolders[0]);
      if (profile() === null && result.data.qualityProfiles[0])
        setProfile(result.data.qualityProfiles[0].id);
      if (metadata() === null && result.data.metadataProfiles?.[0])
        setMetadata(result.data.metadataProfiles[0].id);
      const version = result.data.version ? ` ${result.data.version}` : "";
      setNotice(`Connected to ${info.name}${version}. Choose a folder and profile, then save.`);
    });

  const save = () =>
    run(async () => {
      const result = await client.PUT("/api/v1/requests/arr/{service}", {
        params: { path: { service: props.kind } },
        body: {
          enabled: enabled(),
          url: url().trim(),
          apiKey: apiKey().trim() || undefined,
          rootFolder: root(),
          qualityProfileId: profile() ?? undefined,
          metadataProfileId: props.kind === "lidarr" ? (metadata() ?? undefined) : undefined,
        },
      });
      if (!result.data) throw new Error(reason(result.error, "Could not save settings."));
      setHasKey(result.data.hasApiKey);
      setApiKey("");
      setNotice(result.data.enabled ? info.savedOn : `Saved. ${info.name} is turned off.`);
    });

  return (
    <section
      style={{
        display: "grid",
        gap: "12px",
        padding: "14px",
        border: "1px solid var(--border)",
        "border-radius": "var(--radius-md)",
      }}
    >
      <div style={{ display: "flex", gap: "8px", "align-items": "center" }}>
        <h3 style={{ margin: 0, "font-size": "16px" }}>{info.name}</h3>
        <Badge variant={enabled() ? "success" : "default"}>{enabled() ? "Active" : "Off"}</Badge>
      </div>
      <Show when={error()}>
        <p role="alert" style={{ margin: 0, color: "var(--danger)" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status" style={{ margin: 0 }}>
          {notice()}
        </p>
      </Show>
      <label style={field}>
        <span>Address</span>
        <input
          type="url"
          placeholder={info.example}
          value={url()}
          onInput={(e) => setUrl(e.currentTarget.value)}
        />
      </label>
      <label style={field}>
        <span>API key</span>
        <input
          type="password"
          autocomplete="off"
          placeholder={hasKey() ? "Saved (leave empty to keep)" : "Settings → General → API Key"}
          value={apiKey()}
          onInput={(e) => setApiKey(e.currentTarget.value)}
        />
      </label>
      <div>
        <Button size="sm" variant="secondary" disabled={busy() || !url().trim()} onClick={test}>
          Test connection
        </Button>
      </div>
      <label style={field}>
        <span>Root folder</span>
        <Show
          when={report()?.rootFolders.length}
          fallback={
            <input
              placeholder="Test the connection to list folders"
              value={root()}
              onInput={(e) => setRoot(e.currentTarget.value)}
            />
          }
        >
          <select value={root()} onChange={(e) => setRoot(e.currentTarget.value)}>
            <For each={report()?.rootFolders}>{(f) => <option value={f}>{f}</option>}</For>
          </select>
        </Show>
      </label>
      <label style={field}>
        <span>Quality profile</span>
        <Show
          when={report()?.qualityProfiles.length}
          fallback={
            <input
              type="number"
              min="1"
              placeholder="Test the connection to list profiles"
              value={profile() ?? ""}
              onInput={(e) =>
                setProfile(e.currentTarget.value ? Number(e.currentTarget.value) : null)
              }
            />
          }
        >
          <select
            value={String(profile() ?? "")}
            onChange={(e) => setProfile(Number(e.currentTarget.value))}
          >
            <For each={report()?.qualityProfiles}>
              {(p) => <option value={String(p.id)}>{p.name}</option>}
            </For>
          </select>
        </Show>
      </label>
      <Show when={props.kind === "lidarr"}>
        <label style={field}>
          <span>Metadata profile</span>
          <Show
            when={report()?.metadataProfiles?.length}
            fallback={
              <input
                type="number"
                min="1"
                placeholder="Test the connection to list profiles"
                value={metadata() ?? ""}
                onInput={(e) =>
                  setMetadata(e.currentTarget.value ? Number(e.currentTarget.value) : null)
                }
              />
            }
          >
            <select
              value={String(metadata() ?? "")}
              onChange={(e) => setMetadata(Number(e.currentTarget.value))}
            >
              <For each={report()?.metadataProfiles}>
                {(p) => <option value={String(p.id)}>{p.name}</option>}
              </For>
            </select>
          </Show>
        </label>
      </Show>
      <label style={{ display: "flex", gap: "8px", "font-size": "var(--font-size-sm)" }}>
        <input
          type="checkbox"
          checked={enabled()}
          onChange={(e) => setEnabled(e.currentTarget.checked)}
        />
        <span>{info.toggle}</span>
      </label>
      <div>
        <Button size="sm" disabled={busy()} onClick={save}>
          Save
        </Button>
      </div>
    </section>
  );
}

/** Admin settings for handing approved requests to Radarr, Sonarr and Lidarr. */
export default function ArrSettings() {
  return (
    <details class="mbl-card-section" style={{ "margin-bottom": "20px" }}>
      <summary style={{ cursor: "pointer", "font-weight": "600" }}>
        Radarr, Sonarr and Lidarr
      </summary>
      <p style={{ "font-size": "var(--font-size-sm)", color: "var(--text-secondary)" }}>
        When a service is on, approving a request adds the title there and starts a search. The
        request is marked available once the title shows up in a library.
      </p>
      <div style={{ display: "grid", gap: "14px" }}>
        <ArrService kind="radarr" />
        <ArrService kind="sonarr" />
        <ArrService kind="lidarr" />
      </div>
    </details>
  );
}
