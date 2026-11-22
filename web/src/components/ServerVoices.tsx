import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { forgetServerSpeech } from "../utils/speech";
import { Badge, Button } from "./ui";

type Status = components["schemas"]["VoicesStatus"];
type Voice = components["schemas"]["ServerVoice"];

/**
 * Voices for reading replies aloud on the server (Piper), for browsers without voices of their
 * own, such as Chromium on a Raspberry Pi. The program and each voice are downloaded here.
 */
export default function ServerVoices() {
  const [status, setStatus] = createSignal<Status | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const load = async () => {
    try {
      const result = await client.GET("/api/v1/speech/voices");
      if (!result.data) throw new Error();
      setStatus(result.data);
      const d = result.data.download;
      if (d && !d.finished) timer = setTimeout(load, 1500);
      else forgetServerSpeech();
    } catch {
      setError("Could not load the voices.");
    }
  };
  onMount(() => void load());
  onCleanup(() => clearTimeout(timer));

  const start = async (call: () => Promise<{ response: Response }>, question: string) => {
    if (!window.confirm(question)) return;
    setBusy(true);
    setError("");
    try {
      const result = await call();
      if (!result.response.ok) throw new Error();
      await load();
    } catch {
      setError("The download did not start.");
    } finally {
      setBusy(false);
    }
  };

  const remove = async (voice: Voice) => {
    if (!window.confirm(`Remove the voice ${voice.name}?`)) return;
    setBusy(true);
    try {
      const result = await client.DELETE("/api/v1/speech/voices/{language}", {
        params: { path: { language: voice.language } },
      });
      if (!result.response.ok) throw new Error();
      await load();
    } catch {
      setError("Could not remove the voice.");
    } finally {
      setBusy(false);
    }
  };

  const running = () => {
    const d = status()?.download;
    return d && !d.finished ? d : null;
  };
  const progress = () => {
    const d = running();
    return d?.totalBytes
      ? `Downloading ${Math.floor((100 * d.receivedBytes) / d.totalBytes)} %`
      : "Downloading…";
  };

  return (
    <section class="mbl-card-section">
      <h2 style={{ "font-size": "18px", "font-weight": "700", margin: "0 0 8px" }}>
        Voices for reading aloud
      </h2>
      <p style={{ margin: "0 0 12px", color: "var(--text-secondary)" }}>
        Read replies aloud in the chats even in browsers without voices of their own, such as the
        Raspberry Pi's. The server reads with Piper, a small speech program that runs on its
        processor; nothing leaves the server.
      </p>
      <Show when={status()}>
        {(s) => (
          <>
            <Show
              when={s().supported}
              fallback={<p role="status">Piper is not available for this computer.</p>}
            >
              <div
                style={{
                  display: "flex",
                  gap: "12px",
                  "align-items": "center",
                  "margin-bottom": "12px",
                }}
              >
                <strong>Piper</strong>
                <Show
                  when={s().programInstalled}
                  fallback={
                    <Show
                      when={running()?.what === "program"}
                      fallback={
                        <Button
                          size="sm"
                          disabled={busy() || Boolean(running())}
                          onClick={() =>
                            void start(
                              () => client.POST("/api/v1/speech/piper"),
                              "Download Piper (about 25 MB) from GitHub?",
                            )
                          }
                        >
                          Download
                        </Button>
                      }
                    >
                      <span role="status">{progress()}</span>
                    </Show>
                  }
                >
                  <Badge>Installed</Badge>
                </Show>
              </div>
              <ul
                style={{ margin: 0, padding: 0, "list-style": "none", display: "grid", gap: "6px" }}
              >
                <For each={s().voices}>
                  {(voice) => (
                    <li
                      style={{
                        display: "flex",
                        gap: "12px",
                        "align-items": "center",
                        "flex-wrap": "wrap",
                      }}
                    >
                      <span translate="no" style={{ flex: 1, "min-width": "160px" }}>
                        {voice.name}
                        <span
                          style={{
                            display: "block",
                            "font-size": "12px",
                            color: "var(--text-muted)",
                          }}
                        >
                          {voice.license}
                        </span>
                      </span>
                      <Show
                        when={running()?.what === voice.language}
                        fallback={
                          <Show
                            when={voice.installed}
                            fallback={
                              <Button
                                size="sm"
                                variant="secondary"
                                disabled={busy() || Boolean(running()) || !s().programInstalled}
                                onClick={() =>
                                  void start(
                                    () =>
                                      client.POST("/api/v1/speech/voices/{language}", {
                                        params: { path: { language: voice.language } },
                                      }),
                                    `Download the voice ${voice.name} (about 60 MB)?`,
                                  )
                                }
                              >
                                Download
                              </Button>
                            }
                          >
                            <Button
                              size="sm"
                              variant="secondary"
                              disabled={busy()}
                              onClick={() => void remove(voice)}
                            >
                              Remove
                            </Button>
                          </Show>
                        }
                      >
                        <span role="status">{progress()}</span>
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
              <Show when={s().download?.finished && s().download?.error}>
                <p role="alert" style={{ color: "var(--danger)" }}>
                  {s().download?.error}
                </p>
              </Show>
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
