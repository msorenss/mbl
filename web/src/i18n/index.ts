import { createMemo } from "solid-js";
import { type AppLocale, useTheme } from "../context/ThemeContext";

export const translations = {
  en: {
    // Navigation
    home: "Home",
    libraries: "Libraries",
    search: "Search",
    admin: "Admin",
    users: "Users",
    aiAgents: "AI Agents",
    settings: "Settings",
    switchProfile: "Switch Profile",
    logout: "Log out",

    // Home
    continueWatching: "Continue Watching",
    upNext: "Up Next",
    recentlyAdded: "Recently Added",
    playNow: "Play Now",
    details: "Details & More",

    // Detail
    play: "Play",
    resume: "Resume",
    fromBeginning: "From Beginning",
    directPlay: "Direct Play",
    directStream: "Direct Stream",
    transcode: "Transcode",
    endsAt: "Ends at",
    seasons: "Seasons",
    episodes: "Episodes",
    audio: "Audio",
    subtitles: "Subtitles",
    cast: "Cast & Crew",

    // Player
    nightMode: "Night Mode (Dialogue Boost)",
    statsForNerds: "Stats for Nerds",
    speed: "Speed",
    quality: "Quality",
    skipIntro: "Skip Intro",
    skipCredits: "Skip Credits",
    auto: "Auto",

    // Settings
    theme: "Theme",
    accent: "Accent Color",
    language: "Language",
    reducedMotion: "Reduced Motion",
    subtitlesStyle: "Subtitle Styling",
    size: "Size",
    background: "Background",
    color: "Color",

    // Themes
    dark: "Dark",
    oled: "OLED Black",
    light: "Light",
    highContrast: "High Contrast",

    // Accents
    amber: "Amber",
    indigo: "Indigo",
    emerald: "Emerald",
    rose: "Rose",
    cyan: "Cyan",
    violet: "Violet",

    // Actions
    save: "Save",
    cancel: "Cancel",
    close: "Close",
    searchPlaceholder: "Search movies, shows, music (Ctrl+K)...",
  },
  sv: {
    // Navigation
    home: "Hem",
    libraries: "Bibliotek",
    search: "Sök",
    admin: "Admin",
    users: "Användare",
    aiAgents: "AI-agenter",
    settings: "Inställningar",
    switchProfile: "Byt profil",
    logout: "Logga ut",

    // Home
    continueWatching: "Fortsätt titta",
    upNext: "Nästa avsnitt",
    recentlyAdded: "Nyligen tillagt",
    playNow: "Spela nu",
    details: "Mer info",

    // Detail
    play: "Spela",
    resume: "Återuppta",
    fromBeginning: "Från början",
    directPlay: "Direktuppspelning",
    directStream: "Direktström (Remux)",
    transcode: "Transkodning",
    endsAt: "Slutar kl.",
    seasons: "Säsonger",
    episodes: "Avsnitt",
    audio: "Ljudspår",
    subtitles: "Undertexter",
    cast: "Skådespelare & team",

    // Player
    nightMode: "Nattläge (Förstärk tal)",
    statsForNerds: "Nördfakta (Stats)",
    speed: "Hastighet",
    quality: "Kvalitet",
    skipIntro: "Hoppa över intro",
    skipCredits: "Hoppa över eftertexter",
    auto: "Auto",

    // Settings
    theme: "Tema",
    accent: "Accentfärg",
    language: "Språk",
    reducedMotion: "Minskad rörelse",
    subtitlesStyle: "Undertextstil",
    size: "Storlek",
    background: "Bakgrund",
    color: "Färg",

    // Themes
    dark: "Mörkt",
    oled: "OLED Svart",
    light: "Ljust",
    highContrast: "Hög kontrast",

    // Accents
    amber: "Bärnsten (Amber)",
    indigo: "Indigo",
    emerald: "Smaragd",
    rose: "Ros",
    cyan: "Cyan",
    violet: "Violett",

    // Actions
    save: "Spara",
    cancel: "Avbryt",
    close: "Stäng",
    searchPlaceholder: "Sök filmer, serier, musik (Ctrl+K)...",
  },
  de: {
    // Navigation
    home: "Startseite",
    libraries: "Mediathek",
    search: "Suche",
    admin: "Admin",
    users: "Benutzer",
    aiAgents: "KI-Agenten",
    settings: "Einstellungen",
    switchProfile: "Profil wechseln",
    logout: "Abmelden",

    // Home
    continueWatching: "Weiterschauen",
    upNext: "Als Nächstes",
    recentlyAdded: "Kürzlich hinzugefügt",
    playNow: "Jetzt abspielen",
    details: "Details & Mehr",

    // Detail
    play: "Abspielen",
    resume: "Fortsetzen",
    fromBeginning: "Von Beginn",
    directPlay: "Direkte Wiedergabe",
    directStream: "Direktstream",
    transcode: "Transkodierung",
    endsAt: "Endet um",
    seasons: "Staffeln",
    episodes: "Episoden",
    audio: "Tonspur",
    subtitles: "Untertitel",
    cast: "Besetzung & Crew",

    // Player
    nightMode: "Nachtmodus (Sprachverstärkung)",
    statsForNerds: "Nerddaten (Stats)",
    speed: "Geschwindigkeit",
    quality: "Qualität",
    skipIntro: "Intro überspringen",
    skipCredits: "Abspann überspringen",
    auto: "Auto",

    // Settings
    theme: "Design",
    accent: "Akzentfarbe",
    language: "Sprache",
    reducedMotion: "Reduzierte Bewegung",
    subtitlesStyle: "Untertitel-Stil",
    size: "Größe",
    background: "Hintergrund",
    color: "Farbe",

    // Themes
    dark: "Dunkel",
    oled: "OLED Schwarz",
    light: "Hell",
    highContrast: "Hoher Kontrast",

    // Accents
    amber: "Bernstein",
    indigo: "Indigo",
    emerald: "Smaragd",
    rose: "Rose",
    cyan: "Cyan",
    violet: "Violett",

    // Actions
    save: "Speichern",
    cancel: "Abbrechen",
    close: "Schließen",
    searchPlaceholder: "Filme, Serien, Musik suchen (Strg+K)...",
  },
} as const;

export type TranslationKey = keyof typeof translations.en;

export function useI18n() {
  const { locale } = useTheme();
  const dict = createMemo(() => translations[locale() as AppLocale] || translations.en);

  const t = (key: TranslationKey): string => {
    return (
      (dict() as Record<string, string>)[key] ||
      (translations.en as Record<string, string>)[key] ||
      key
    );
  };

  return { t, locale };
}
