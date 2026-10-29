import { createSignal, For, onMount, Show } from "solid-js";
import client from "../../api/client";
import type { components } from "../../api/schema";
import Invites from "../../components/Invites";
import UserPermissions from "../../components/UserPermissions";
import { useAuth } from "../../context/AuthContext";
import { useToast } from "../../context/ToastContext";

type UserDto = components["schemas"]["UserDto"];

export function Users() {
  const { user: currentUser } = useAuth();
  const { showToast } = useToast();

  const [users, setUsers] = createSignal<UserDto[]>([]);
  const [loading, setLoading] = createSignal(true);
  const [showCreateModal, setShowCreateModal] = createSignal(false);

  // Form state
  const [newUsername, setNewUsername] = createSignal("");
  const [newDisplayName, setNewDisplayName] = createSignal("");
  const [newPassword, setNewPassword] = createSignal("");
  const [newIsAdmin, setNewIsAdmin] = createSignal(false);
  const [isSubmitting, setIsSubmitting] = createSignal(false);

  const loadUsers = async () => {
    setLoading(true);
    try {
      const { data, error } = await client.GET("/api/v1/users");
      if (error) throw new Error("Could not load users");
      if (data) setUsers(data);
    } catch {
      showToast({
        title: "Error",
        message: "Could not load users. Reload to retry.",
        type: "error",
      });
    } finally {
      setLoading(false);
    }
  };

  const handleCreateUser = async (e: Event) => {
    e.preventDefault();
    if (!newUsername().trim() || !newPassword()) {
      showToast({
        title: "Validation Error",
        message: "Username and password are required",
        type: "danger",
      });
      return;
    }
    if (newPassword().length < 8) {
      showToast({
        title: "Validation Error",
        message: "Password must be at least 8 characters",
        type: "danger",
      });
      return;
    }

    setIsSubmitting(true);
    try {
      const { data, error } = await client.POST("/api/v1/users", {
        body: {
          username: newUsername().trim(),
          password: newPassword(),
          displayName: newDisplayName().trim() || undefined,
          isAdmin: newIsAdmin(),
        },
      });

      if (data && !error) {
        showToast({
          title: "User Created",
          message: `User ${data.username} created`,
          type: "success",
        });
        setShowCreateModal(false);
        setNewUsername("");
        setNewDisplayName("");
        setNewPassword("");
        setNewIsAdmin(false);
        await loadUsers();
      } else {
        showToast({
          title: "Creation Failed",
          message: error ? String(error) : "Failed to create user",
          type: "danger",
        });
      }
    } catch (e) {
      showToast({
        title: "Error",
        message: e instanceof Error ? e.message : "Error creating user",
        type: "danger",
      });
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteUser = async (userToDelete: UserDto) => {
    if (userToDelete.id === currentUser()?.id) {
      showToast({
        title: "Action Denied",
        message: "You cannot delete your own account",
        type: "warning",
      });
      return;
    }

    if (!confirm(`Are you sure you want to delete user '${userToDelete.username}'?`)) {
      return;
    }

    try {
      const { error } = await client.DELETE("/api/v1/users/{id}", {
        params: { path: { id: userToDelete.id } },
      });

      if (!error) {
        showToast({
          title: "User Deleted",
          message: `User ${userToDelete.username} deleted`,
          type: "info",
        });
        await loadUsers();
      } else {
        showToast({ title: "Delete Failed", message: String(error), type: "danger" });
      }
    } catch (e) {
      showToast({
        title: "Error",
        message: e instanceof Error ? e.message : "Network error",
        type: "danger",
      });
    }
  };

  onMount(loadUsers);

  return (
    <div>
      <div
        style={{
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          "margin-bottom": "24px",
        }}
      >
        <div>
          <h1 style={{ margin: "0 0 6px", "font-size": "24px", "font-weight": "700" }}>Users</h1>
          <p style={{ margin: 0, color: "var(--text-secondary)", "font-size": "14px" }}>
            Manage user accounts, roles, and access credentials
          </p>
        </div>

        <button
          type="button"
          onClick={() => setShowCreateModal(true)}
          style={{
            padding: "9px 16px",
            background: "var(--accent)",
            color: "#fff",
            border: "none",
            "border-radius": "var(--radius-sm)",
            "font-weight": "600",
            "font-size": "13px",
            cursor: "pointer",
          }}
        >
          + Create User
        </button>
      </div>

      <Show
        when={!loading()}
        fallback={<div style={{ color: "var(--text-muted)" }}>Loading users...</div>}
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
              "font-size": "14px",
            }}
          >
            <thead>
              <tr
                style={{ "border-bottom": "1px solid var(--border)", background: "var(--bg-card)" }}
              >
                <th style={{ padding: "12px 16px" }}>Username</th>
                <th style={{ padding: "12px 16px" }}>Display Name</th>
                <th style={{ padding: "12px 16px" }}>Role</th>
                <th style={{ padding: "12px 16px" }}>Created</th>
                <th style={{ padding: "12px 16px", "text-align": "right" }}>Actions</th>
              </tr>
            </thead>
            <tbody>
              <For each={users()}>
                {(u) => (
                  <tr style={{ "border-bottom": "1px solid var(--border)" }}>
                    <td style={{ padding: "12px 16px", "font-weight": "500" }}>{u.username}</td>
                    <td style={{ padding: "12px 16px", color: "var(--text-secondary)" }}>
                      {u.displayName || "-"}
                    </td>
                    <td style={{ padding: "12px 16px" }}>
                      <span
                        style={{
                          display: "inline-block",
                          padding: "2px 8px",
                          "border-radius": "12px",
                          "font-size": "11px",
                          "font-weight": "600",
                          background: u.isAdmin ? "var(--accent-light)" : "var(--bg-overlay)",
                          color: u.isAdmin ? "var(--accent)" : "var(--text-secondary)",
                        }}
                      >
                        {u.isAdmin ? "Admin" : "User"}
                      </span>
                    </td>
                    <td
                      style={{
                        padding: "12px 16px",
                        color: "var(--text-muted)",
                        "font-size": "13px",
                      }}
                    >
                      {new Date(u.createdAt).toLocaleDateString()}
                    </td>
                    <td style={{ padding: "12px 16px", "text-align": "right" }}>
                      <UserPermissions user={u} />
                      <Show when={u.id !== currentUser()?.id}>
                        <button
                          type="button"
                          onClick={() => handleDeleteUser(u)}
                          title="Delete user account"
                          style={{
                            background: "transparent",
                            border: "none",
                            color: "var(--danger)",
                            cursor: "pointer",
                            padding: "4px 8px",
                            "font-size": "12px",
                          }}
                        >
                          Delete
                        </button>
                      </Show>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      </Show>

      {/* Create User Modal */}
      <Show when={showCreateModal()}>
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
              width: "440px",
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
              <h2 style={{ margin: 0, "font-size": "18px", "font-weight": "700" }}>
                Create New User
              </h2>
              <button
                type="button"
                onClick={() => setShowCreateModal(false)}
                style={{
                  background: "transparent",
                  border: "none",
                  color: "var(--text-muted)",
                  "font-size": "18px",
                  cursor: "pointer",
                }}
              >
                ✕
              </button>
            </div>

            <form
              onSubmit={handleCreateUser}
              style={{ display: "flex", "flex-direction": "column", gap: "14px" }}
            >
              <div>
                <label
                  for="create-username"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                  }}
                >
                  Username
                </label>
                <input
                  id="create-username"
                  type="text"
                  value={newUsername()}
                  onInput={(e) => setNewUsername(e.currentTarget.value)}
                  placeholder="e.g. alice"
                  style={{
                    width: "100%",
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                    "font-size": "14px",
                  }}
                />
              </div>

              <div>
                <label
                  for="create-displayname"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                  }}
                >
                  Display Name (optional)
                </label>
                <input
                  id="create-displayname"
                  type="text"
                  value={newDisplayName()}
                  onInput={(e) => setNewDisplayName(e.currentTarget.value)}
                  placeholder="e.g. Alice Smith"
                  style={{
                    width: "100%",
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                    "font-size": "14px",
                  }}
                />
              </div>

              <div>
                <label
                  for="create-password"
                  style={{
                    display: "block",
                    "margin-bottom": "6px",
                    "font-size": "13px",
                    "font-weight": "600",
                  }}
                >
                  Password (min. 8 characters)
                </label>
                <input
                  id="create-password"
                  type="password"
                  value={newPassword()}
                  onInput={(e) => setNewPassword(e.currentTarget.value)}
                  placeholder="••••••••"
                  style={{
                    width: "100%",
                    padding: "9px 12px",
                    background: "var(--bg-base)",
                    border: "1px solid var(--border)",
                    "border-radius": "var(--radius-sm)",
                    color: "var(--text-primary)",
                    "font-size": "14px",
                  }}
                />
              </div>

              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "8px",
                  "font-size": "13px",
                  "margin-top": "4px",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={newIsAdmin()}
                  onChange={(e) => setNewIsAdmin(e.currentTarget.checked)}
                  style={{ "accent-color": "var(--accent)" }}
                />
                <span>Grant Administrator privileges</span>
              </label>

              <div
                style={{
                  display: "flex",
                  "justify-content": "flex-end",
                  gap: "10px",
                  "margin-top": "12px",
                }}
              >
                <button
                  type="button"
                  onClick={() => setShowCreateModal(false)}
                  style={{
                    padding: "8px 14px",
                    background: "var(--bg-card)",
                    border: "1px solid var(--border)",
                    color: "var(--text-primary)",
                    "border-radius": "var(--radius-sm)",
                    cursor: "pointer",
                  }}
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={isSubmitting()}
                  style={{
                    padding: "8px 16px",
                    background: "var(--accent)",
                    color: "#fff",
                    border: "none",
                    "border-radius": "var(--radius-sm)",
                    "font-weight": "600",
                    cursor: isSubmitting() ? "not-allowed" : "pointer",
                  }}
                >
                  {isSubmitting() ? "Creating..." : "Create User"}
                </button>
              </div>
            </form>
          </div>
        </div>
      </Show>
      <Invites />
    </div>
  );
}
export default Users;
