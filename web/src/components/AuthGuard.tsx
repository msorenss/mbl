import { useNavigate } from "@solidjs/router";
import { createEffect, type JSX, Show } from "solid-js";
import { useAuth } from "../context/AuthContext";

export function AuthGuard(props: { children: JSX.Element; adminOnly?: boolean }) {
  const { user, loading, needsSetup } = useAuth();
  const navigate = useNavigate();

  createEffect(() => {
    if (!loading()) {
      if (needsSetup()) {
        navigate("/setup", { replace: true });
      } else if (!user()) {
        navigate("/login", { replace: true });
      } else if (props.adminOnly && !user()?.isAdmin) {
        navigate("/", { replace: true });
      }
    }
  });

  return (
    <Show
      when={!loading() && user() && (!props.adminOnly || user()?.isAdmin)}
      fallback={
        <div
          style={{
            display: "flex",
            "justify-content": "center",
            "align-items": "center",
            height: "100vh",
            color: "var(--text-muted)",
          }}
        >
          Loading...
        </div>
      }
    >
      {props.children}
    </Show>
  );
}
