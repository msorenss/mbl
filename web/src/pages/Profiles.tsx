import { useNavigate } from "@solidjs/router";
import { createResource, createSignal, For } from "solid-js";
import client from "../api/client";
import type { components } from "../api/schema";
import { Button, Dialog, PinPad } from "../components/ui";
import { useAuth } from "../context/AuthContext";

type UserProfileDto = components["schemas"]["UserProfileDto"];

export function Profiles() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [selectedUser, setSelectedUser] = createSignal<UserProfileDto | null>(null);
  const [pinError, setPinError] = createSignal("");

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
        },
      ];
    }
    return [];
  });

  const handleSelectProfile = (p: UserProfileDto) => {
    if (p.hasPin) {
      setSelectedUser(p);
      setPinError("");
    } else {
      // Profile selected
      navigate("/");
    }
  };

  const handlePinComplete = (pin: string) => {
    if (pin === "0000" || !selectedUser()?.hasPin) {
      setSelectedUser(null);
      navigate("/");
    } else {
      setPinError("Incorrect PIN");
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        "align-items": "center",
        "justify-content": "center",
        "min-height": "80vh",
        padding: "var(--space-6)",
      }}
    >
      <h1
        style={{
          "font-size": "clamp(28px, 4vw, 42px)",
          "font-weight": "var(--font-weight-bold)",
          color: "var(--text-primary)",
          "margin-bottom": "var(--space-10)",
        }}
      >
        Who's watching?
      </h1>

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

      <Button variant="outline" size="md" onClick={() => navigate("/admin/users")}>
        Manage Profiles
      </Button>

      {/* PIN Dialog */}
      <Dialog
        open={selectedUser() !== null}
        onOpenChange={(open) => !open && setSelectedUser(null)}
      >
        <PinPad
          title={`Enter PIN for ${selectedUser()?.displayName}`}
          error={pinError()}
          onComplete={handlePinComplete}
        />
      </Dialog>
    </div>
  );
}

export default Profiles;
