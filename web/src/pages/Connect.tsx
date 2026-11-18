import { useSearchParams } from "@solidjs/router";
import { createSignal, Match, Switch } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button } from "../components/ui";

type Pending = components["schemas"]["QuickConnectPending"];

const CODE = /^\d{6}$/;

function errorMessage(status: number | undefined, fallback: string): string {
  if (status === 404) return "No device is waiting with that code. Codes expire after 10 minutes.";
  if (status === 409) return "That code has already been used.";
  return fallback;
}

/** Approve a TV, app or other device that shows a 6-digit Quick Connect code. */
export default function Connect() {
  const [params] = useSearchParams();
  const initial = typeof params.code === "string" ? params.code.replace(/\D/g, "").slice(0, 6) : "";
  const [code, setCode] = createSignal(initial);
  const [pending, setPending] = createSignal<Pending | null>(null);
  const [done, setDone] = createSignal<Pending | null>(null);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  const lookup = async (event?: Event) => {
    event?.preventDefault();
    setError("");
    if (!CODE.test(code())) {
      setError("Enter the 6-digit code shown on the device.");
      return;
    }
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/auth/quick-connect/lookup", {
        body: { code: code() },
      });
      if (!result.data) {
        setError(errorMessage(result.response?.status, "Could not look up the code."));
        return;
      }
      setPending(result.data);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const approve = async () => {
    const device = pending();
    if (!device) return;
    setError("");
    setBusy(true);
    try {
      const result = await client.POST("/api/v1/auth/quick-connect/authorize", {
        body: { code: code() },
      });
      if (result.response?.status !== 204) {
        setPending(null);
        setError(errorMessage(result.response?.status, "Could not approve the device."));
        return;
      }
      setPending(null);
      setDone(device);
    } catch {
      setError("Could not reach the server.");
    } finally {
      setBusy(false);
    }
  };

  const reset = () => {
    setPending(null);
    setDone(null);
    setError("");
    setCode("");
  };

  return (
    <div class="mbl-page" style={{ "max-width": "520px", display: "grid", gap: "20px" }}>
      <h1 style={{ margin: 0, "font-size": "24px", "font-weight": "700" }}>Sign in a device</h1>
      <section class="mbl-card-section" style={{ display: "grid", gap: "14px" }}>
        <Switch>
          <Match when={done()}>
            {(device) => (
              <>
                <p role="status" style={{ margin: 0 }}>
                  <strong>{device().deviceName}</strong> is signed in to your account. It continues
                  on its own within a few seconds.
                </p>
                <div>
                  <Button variant="secondary" onClick={reset}>
                    Sign in another device
                  </Button>
                </div>
              </>
            )}
          </Match>
          <Match when={pending()}>
            {(device) => (
              <>
                <p style={{ margin: 0 }}>
                  Sign in <strong>{device().deviceName}</strong> ({device().clientName}) with your
                  account?
                </p>
                <p
                  style={{
                    margin: 0,
                    "font-size": "var(--font-size-sm)",
                    color: "var(--text-secondary)",
                  }}
                >
                  Only approve a device you are holding or can see. It gets the same access as you
                  until you sign it out under Account &amp; security.
                </p>
                <div style={{ display: "flex", gap: "10px", "flex-wrap": "wrap" }}>
                  <Button disabled={busy()} onClick={approve}>
                    {busy() ? "Approving…" : "Approve"}
                  </Button>
                  <Button variant="secondary" disabled={busy()} onClick={reset}>
                    Cancel
                  </Button>
                </div>
              </>
            )}
          </Match>
          <Match when={true}>
            <form onSubmit={lookup} style={{ display: "grid", gap: "12px" }}>
              <p
                style={{
                  margin: 0,
                  "font-size": "var(--font-size-sm)",
                  color: "var(--text-secondary)",
                }}
              >
                Choose "Quick Connect" on the TV or app, then enter the 6-digit code it shows.
              </p>
              <label style={{ display: "grid", gap: "6px", "max-width": "220px" }}>
                <span style={{ "font-size": "var(--font-size-xs)", "font-weight": "600" }}>
                  Code
                </span>
                <input
                  inputmode="numeric"
                  autocomplete="one-time-code"
                  maxLength={6}
                  pattern="\d{6}"
                  required
                  value={code()}
                  onInput={(e) => setCode(e.currentTarget.value.replace(/\D/g, "").slice(0, 6))}
                  style={{ "font-size": "24px", "letter-spacing": "0.3em" }}
                />
              </label>
              <div>
                <Button type="submit" disabled={busy()}>
                  {busy() ? "Checking…" : "Continue"}
                </Button>
              </div>
            </form>
          </Match>
        </Switch>
        <p role="alert" style={{ margin: 0, color: "var(--danger, #c0392b)" }}>
          {error()}
        </p>
      </section>
    </div>
  );
}
