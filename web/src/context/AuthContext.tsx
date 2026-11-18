import { createContext, createSignal, type JSX, onMount, useContext } from "solid-js";
import client, { setStoredToken } from "../api/client";
import type { components } from "../api/schema";

export type UserDto = components["schemas"]["UserDto"];

interface AuthContextValue {
  user: () => UserDto | null;
  loading: () => boolean;
  needsSetup: () => boolean;
  login: (username: string, password: string, totpCode?: string) => Promise<boolean>;
  loginReason?: () => string | null;
  logout: () => Promise<void>;
  refreshUser: () => Promise<void>;
  /** Switch to a managed profile; returns an error message, or null on success. */
  switchProfile: (userId: number, pin?: string) => Promise<string | null>;
  checkSetup: () => Promise<boolean>;
}

const AuthContext = createContext<AuthContextValue>();

export function AuthProvider(props: { children: JSX.Element }) {
  const [user, setUser] = createSignal<UserDto | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [needsSetup, setNeedsSetup] = createSignal(false);
  // Why the last sign-in failed after the password was accepted (two-factor states).
  const [loginReason, setLoginReason] = createSignal<string | null>(null);

  const checkSetup = async () => {
    try {
      const { data } = await client.GET("/api/v1/system/setup");
      if (data && typeof data.needsSetup === "boolean") {
        setNeedsSetup(data.needsSetup);
        return data.needsSetup;
      }
    } catch {
      // Ignore network errors
    }
    return false;
  };

  const refreshUser = async () => {
    try {
      const { data, error } = await client.GET("/api/v1/auth/me");
      if (data && !error) {
        setUser(data);
      } else {
        setUser(null);
      }
    } catch {
      setUser(null);
    }
  };

  const login = async (username: string, password: string, totpCode?: string): Promise<boolean> => {
    setLoading(true);
    try {
      setLoginReason(null);
      const { data, error, response } = await client.POST("/api/v1/auth/login", {
        body: { username, password, totpCode: totpCode?.trim() || undefined },
      });
      if (!data) setLoginReason(response?.headers?.get("x-mbl-login-reason") ?? null);
      if (data && !error) {
        setStoredToken(data.token);
        setUser(data.user);
        return true;
      }
    } catch {
      // Failed login
    } finally {
      setLoading(false);
    }
    return false;
  };

  const switchProfile = async (userId: number, pin?: string): Promise<string | null> => {
    try {
      const { data, error, response } = await client.POST("/api/v1/auth/switch-profile", {
        body: { userId, pin: pin || undefined },
      });
      if (data && !error) {
        setStoredToken(data.token);
        setUser(data.user);
        return null;
      }
      if (response?.status === 429) return "Too many attempts. Wait a while and try again.";
      const message = (error as { error?: { message?: string } } | undefined)?.error?.message;
      return message || "Could not switch profile.";
    } catch {
      return "Could not reach the server.";
    }
  };

  const logout = async () => {
    setLoading(true);
    try {
      await client.POST("/api/v1/auth/logout");
    } catch {
      // Ignore logout errors
    } finally {
      setStoredToken(null);
      setUser(null);
      setLoading(false);
    }
  };

  onMount(async () => {
    const isSetupNeeded = await checkSetup();
    if (!isSetupNeeded) {
      await refreshUser();
    }
    setLoading(false);
  });

  return (
    <AuthContext.Provider
      value={{
        user,
        loading,
        needsSetup,
        login,
        loginReason,
        logout,
        refreshUser,
        switchProfile,
        checkSetup,
      }}
    >
      {props.children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return ctx;
}
