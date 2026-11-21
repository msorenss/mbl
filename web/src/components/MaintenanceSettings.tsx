import { createSignal, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button, Switch } from "./ui";

type Settings = components["schemas"]["MaintenanceSettings"];
type Report = components["schemas"]["MaintenanceReport"];

function size(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

/** What a run did or would do, in a few lines. */
function ReportView(props: { report: Report }) {
  const r = () => props.report;
  return (
    <div style={{ "font-size": "14px", display: "grid", gap: "4px" }}>
      <div>{`Database ${size(r().databaseBytes)}, of which ${size(r().unusedBytes)} unused.`}</div>
      <div>
        {r().dryRun
          ? `Would remove ${r().goneItems} titles gone for ${r().goneDays}+ days and ${r().emptyItems} empty seasons or shows.`
          : r().cleaned
            ? `Removed ${r().goneItems} titles and ${r().emptyItems} empty seasons or shows.`
            : `${r().goneItems} titles have been gone for ${r().goneDays}+ days (not removed).`}
      </div>
      <Show when={r().examples.length > 0}>
        <div style={{ color: "var(--text-secondary)" }}>{r().examples.join(", ")}</div>
      </Show>
      <div>{`${r().oldJobs} finished jobs older than 30 days.`}</div>
      <Show when={r().backup}>
        <div>{`Backup: ${r().backup}`}</div>
      </Show>
      <Show when={r().unreachableLibraries.length > 0}>
        <div style={{ color: "var(--warning, var(--text-secondary))" }}>
          {`Left alone, folders could not be read: ${r().unreachableLibraries.join(", ")}`}
        </div>
      </Show>
    </div>
  );
}

/** Server settings: scheduled database upkeep, backups and cleanup of titles whose files are gone. */
export default function MaintenanceSettings() {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [report, setReport] = createSignal<Report | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  const [notice, setNotice] = createSignal("");

  onMount(async () => {
    try {
      const result = await client.GET("/api/v1/system/maintenance");
      if (result.data) setSettings(result.data);
    } catch {
      setError("Could not load the maintenance settings.");
    }
  });

  const update = (patch: Partial<Settings>) => {
    const current = settings();
    if (current) setSettings({ ...current, ...patch });
  };

  const save = async () => {
    const s = settings();
    if (!s) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.PUT("/api/v1/system/maintenance", {
        body: {
          enabled: s.enabled,
          hour: s.hour,
          intervalDays: s.intervalDays,
          keepBackups: s.keepBackups,
          cleanupAfterDays: s.cleanupAfterDays,
          // The hour is the admin's local time.
          utcOffsetMinutes: -new Date().getTimezoneOffset(),
        },
      });
      if (!result.data) throw new Error();
      setSettings(result.data);
      setNotice("Saved.");
    } catch {
      setError("Could not save. Check the values.");
    } finally {
      setBusy(false);
    }
  };

  const run = async (dryRun: boolean, cleanup: boolean) => {
    if (cleanup && !dryRun) {
      const last = report();
      const count = last?.dryRun ? last.goneItems + last.emptyItems : null;
      const question =
        count === null
          ? "Remove the titles whose files are gone? This cannot be undone."
          : `Remove ${count} titles? This cannot be undone.`;
      if (!window.confirm(question)) return;
    }
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await client.POST("/api/v1/system/maintenance/run", {
        body: { dryRun, cleanup },
      });
      if (!result.data) throw new Error();
      setReport(result.data);
    } catch {
      setError("Maintenance did not finish. It may already be running.");
    } finally {
      setBusy(false);
    }
  };

  const numberInput = (value: number, min: number, max: number, set: (n: number) => void) => (
    <input
      type="number"
      min={min}
      max={max}
      value={value}
      onInput={(e) => set(Number(e.currentTarget.value))}
      style={{ width: "5em" }}
    />
  );

  return (
    <section class="mbl-card-section">
      <h2>Database maintenance</h2>
      <p style={{ color: "var(--text-secondary)" }}>
        Keeps the database fast and small, makes automatic backups and removes old finished jobs.
        Titles whose files are gone are only removed when you ask, or automatically after the number
        of days you choose. Libraries whose folders cannot be read (for example an unplugged disk)
        are left alone.
      </p>
      <Show when={settings()}>
        {(s) => (
          <div style={{ display: "grid", gap: "8px", "max-width": "520px" }}>
            <Switch
              checked={s().enabled}
              onChange={(enabled: boolean) => update({ enabled })}
              label="Run automatically"
            />
            <label>Hour (0–23) {numberInput(s().hour, 0, 23, (hour) => update({ hour }))}</label>
            <label>
              Days between runs{" "}
              {numberInput(s().intervalDays, 1, 30, (intervalDays) => update({ intervalDays }))}
            </label>
            <label>
              Automatic backups to keep{" "}
              {numberInput(s().keepBackups, 0, 60, (keepBackups) => update({ keepBackups }))}
            </label>
            <label>
              Remove titles gone for (days, 0 = never automatically){" "}
              {numberInput(s().cleanupAfterDays, 0, 365, (cleanupAfterDays) =>
                update({ cleanupAfterDays }),
              )}
            </label>
            <Show when={s().lastRun}>
              {(at) => (
                <div style={{ color: "var(--text-secondary)", "font-size": "13px" }}>
                  {`Last run ${new Date(at()).toLocaleString()}`}
                </div>
              )}
            </Show>
            <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap" }}>
              <Button onClick={() => void save()} disabled={busy()}>
                Save
              </Button>
              <Button variant="secondary" onClick={() => void run(true, true)} disabled={busy()}>
                Preview
              </Button>
              <Button variant="secondary" onClick={() => void run(false, false)} disabled={busy()}>
                Run now
              </Button>
              <Show
                when={
                  report()?.dryRun && (report()?.goneItems ?? 0) + (report()?.emptyItems ?? 0) > 0
                }
              >
                <Button variant="secondary" onClick={() => void run(false, true)} disabled={busy()}>
                  Remove gone titles
                </Button>
              </Show>
            </div>
          </div>
        )}
      </Show>
      <Show when={report() ?? settings()?.lastReport}>
        {(r) => (
          <div style={{ "margin-top": "12px" }}>
            <ReportView report={r()} />
          </div>
        )}
      </Show>
      <Show when={error()}>
        <p role="alert" style={{ color: "var(--danger)" }}>
          {error()}
        </p>
      </Show>
      <Show when={notice()}>
        <p role="status">{notice()}</p>
      </Show>
    </section>
  );
}
