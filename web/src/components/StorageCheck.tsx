import { createSignal, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "./ui";

type Report = components["schemas"]["StorageHardlinkReport"];

export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
}

/** Admin check of how much disk space deleting library files would actually free. */
export default function StorageCheck() {
  const [report, setReport] = createSignal<Report | null>(null);
  const [error, setError] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const run = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await client.GET("/api/v1/requests/hardlink-analysis");
      if (!result.data) {
        setError("Could not analyse storage.");
        return;
      }
      setReport(result.data);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <section class="mbl-card-section" aria-label="Storage check">
      <h2 style={{ "font-size": "1.1rem" }}>Storage check</h2>
      <p style={{ color: "var(--color-text-muted)" }}>
        Files that Radarr or Sonarr hardlinked from a download folder use the same disk space twice
        over. Deleting only the library copy frees nothing.
      </p>
      <Button variant="secondary" onClick={run} disabled={loading()}>
        {loading() ? "Checking…" : "Check storage"}
      </Button>
      <Show when={report()}>
        {(r) => (
          <ul>
            <li>{`${r().totalScannedFiles} files, ${formatBytes(r().totalFileSizeBytes)}`}</li>
            <li>{`${r().hardlinkSharedFilesCount} files are hardlinked elsewhere`}</li>
            <li>
              {`Deleting all files would free ${formatBytes(r().estimatedNetFreedBytesIfDeleted)}`}
            </li>
          </ul>
        )}
      </Show>
      <Show when={error()}>
        <p role="alert">{error()}</p>
      </Show>
    </section>
  );
}
