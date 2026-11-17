import { useNavigate } from "@solidjs/router";
import { createEffect, createSignal, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";
import { useToast } from "../context/ToastContext";
import { useI18n } from "../i18n";

export function Login() {
  const { t } = useI18n();
  const [username, setUsername] = createSignal(
    new URLSearchParams(window.location.search).get("username") ?? "",
  );
  const [password, setPassword] = createSignal("");
  const [totpCode, setTotpCode] = createSignal("");
  const [error, setError] = createSignal<string | null>(null);
  const [isSubmitting, setIsSubmitting] = createSignal(false);
  const { login, user, needsSetup, loading, loginReason } = useAuth();
  const [twoFactor, setTwoFactor] = createSignal(false);
  let totpInput: HTMLInputElement | undefined;
  const { showToast } = useToast();
  const navigate = useNavigate();

  createEffect(() => {
    if (loading()) return;
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
    const success = await login(username(), password(), totpCode());
    setIsSubmitting(false);

    if (success) {
      showToast({ title: "Welcome back", message: "Successfully logged in", type: "success" });
      navigate("/");
    } else {
      const reason = loginReason?.() ?? null;
      if (reason === "totp_required" || reason === "totp_invalid") {
        setTwoFactor(true);
        setError(
          reason === "totp_required"
            ? "Two-factor authentication is enabled for this account. Enter the 6-digit code from your authenticator app."
            : "The authentication code was not accepted. Enter the current 6-digit code from your authenticator app.",
        );
        totpInput?.focus();
      } else if (reason === "admin_totp_setup_required") {
        setError(
          "Administrator accounts need two-factor authentication to sign in from outside the home network. Sign in from the home network and enable it under Account.",
        );
      } else {
        setError("Sign-in failed. Check your credentials and authentication code.");
      }
    }
  };

  return (
    <div
      style={{
        display: "flex",
        "justify-content": "center",
        "align-items": "center",
        "min-height": "100dvh",
        padding:
          "max(20px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right)) max(20px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left))",
        background: "var(--bg-base)",
      }}
    >
      <div
        class="mbl-page"
        style={{
          width: "min(380px, 100%)",
          padding: "clamp(24px, 5vw, 34px)",
          background: "var(--bg-surface)",
          border: "1px solid var(--border)",
          "border-radius": "var(--radius-xl)",
          "box-shadow": "var(--shadow-xl)",
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
            {t("signInTitle")}
          </h2>
          <p style={{ margin: 0, color: "var(--text-muted)", "font-size": "13px" }}>
            {t("signInSubtitle")}
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
              {t("username")}
            </label>
            <input
              id="username"
              type="text"
              value={username()}
              onInput={(e) => setUsername(e.currentTarget.value)}
              autocomplete="username"
              placeholder={t("username")}
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
              {t("password")}
            </label>
            <input
              id="password"
              type="password"
              value={password()}
              onInput={(e) => setPassword(e.currentTarget.value)}
              autocomplete="current-password"
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

          <div>
            <label
              for="totp-code"
              style={{
                display: "block",
                "margin-bottom": "6px",
                "font-size": "13px",
                "font-weight": "600",
                color: "var(--text-secondary)",
              }}
            >
              {t("authCodeIfEnabled")}
              <Show when={twoFactor()}>
                <span
                  data-testid="two-factor-badge"
                  style={{
                    "margin-left": "8px",
                    padding: "2px 8px",
                    "border-radius": "999px",
                    background: "var(--accent)",
                    color: "#fff",
                    "font-size": "11px",
                    "font-weight": "700",
                  }}
                >
                  2FA enabled
                </span>
              </Show>
            </label>
            <input
              id="totp-code"
              ref={totpInput}
              required={twoFactor()}
              type="text"
              inputmode="numeric"
              autocomplete="one-time-code"
              pattern="[0-9]{6}"
              maxlength={6}
              placeholder="123456"
              value={totpCode()}
              onInput={(e) => setTotpCode(e.currentTarget.value)}
              style={{
                width: "100%",
                padding: "10px 12px",
                background: "var(--bg-base)",
                border: "1px solid var(--border)",
                "border-radius": "var(--radius-sm)",
                color: "var(--text-primary)",
                "font-size": "14px",
                "border-color": twoFactor() ? "var(--accent)" : "var(--border)",
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
            {isSubmitting() ? t("signingIn") : t("signIn")}
          </button>
        </form>
      </div>
    </div>
  );
}

export default Login;
