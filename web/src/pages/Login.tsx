import { useNavigate } from "@solidjs/router";
import { createSignal, onMount, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";

export function Login() {
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [isSubmitting, setIsSubmitting] = createSignal(false);
  const { login, user, needsSetup } = useAuth();
  const { showToast } = useToast();
  const navigate = useNavigate();

  onMount(() => {
    if (needsSetup()) {
      navigate("/setup", { replace: true });
    } else if (user()) {
      navigate("/", { replace: true });
    }
  });

  const handleSubmit = async (e: Event) => {
    e.preventDefault();
    if (!username() || !password()) {
      setError("Please enter both username and password");
      return;
    }
    setError(null);
    setIsSubmitting(true);
    const success = await login(username(), password());
    setIsSubmitting(false);

    if (success) {
      showToast({ title: "Welcome back", message: "Successfully logged in", type: "success" });
      navigate("/");
    } else {
      setError("Invalid username or password");
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "justify-content": "center",
        "align-items": "center",
        height: "100vh",
        background: "var(--bg-base)",
      }}
    >
      <div
        style={{
          width: "360px",
          padding: "32px",
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-lg)",
          "box-shadow": "0 20px 25px -5px rgba(0, 0, 0, 0.5)",
        }}
      >
        <div style={{ "text-align": "center", "margin-bottom": "28px" }}>
          <div
            style={{
              width: "48px",
              height: "48px",
              background: "var(--accent)",
              "border-radius": "var(--radius-md)",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-weight": "800",
              color: "#fff",
              "font-size": "24px",
              margin: "0 auto 12px",
            }}
          >
            M
          </div>
          <h2 style={{ margin: "0 0 4px", "font-size": "20px", "font-weight": "700" }}>
            Sign in to MBL
          </h2>
          <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
            Enter your server credentials to continue
          </p>
        </div>

        <Show when={error()}>
          <div
            style={{
              padding: "10px 14px",
              background: "var(--danger-light)",
              border: "1px solid var(--danger)",
              "border-radius": "var(--radius-sm)",
              color: "var(--danger)",
              "font-size": "13px",
              "margin-bottom": "18px",
            }}
          >
            {error()}
          </div>
        </Show>

        <form
          onSubmit={handleSubmit}
          style={{ display: "flex", "flex-direction": "column", gap: "16px" }}
        >
          <div>
            <label
              for="username"
              style={{
                display: "block",
                "margin-bottom": "6px",
                "font-size": "13px",
                "font-weight": "600",
                color: "var(--text-secondary)",
              }}
            >
              Username
            </label>
            <input
              id="username"
              type="text"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              placeholder="Username"
              style={{
                width: "100%",
                padding: "10px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
                outline: "none",
              }}
            />
          </div>

          <div>
            <label
              for="password"
              style={{
                display: "block",
                "margin-bottom": "6px",
                "font-size": "13px",
                "font-weight": "600",
                color: "var(--text-secondary)",
              }}
            >
              Password
            </label>
            <input
              id="password"
              type="password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              placeholder="••••••••"
              style={{
                width: "100%",
                padding: "10px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
                outline: "none",
              }}
            />
          </div>

          <button
            type="submit"
            disabled={isSubmitting()}
            style={{
              "margin-top": "8px",
              padding: "11px",
              background: "var(--accent)",
              color: "#fff",
              border: "none",
              "border-radius": "var(--radius-sm)",
              "font-weight": "600",
              "font-size": "14px",
              cursor: isSubmitting() ? "not-allowed" : "pointer",
              opacity: isSubmitting() ? 0.7 : 1,
            }}
          >
            {isSubmitting() ? "Signing in..." : "Sign In"}
          </button>
        </form>
      </div>
    </div>
  );
}

export default Login;
