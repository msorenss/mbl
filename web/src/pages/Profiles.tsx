import { useNavigate } from "@solidjs/router";
import { createResource, createSignal, For, Show } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button, Dialog } from "../components/ui";
import { useAuth } from "../context/AuthContext";
import { useI18n } from "../i18n";

type UserProfileDto = components["schemas"]["UserProfileDto"];

export function Profiles() {
  const navigate = useNavigate();
  const { user, logout, switchProfile } = useAuth();
  const [pinFor, setPinFor] = createSignal<UserProfileDto | null>(null);
  const [pin, setPin] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const { t } = useI18n();

  const [profiles] = createResource(async () => {
    try {
      const res = await client.GET("/api/v1/users/profiles");
      if (res.data) return res.data as UserProfileDto[];
    } catch {}
    // Fallback to current user if profiles endpoint is unavailable
    const cur = user();
    if (cur) {
      return [
        {
          id: cur.id,
          username: cur.username,
          displayName: cur.displayName,
          isAdmin: cur.isAdmin,
          hasPin: false,
          isManaged: false,
        },
      ];
    }
    return [];
  });

  const enter = async (p: UserProfileDto, code?: string) => {
    setBusy(true);
    setError("");
    const failure = await switchProfile(p.id, code);
    setBusy(false);
    if (failure) {
      setError(failure);
      return;
    }
    setPinFor(null);
    setPin("");
    navigate("/", { replace: true });
  };

  const handleSelectProfile = async (p: UserProfileDto) => {
    if (p.id === user()?.id) {
      navigate("/");
      return;
    }
    // Managed profiles and non-admin accounts with a PIN switch without the password.
    if (p.isManaged || (p.hasPin && !p.isAdmin)) {
      setError("");
      if (p.hasPin) {
        setPin("");
        setPinFor(p);
      } else {
        await enter(p);
      }
      return;
    }
    // Full accounts sign in with their own password.
    await logout();
    navigate(`/login?username=${encodeURIComponent(p.username)}`, { replace: true });
  };

  return (
    <div
      class="mbl-page"
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        "min-height": "75dvh",
        padding: "var(--space-6)",
      }}
    >
      <h1
        style={{
          "font-size": "clamp(28px, 4vw, 42px)",
          "font-weight": "var(--font-weight-bold)",
          color: "var(--text-primary)",
          "margin-bottom": "var(--space-3)",
        }}
      >
        {t("whosWatching")}
      </h1>

      <p
        style={{
          color: "var(--text-secondary)",
          "text-align": "center",
          "max-width": "480px",
          "margin-bottom": "var(--space-8)",
        }}
      >
        {t("profilesHint")}
      </p>
      {/* Profiles Grid */}
      <div
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          gap: "var(--space-8)",
          "justify-content": "center",
          "margin-bottom": "var(--space-10)",
        }}
      >
        <For each={profiles()}>
          {(p) => {
            const isCurrent = () => user()?.id === p.id;
            return (
              <button
                type="button"
                onClick={() => handleSelectProfile(p)}
                style={{
                  background: "transparent",
                  border: "none",
                  display: "flex",
                  "flex-direction": "column",
                  "align-items": "center",
                  gap: "var(--space-3)",
                  cursor: "pointer",
                  outline: "none",
                  padding: "0",
                }}
              >
                <div
                  style={{
                    width: "120px",
                    height: "120px",
                    "border-radius": "var(--radius-lg)",
                    background: isCurrent()
                      ? "linear-gradient(135deg, var(--accent) 0%, var(--accent-hover) 100%)"
                      : "var(--bg-card)",
                    border: isCurrent() ? "3px solid var(--accent)" : "2px solid var(--border)",
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "center",
                    "font-size": "44px",
                    "box-shadow": "var(--shadow-md)",
                    transition: "all var(--duration-fast) var(--ease-standard)",
                  }}
                  class="profile-card"
                >
                  {p.displayName.charAt(0).toUpperCase()}
                </div>
                <div style={{ display: "flex", "align-items": "center", gap: "var(--space-1)" }}>
                  <span
                    style={{
                      "font-size": "var(--font-size-md)",
                      "font-weight": "var(--font-weight-medium)",
                      color: isCurrent() ? "var(--text-primary)" : "var(--text-secondary)",
                    }}
                  >
                    {p.displayName}
                  </span>
                  {p.hasPin && <span style={{ "font-size": "12px", opacity: 0.6 }}>🔒</span>}
                </div>
                {p.isAdmin && (
                  <span
                    style={{
                      "font-size": "10px",
                      color: "var(--accent)",
                      "text-transform": "uppercase",
                      "letter-spacing": "0.05em",
                      "font-weight": "var(--font-weight-bold)",
                    }}
                  >
                    Admin
                  </span>
                )}
              </button>
            );
          }}
        </For>
      </div>

      <Show when={error() && !pinFor()}>
        <p role="alert" style={{ color: "var(--danger, #c0392b)" }}>
          {error()}
        </p>
      </Show>
      <Dialog
        open={pinFor() !== null}
        onOpenChange={(open) => {
          if (!open) setPinFor(null);
        }}
        title={`Enter PIN for ${pinFor()?.displayName ?? ""}`}
      >
        <form
          onSubmit={(e) => {
            e.preventDefault();
            const target = pinFor();
            if (target) void enter(target, pin());
          }}
          style={{ display: "grid", gap: "12px" }}
        >
          <input
            type="password"
            inputmode="numeric"
            autocomplete="off"
            aria-label="PIN"
            maxLength={8}
            required
            value={pin()}
            onInput={(e) => setPin(e.currentTarget.value.replace(/\D/g, ""))}
            style={{ "font-size": "24px", "letter-spacing": "0.3em" }}
          />
          <Show when={error()}>
            <p role="alert" style={{ margin: 0, color: "var(--danger, #c0392b)" }}>
              {error()}
            </p>
          </Show>
          <div style={{ display: "flex", gap: "10px" }}>
            <Button type="submit" disabled={busy() || pin().length < 4}>
              {busy() ? "Checking…" : "Switch profile"}
            </Button>
            <Button type="button" variant="secondary" onClick={() => setPinFor(null)}>
              Cancel
            </Button>
          </div>
        </form>
      </Dialog>

      <Show when={user()?.isAdmin}>
        <Button variant="outline" size="md" onClick={() => navigate("/admin/users")}>
          {t("manageProfiles")}
        </Button>
      </Show>
    </div>
  );
}

export default Profiles;
