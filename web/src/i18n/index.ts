import { createMemo } from "solid-js";
import { type AppLocale, useTheme } from "../context/ThemeContext";

export const translations = {
  en: {
    // Navigation
    home: "Home",
    libraries: "Libraries",
    search: "Search",
    liveTv: "Live TV",
    admin: "Admin",
    users: "Users",
    aiAgents: "AI Agents",
    hardware: "Hardware",
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
    liveTv: "Live-TV",
    admin: "Admin",
    users: "Användare",
    aiAgents: "AI-agenter",
    hardware: "Hårdvara",
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
    liveTv: "Live-TV",
    admin: "Admin",
    users: "Benutzer",
    aiAgents: "KI-Agenten",
    hardware: "Hardware",
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
  fr: {
    // Navigation
    home: "Accueil",
    libraries: "Médiathèques",
    search: "Rechercher",
    liveTv: "TV en direct",
    admin: "Administration",
    users: "Utilisateurs",
    aiAgents: "Agents IA",
    hardware: "Matériel",
    settings: "Paramètres",
    switchProfile: "Changer de profil",
    logout: "Déconnexion",

    // Home
    continueWatching: "Reprendre la lecture",
    upNext: "À suivre",
    recentlyAdded: "Récemment ajoutés",
    playNow: "Lire maintenant",
    details: "Détails & Plus",

    // Detail
    play: "Lire",
    resume: "Reprendre",
    fromBeginning: "Depuis le début",
    directPlay: "Lecture directe",
    directStream: "Flux direct (Remux)",
    transcode: "Transcodage",
    endsAt: "Fin à",
    seasons: "Saisons",
    episodes: "Épisodes",
    audio: "Audio",
    subtitles: "Sous-titres",
    cast: "Distribution & Équipe",

    // Player
    nightMode: "Mode nuit (Amplification vocale)",
    statsForNerds: "Stats pour geeks",
    speed: "Vitesse",
    quality: "Qualité",
    skipIntro: "Passer l'intro",
    skipCredits: "Passer le générique",
    auto: "Auto",

    // Settings
    theme: "Thème",
    accent: "Couleur d'accent",
    language: "Langue",
    reducedMotion: "Réduction des mouvements",
    subtitlesStyle: "Style des sous-titres",
    size: "Taille",
    background: "Arrière-plan",
    color: "Couleur",

    // Themes
    dark: "Sombre",
    oled: "Noir OLED",
    light: "Clair",
    highContrast: "Contraste élevé",

    // Accents
    amber: "Ambre",
    indigo: "Indigo",
    emerald: "Émeraude",
    rose: "Rose",
    cyan: "Cyan",
    violet: "Violet",

    // Actions
    save: "Enregistrer",
    cancel: "Annuler",
    close: "Fermer",
    searchPlaceholder: "Rechercher films, séries, musique (Ctrl+K)...",
  },
  es: {
    // Navigation
    home: "Inicio",
    libraries: "Bibliotecas",
    search: "Buscar",
    liveTv: "TV en vivo",
    admin: "Administración",
    users: "Usuarios",
    aiAgents: "Agentes IA",
    hardware: "Hardware",
    settings: "Configuración",
    switchProfile: "Cambiar perfil",
    logout: "Cerrar sesión",

    // Home
    continueWatching: "Continuar viendo",
    upNext: "A continuación",
    recentlyAdded: "Añadidos recientemente",
    playNow: "Reproducir ahora",
    details: "Detalles y más",

    // Detail
    play: "Reproducir",
    resume: "Reanudar",
    fromBeginning: "Desde el principio",
    directPlay: "Reproducción directa",
    directStream: "Transmisión directa (Remux)",
    transcode: "Transcodificación",
    endsAt: "Termina a las",
    seasons: "Temporadas",
    episodes: "Episodios",
    audio: "Audio",
    subtitles: "Subtítulos",
    cast: "Reparto y equipo",

    // Player
    nightMode: "Modo nocturno (Mejora de diálogo)",
    statsForNerds: "Datos para nerds",
    speed: "Velocidad",
    quality: "Calidad",
    skipIntro: "Saltar introducción",
    skipCredits: "Saltar créditos",
    auto: "Automático",

    // Settings
    theme: "Tema",
    accent: "Color de acento",
    language: "Idioma",
    reducedMotion: "Movimiento reducido",
    subtitlesStyle: "Estilo de subtítulos",
    size: "Tamaño",
    background: "Fondo",
    color: "Color",

    // Themes
    dark: "Oscuro",
    oled: "Negro OLED",
    light: "Claro",
    highContrast: "Alto contraste",

    // Accents
    amber: "Ámbar",
    indigo: "Índigo",
    emerald: "Esmeralda",
    rose: "Rosa",
    cyan: "Cian",
    violet: "Violeta",

    // Actions
    save: "Guardar",
    cancel: "Cancelar",
    close: "Cerrar",
    searchPlaceholder: "Buscar películas, series, música (Ctrl+K)...",
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
