import {
  createContext,
  createEffect,
  createSignal,
  onMount,
  type ParentProps,
  useContext,
} from "solid-js";
import { client } from "../api/client";

export type ThemeMode = "dark" | "oled" | "light" | "high-contrast";
export type AccentColor = "amber" | "indigo" | "emerald" | "rose" | "cyan" | "violet";
export type AppLocale =
  | "en"
  | "sv"
  | "de"
  | "fr"
  | "es"
  | "pt"
  | "it"
  | "nl"
  | "pl"
  | "tr"
  | "ru"
  | "uk"
  | "ar"
  | "hi"
  | "zh"
  | "ja"
  | "ko"
  | "id"
  | "vi"
  | "no"
  | "tl"
  | "ceb"
  | "war";

export interface LocaleOption {
  code: AppLocale;
  name: string;
  nativeName: string;
}

export const SUPPORTED_LOCALES: readonly LocaleOption[] = [
  { code: "en", name: "English", nativeName: "English" },
  { code: "sv", name: "Swedish", nativeName: "Svenska" },
  { code: "de", name: "German", nativeName: "Deutsch" },
  { code: "fr", name: "French", nativeName: "Français" },
  { code: "es", name: "Spanish", nativeName: "Español" },
  { code: "pt", name: "Portuguese", nativeName: "Português" },
  { code: "it", name: "Italian", nativeName: "Italiano" },
  { code: "nl", name: "Dutch", nativeName: "Nederlands" },
  { code: "pl", name: "Polish", nativeName: "Polski" },
  { code: "tr", name: "Turkish", nativeName: "Türkçe" },
  { code: "ru", name: "Russian", nativeName: "Русский" },
  { code: "uk", name: "Ukrainian", nativeName: "Українська" },
  { code: "ar", name: "Arabic", nativeName: "العربية" },
  { code: "hi", name: "Hindi", nativeName: "हिन्दी" },
  { code: "zh", name: "Chinese", nativeName: "中文" },
  { code: "ja", name: "Japanese", nativeName: "日本語" },
  { code: "ko", name: "Korean", nativeName: "한국어" },
  { code: "id", name: "Indonesian", nativeName: "Bahasa Indonesia" },
  { code: "vi", name: "Vietnamese", nativeName: "Tiếng Việt" },
  { code: "no", name: "Norwegian", nativeName: "Norsk" },
  { code: "tl", name: "Tagalog", nativeName: "Tagalog" },
  { code: "ceb", name: "Bisaya (Cebuano)", nativeName: "Bisaya" },
  { code: "war", name: "Waray-Waray", nativeName: "Waray-Waray" },
];

export interface SubtitleStyle {
  size: "sm" | "md" | "lg" | "xl";
  background: "none" | "dim" | "solid";
  color: "white" | "yellow" | "cyan";
}

interface ThemeContextValue {
  theme: () => ThemeMode;
  setTheme: (t: ThemeMode) => void;
  accent: () => AccentColor;
  setAccent: (a: AccentColor) => void;
  reducedMotion: () => boolean;
  setReducedMotion: (rm: boolean) => void;
  locale: () => AppLocale;
  setLocale: (l: AppLocale) => void;
  subtitleStyle: () => SubtitleStyle;
  setSubtitleStyle: (s: SubtitleStyle) => void;
  nightMode: () => boolean;
  setNightMode: (nm: boolean) => void;
  audioOffsetMs: () => number;
  setAudioOffsetMs: (ms: number) => void;
  syncWithBackend: () => Promise<void>;
}

const ThemeContext = createContext<ThemeContextValue>();

const STORAGE_KEYS = {
  theme: "mbl.theme",
  accent: "mbl.accent",
  reducedMotion: "mbl.reducedMotion",
  locale: "mbl.locale",
  subtitleStyle: "mbl.subtitleStyle",
  nightMode: "mbl.nightMode",
  audioOffsetMs: "mbl.audioOffsetMs",
};

export function ThemeProvider(props: ParentProps) {
  const [theme, setThemeSignal] = createSignal<ThemeMode>(
    (typeof localStorage !== "undefined" &&
      (localStorage.getItem(STORAGE_KEYS.theme) as ThemeMode)) ||
      "dark",
  );
  const [accent, setAccentSignal] = createSignal<AccentColor>(
    (typeof localStorage !== "undefined" &&
      (localStorage.getItem(STORAGE_KEYS.accent) as AccentColor)) ||
      "amber",
  );
  const [reducedMotion, setReducedMotionSignal] = createSignal<boolean>(
    typeof localStorage !== "undefined" &&
      localStorage.getItem(STORAGE_KEYS.reducedMotion) === "true",
  );
  const [locale, setLocaleSignal] = createSignal<AppLocale>(
    (typeof localStorage !== "undefined" &&
      (localStorage.getItem(STORAGE_KEYS.locale) as AppLocale)) ||
      "en",
  );
  const [subtitleStyle, setSubtitleStyleSignal] = createSignal<SubtitleStyle>(
    (() => {
      try {
        const stored =
          typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEYS.subtitleStyle);
        if (stored) return JSON.parse(stored);
      } catch {}
      return { size: "md", background: "dim", color: "white" };
    })(),
  );
  const [nightMode, setNightModeSignal] = createSignal<boolean>(
    typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEYS.nightMode) === "true",
  );
  const [audioOffsetMs, setAudioOffsetMsSignal] = createSignal<number>(
    typeof localStorage !== "undefined"
      ? Number.parseInt(localStorage.getItem(STORAGE_KEYS.audioOffsetMs) || "0", 10) || 0
      : 0,
  );

  const applyAttributes = () => {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    const currentTheme = theme();
    const currentLocale = locale();
    root.setAttribute("data-theme", currentTheme);
    root.setAttribute("data-accent", accent());
    root.setAttribute("lang", currentLocale);
    root.setAttribute("dir", currentLocale === "ar" ? "rtl" : "ltr");
    if (reducedMotion()) {
      root.setAttribute("data-reduced-motion", "true");
    } else {
      root.removeAttribute("data-reduced-motion");
    }
    const themeColorMeta = document.querySelector<HTMLMetaElement>('meta[name="theme-color"]');
    if (themeColorMeta) {
      const colorMap: Record<ThemeMode, string> = {
        dark: "#141721",
        oled: "#000000",
        light: "#ffffff",
        "high-contrast": "#000000",
      };
      themeColorMeta.setAttribute("content", colorMap[currentTheme] || "#0c0e14");
    }
  };

  createEffect(() => {
    applyAttributes();
  });

  const saveToBackend = async (patch: {
    theme?: string;
    accent?: string;
    locale?: string;
    reducedMotion?: boolean;
    nightMode?: boolean;
    audioOffsetMs?: number;
    subtitleStyle?: SubtitleStyle;
  }) => {
    try {
      await client.PATCH("/api/v1/users/me/preferences", {
        body: patch as any,
      });
    } catch {
      // offline / not logged in is fine
    }
  };

  const setTheme = (t: ThemeMode) => {
    setThemeSignal(t);
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEYS.theme, t);
    saveToBackend({ theme: t });
  };

  const setAccent = (a: AccentColor) => {
    setAccentSignal(a);
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEYS.accent, a);
    saveToBackend({ accent: a });
  };

  const setReducedMotion = (rm: boolean) => {
    setReducedMotionSignal(rm);
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEYS.reducedMotion, String(rm));
    saveToBackend({ reducedMotion: rm });
  };

  const setLocale = (l: AppLocale) => {
    setLocaleSignal(l);
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEYS.locale, l);
    saveToBackend({ locale: l });
  };

  const setSubtitleStyle = (s: SubtitleStyle) => {
    setSubtitleStyleSignal(s);
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEYS.subtitleStyle, JSON.stringify(s));
    saveToBackend({ subtitleStyle: s });
  };

  const setNightMode = (nm: boolean) => {
    setNightModeSignal(nm);
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEYS.nightMode, String(nm));
    saveToBackend({ nightMode: nm });
  };

  const setAudioOffsetMs = (ms: number) => {
    setAudioOffsetMsSignal(ms);
    if (typeof localStorage !== "undefined")
      localStorage.setItem(STORAGE_KEYS.audioOffsetMs, String(ms));
    saveToBackend({ audioOffsetMs: ms });
  };

  const syncWithBackend = async () => {
    try {
      const res = await client.GET("/api/v1/users/me/preferences");
      if (res.data) {
        const d = res.data as any;
        if (d.theme) setThemeSignal(d.theme);
        if (d.accent) setAccentSignal(d.accent);
        if (d.locale) setLocaleSignal(d.locale);
        if (typeof d.reducedMotion === "boolean") setReducedMotionSignal(d.reducedMotion);
        if (typeof d.nightMode === "boolean") setNightModeSignal(d.nightMode);
        if (typeof d.audioOffsetMs === "number") setAudioOffsetMsSignal(d.audioOffsetMs);
        if (d.subtitleStyle) {
          setSubtitleStyleSignal({
            size: d.subtitleStyle.size || "md",
            background: d.subtitleStyle.background || "dim",
            color: d.subtitleStyle.color || "white",
          });
        }
        applyAttributes();
      }
    } catch {
      // not logged in yet or offline
    }
  };

  onMount(() => {
    applyAttributes();
    syncWithBackend();
  });

  return (
    <ThemeContext.Provider
      value={{
        theme,
        setTheme,
        accent,
        setAccent,
        reducedMotion,
        setReducedMotion,
        locale,
        setLocale,
        subtitleStyle,
        setSubtitleStyle,
        nightMode,
        setNightMode,
        audioOffsetMs,
        setAudioOffsetMs,
        syncWithBackend,
      }}
    >
      {props.children}
    </ThemeContext.Provider>
  );
}

export function useOptionalTheme() {
  return useContext(ThemeContext);
}

export function useTheme() {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return ctx;
}
