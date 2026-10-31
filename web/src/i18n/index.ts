import { createMemo } from "solid-js";
import { type AppLocale, useTheme } from "../context/ThemeContext";

export const translations = {
  en: {
    castDiagnosticsTitle: "Cast diagnostics",
    castDiagnosticsHelp:
      "Local log of the latest Cast attempt. URL queries and credentials are masked. Nothing is uploaded.",
    castDiagnosticsCopy: "Copy Cast log",
    castDiagnosticsCopied: "Copied",
    castDiagnosticsFallback: "Select and copy the log below.",
    castDiagnosticsLog: "Cast diagnostic log",
    onlineRecent: "Recently played online",
    onlinePlay: "Play with resume",
    onlinePlayerTitle: "Online player",
    onlinePlayerLoading: "Preparing and verifying playback…",
    onlinePlayerError: "Playback failed or access was revoked. Check the source again.",
    onlinePlayerHelp:
      "Position is saved for this account. Supported unencrypted VOD HLS is relayed by MBL.",

    onlineProviderSettings: "Search settings",
    onlineProviderHelp:
      "Use Mwmbl or your public HTTPS SearXNG instance with JSON search enabled. Results are ranked by title and viewing signals; they still need a playback check.",
    onlineProviderSave: "Save search settings",
    onlineProviderSaved: "Search settings saved. Submit the title again.",

    onlineTitle: "WebTV — Online Media",
    onlineDescription:
      "Search and stream video channels, series, and online media directly inside Media Lounge Box.",
    onlineSearchLabel: "Title or series",
    onlineSearchAction: "Search the web",
    onlineSearching: "Searching the web…",
    onlineSearchError: "Internet search failed or timed out. Try again.",
    onlineSourceUrlLabel: "Check a public source URL",
    onlineCheck: "Check source",
    onlineChecking: "Checking source…",
    onlineSourceError:
      "The source could not be checked. Use a public HTTP(S) address and try again.",
    onlineOpen: "Open website",
    onlinePreview: "Preview",
    onlinePreviewHelp: "This is a direct-media preview. It does not update library watch history.",
    onlinePreviewError:
      "This browser could not play the source. Open the website or check it again.",
    onlinePreviewLoading: "Loading preview…",
    onlinePreviewReady: "Use the player controls to start playback.",
    onlinePlaying: "Playing",
    onlineUnchecked: "Not checked",
    onlineExternalPage: "Website found",
    onlineExternalPlayer: "HLS source — check in player",
    onlineMediaDetected: "Media sample verified",
    onlineRestricted: "Access restricted or sign-in required",
    onlineUnavailable: "Unavailable",
    onlineUnknown: "Playback not confirmed",
    onlineSeason: "Season",
    onlineEpisode: "Episode",
    onlineCheckedSource: "Checked source",
    onlineProvider: "Search provider",
    onlineResults: "Web results",
    onlineNoResults: "No results found. Try different wording or check a known source URL.",
    onlineIndexed: "My inspected sources",
    onlineVerifying: "Verifying sources…",
    onlineNoVerified:
      "No verified playable source found. Try different wording or a known source URL.",
    onlineNotVerified: "Source could not be verified as playable in MBL, so it is not listed.",
    onlineVerifiedResults: "Verified sources",
    catalogueLoading: "Loading catalogue…",
    catalogueFailed: "The catalogue request failed or timed out. Try again.",
    catalogueEmpty: "No catalogue entries found.",
    retry: "Retry",
    folderRoots: "Folder roots",
    folderPath: "Folder path on server",
    folderOpen: "Open",
    folderCurrent: "Current",
    folderServerRoots: "Server roots",
    folderAddCurrent: "Add current",
    folderLoading: "Loading folders…",
    folderUp: "Parent folder",
    folderAdd: "Add",
    folderEmpty: "No subfolders. You can add the current folder.",
    folderRemove: "Remove folder",
    folderRequestFailed: "Cannot open folder",
    folderAccessHelp:
      "The server service account needs permission to read this folder and traverse its parent folders. Linux packages use the mbl account. Entering a path does not bypass these permissions.",
    folderTimeout: "The folder request timed out. Try again.",
    folderRootWarning:
      "A whole filesystem or drive is selected. It includes system files and prevents overlapping libraries. Choose a dedicated media folder.",
    apiReference: "API reference",
    distributedNodes: "Distributed nodes",
    mediaRequests: "Media requests",
    libraryScanningTitle: "Library Scanning & Automation",
    libraryScanningDesc:
      "Real-time folder watching, missing metadata lookup, and scheduled periodic scans",
    autoScanOnChange: "Scan when changes are detected",
    autoScanOnChangeHelp:
      "Monitors library folders with inotify/file events and scans new or modified files automatically after settle debounce.",
    periodicScanInterval: "Periodic auto-scan interval",
    periodicScanDisabled: "Disabled (Manual only)",
    periodicScan15m: "Every 15 minutes",
    periodicScan30m: "Every 30 minutes",
    periodicScan1h: "Every hour (Recommended)",
    periodicScan6h: "Every 6 hours",
    periodicScan24h: "Every 24 hours",
    autoFetchMissingMetadata: "Automatically query metadata for items without metadata",
    autoFetchMissingMetadataHelp:
      "When scanning, query TMDB and MusicBrainz for existing media that currently lack plot, posters, or metadata.",
    lastScanned: "Last scanned",
    neverScanned: "Never scanned",
    pluginDemoUnavailable: "Demo — playback unavailable",
    pluginDemoHelp:
      "This provider contains sample catalogue entries. Streaming is not connected, so these entries cannot be played.",
    // Navigation
    home: "Home",
    libraries: "Libraries",
    search: "Search",
    liveTv: "Live TV",
    webtv: "WebTV",
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
    castDiagnosticsTitle: "Cast-diagnostik",
    castDiagnosticsHelp:
      "Lokal logg för senaste Cast-försöket. URL-parametrar och inloggningsuppgifter maskeras. Inget laddas upp.",
    castDiagnosticsCopy: "Kopiera Cast-logg",
    castDiagnosticsCopied: "Kopierad",
    castDiagnosticsFallback: "Markera och kopiera loggen nedan.",
    castDiagnosticsLog: "Diagnostiklogg för Cast",
    onlineRecent: "Senast spelat online",
    onlinePlay: "Spela med återupptagning",
    onlinePlayerTitle: "Onlinespelare",
    onlinePlayerLoading: "Förbereder och verifierar uppspelning…",
    onlinePlayerError:
      "Uppspelningen misslyckades eller åtkomsten drogs tillbaka. Kontrollera källan igen.",
    onlinePlayerHelp:
      "Positionen sparas för detta konto. HLS stöds för okrypterad VOD genom MBL:s relä.",

    onlineProviderSettings: "Sökinställningar",
    onlineProviderHelp:
      "Använd Mwmbl eller en egen offentlig HTTPS-instans av SearXNG med JSON-sökning. Träffar sorteras efter titel och visningssignaler; uppspelningen måste fortfarande kontrolleras.",
    onlineProviderSave: "Spara sökinställningar",
    onlineProviderSaved: "Sökinställningar sparade. Sök på titeln igen.",

    onlineTitle: "WebTV — Strömmande webbmedia",
    onlineDescription:
      "Sök och spela upp strömmande video, serier och webb-tv direkt i Media Lounge Box.",
    onlineSearchLabel: "Titel eller serie",
    onlineSearchAction: "Sök på webben",
    onlineSearching: "Söker på webben…",
    onlineSearchError: "Internetsökningen misslyckades eller tog för lång tid. Försök igen.",
    onlineSourceUrlLabel: "Kontrollera en offentlig källadress",
    onlineCheck: "Kontrollera källa",
    onlineChecking: "Kontrollerar källan…",
    onlineSourceError:
      "Källan kunde inte kontrolleras. Ange en offentlig HTTP(S)-adress och försök igen.",
    onlineOpen: "Öppna webbplats",
    onlinePreview: "Förhandsvisa",
    onlinePreviewHelp:
      "Detta är en förhandsvisning av direktmedia. Bibliotekets tittarhistorik uppdateras inte.",
    onlinePreviewError:
      "Webbläsaren kunde inte spela källan. Öppna webbplatsen eller kontrollera källan igen.",
    onlinePreviewLoading: "Laddar förhandsvisningen…",
    onlinePreviewReady: "Starta uppspelningen med spelarens kontroller.",
    onlinePlaying: "Spelar",
    onlineUnchecked: "Inte kontrollerad",
    onlineExternalPage: "Webbsida hittad",
    onlineExternalPlayer: "HLS-källa — kontrollera i spelaren",
    onlineMediaDetected: "Medieprov verifierat",
    onlineRestricted: "Åtkomst spärrad eller inloggning krävs",
    onlineUnavailable: "Otillgänglig",
    onlineUnknown: "Uppspelning inte bekräftad",
    onlineSeason: "Säsong",
    onlineEpisode: "Avsnitt",
    onlineCheckedSource: "Kontrollerad källa",
    onlineProvider: "Söktjänst",
    onlineResults: "Webbträffar",
    onlineNoResults: "Inga träffar. Prova andra sökord eller kontrollera en känd källadress.",
    onlineIndexed: "Mina kontrollerade källor",
    onlineVerifying: "Verifierar källor…",
    onlineNoVerified:
      "Ingen verifierad spelbar källa hittades. Prova andra sökord eller en känd källadress.",
    onlineNotVerified: "Källan kunde inte verifieras som spelbar i MBL och listas därför inte.",
    onlineVerifiedResults: "Verifierade källor",
    catalogueLoading: "Läser katalogen…",
    catalogueFailed: "Katalogen kunde inte hämtas eller tog för lång tid. Försök igen.",
    catalogueEmpty: "Inga katalogposter hittades.",
    retry: "Försök igen",
    folderRoots: "Biblioteksmappar",
    folderPath: "Mappsökväg på servern",
    folderOpen: "Öppna",
    folderCurrent: "Aktuell",
    folderServerRoots: "Serverns rotmappar",
    folderAddCurrent: "Lägg till aktuell",
    folderLoading: "Läser mappar…",
    folderUp: "Överordnad mapp",
    folderAdd: "Lägg till",
    folderEmpty: "Inga undermappar. Du kan lägga till den aktuella mappen.",
    folderRemove: "Ta bort mapp",
    folderRequestFailed: "Kan inte öppna mappen",
    folderAccessHelp:
      "Serverns tjänstekonto behöver kunna läsa mappen och passera dess överordnade mappar. Linux-paket använder kontot mbl. En manuellt angiven sökväg ändrar inte rättigheterna.",
    folderTimeout: "Begäran tog för lång tid. Försök igen.",
    folderRootWarning:
      "Hela filsystemet eller enheten är vald. Det omfattar systemfiler och hindrar överlappande bibliotek. Välj en avgränsad mediamapp.",
    apiReference: "API-referens",
    distributedNodes: "Distribuerade noder",
    mediaRequests: "Medieförfrågningar",
    libraryScanningTitle: "Biblioteksskanning & Automatisering",
    libraryScanningDesc:
      "Realtidsbevakning av mappar, automatisk hämtning av saknad metadata och schemalagda skanningar",
    autoScanOnChange: "Skanna vid filändringar",
    autoScanOnChangeHelp:
      "Bevakar biblioteksmappar i realtid med filsystemhändelser och skannar automatiskt nya och ändrade filer när de kopierats klart.",
    periodicScanInterval: "Periodisk automatisk skanning",
    periodicScanDisabled: "Inaktiverad (Endast manuell)",
    periodicScan15m: "Var 15:e minut",
    periodicScan30m: "Var 30:e minut",
    periodicScan1h: "Varje timme (Rekommenderas)",
    periodicScan6h: "Var 6:e timme",
    periodicScan24h: "Var 24:e timme (Dagligen)",
    autoFetchMissingMetadata: "Sök automatiskt efter saknad metadata vid skanning",
    autoFetchMissingMetadataHelp:
      "Vid skanning görs automatiska sökningar mot TMDB och MusicBrainz för befintliga filmer, serier och musik som saknar beskrivning eller omslag.",
    lastScanned: "Senast skannad",
    neverScanned: "Aldrig skannad",
    pluginDemoUnavailable: "Demo — kan inte spelas",
    pluginDemoHelp:
      "Leverantören innehåller exempelposter. Streaming är inte ansluten och posterna går därför inte att spela.",
    // Navigation
    home: "Hem",
    libraries: "Bibliotek",
    search: "Sök",
    liveTv: "Live-TV",
    webtv: "WebTV",
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
    castDiagnosticsTitle: "Cast-Diagnose",
    castDiagnosticsHelp:
      "Lokales Protokoll des letzten Cast-Versuchs. URL-Parameter und Zugangsdaten sind maskiert. Es wird nichts hochgeladen.",
    castDiagnosticsCopy: "Cast-Protokoll kopieren",
    castDiagnosticsCopied: "Kopiert",
    castDiagnosticsFallback: "Protokoll unten markieren und kopieren.",
    castDiagnosticsLog: "Cast-Diagnoseprotokoll",
    onlineRecent: "Zuletzt online abgespielt",
    onlinePlay: "Mit Fortsetzen abspielen",
    onlinePlayerTitle: "Online-Player",
    onlinePlayerLoading: "Wiedergabe wird geprüft…",
    onlinePlayerError: "Wiedergabe fehlgeschlagen oder Zugriff entzogen. Quelle erneut prüfen.",
    onlinePlayerHelp:
      "Die Position wird für dieses Konto gespeichert. Unverschlüsseltes VOD-HLS wird über MBL übertragen.",

    onlineProviderSettings: "Sucheinstellungen",
    onlineProviderHelp:
      "Mwmbl oder eine öffentliche HTTPS-SearXNG-Instanz mit JSON-Suche verwenden. Titel und Wiedergabehinweise bestimmen die Reihenfolge; Wiedergabe muss geprüft werden.",
    onlineProviderSave: "Sucheinstellungen speichern",
    onlineProviderSaved: "Gespeichert. Titel erneut suchen.",

    onlineTitle: "WebTV — Online-Medien",
    onlineDescription:
      "Durchsuchen und streamen Sie Online-Kanäle, Serien und Web-TV direkt in der Media Lounge Box.",
    onlineSearchLabel: "Titel oder Serie",
    onlineSearchAction: "Im Web suchen",
    onlineSearching: "Websuche läuft…",
    onlineSearchError:
      "Die Internetsuche ist fehlgeschlagen oder hat zu lange gedauert. Erneut versuchen.",
    onlineSourceUrlLabel: "Öffentliche Quelladresse prüfen",
    onlineCheck: "Quelle prüfen",
    onlineChecking: "Quelle wird geprüft…",
    onlineSourceError:
      "Die Quelle konnte nicht geprüft werden. Eine öffentliche HTTP(S)-Adresse verwenden und erneut versuchen.",
    onlineOpen: "Website öffnen",
    onlinePreview: "Vorschau",
    onlinePreviewHelp:
      "Dies ist eine Vorschau direkter Medien. Der Bibliotheksverlauf wird nicht aktualisiert.",
    onlinePreviewError:
      "Der Browser konnte diese Quelle nicht wiedergeben. Website öffnen oder erneut prüfen.",
    onlinePreviewLoading: "Vorschau wird geladen…",
    onlinePreviewReady: "Wiedergabe mit den Player-Steuerelementen starten.",
    onlinePlaying: "Wiedergabe läuft",
    onlineUnchecked: "Nicht geprüft",
    onlineExternalPage: "Website gefunden",
    onlineExternalPlayer: "HLS-Quelle — im Player prüfen",
    onlineMediaDetected: "Medienprobe geprüft",
    onlineRestricted: "Zugriff gesperrt oder Anmeldung erforderlich",
    onlineUnavailable: "Nicht verfügbar",
    onlineUnknown: "Wiedergabe nicht bestätigt",
    onlineSeason: "Staffel",
    onlineEpisode: "Folge",
    onlineCheckedSource: "Geprüfte Quelle",
    onlineProvider: "Suchdienst",
    onlineResults: "Webtreffer",
    onlineNoResults: "Keine Treffer. Andere Suchwörter oder eine bekannte Quelladresse versuchen.",
    onlineIndexed: "Meine geprüften Quellen",
    onlineVerifying: "Quellen werden geprüft…",
    onlineNoVerified:
      "Keine geprüfte abspielbare Quelle gefunden. Andere Suchwörter oder eine bekannte Quelladresse versuchen.",
    onlineNotVerified:
      "Die Quelle konnte nicht als in MBL abspielbar verifiziert werden und wird nicht aufgelistet.",
    onlineVerifiedResults: "Geprüfte Quellen",
    catalogueLoading: "Katalog wird geladen…",
    catalogueFailed:
      "Der Katalog konnte nicht geladen werden oder die Zeit wurde überschritten. Erneut versuchen.",
    catalogueEmpty: "Keine Katalogeinträge gefunden.",
    retry: "Erneut versuchen",
    folderRoots: "Bibliotheksordner",
    folderPath: "Ordnerpfad auf dem Server",
    folderOpen: "Öffnen",
    folderCurrent: "Aktuell",
    folderServerRoots: "Server-Stammverzeichnisse",
    folderAddCurrent: "Aktuellen hinzufügen",
    folderLoading: "Ordner werden geladen…",
    folderUp: "Übergeordneter Ordner",
    folderAdd: "Hinzufügen",
    folderEmpty: "Keine Unterordner. Der aktuelle Ordner kann hinzugefügt werden.",
    folderRemove: "Ordner entfernen",
    folderRequestFailed: "Ordner kann nicht geöffnet werden",
    folderAccessHelp:
      "Das Dienstkonto benötigt Leserechte für diesen Ordner und Zugriffsrechte für übergeordnete Ordner. Linux-Pakete verwenden das Konto mbl. Eine manuelle Pfadangabe ändert keine Berechtigungen.",
    folderTimeout: "Zeitüberschreitung beim Laden der Ordner. Erneut versuchen.",
    folderRootWarning:
      "Ein ganzes Dateisystem oder Laufwerk ist ausgewählt. Es enthält Systemdateien und verhindert überlappende Bibliotheken. Einen eigenen Medienordner wählen.",
    apiReference: "API-Referenz",
    distributedNodes: "Verteilte Knoten",
    mediaRequests: "Medienanfragen",
    libraryScanningTitle: "Mediathek-Scan & Automatisierung",
    libraryScanningDesc:
      "Echtzeit-Ordnerüberwachung, Metadaten-Nachrecherche und regelmäßige Scans",
    autoScanOnChange: "Bei Dateiänderungen automatisch scannen",
    autoScanOnChangeHelp:
      "Überwacht Mediathek-Ordner in Echtzeit und importiert neue oder geänderte Dateien automatisch.",
    periodicScanInterval: "Intervall für regelmäßigen Scan",
    periodicScanDisabled: "Deaktiviert (Nur manuell)",
    periodicScan15m: "Alle 15 Minuten",
    periodicScan30m: "Alle 30 Minuten",
    periodicScan1h: "Stündlich (Empfohlen)",
    periodicScan6h: "Alle 6 Stunden",
    periodicScan24h: "Alle 24 Stunden (Täglich)",
    autoFetchMissingMetadata: "Fehlende Metadaten bei Bibliotheks-Scan abrufen",
    autoFetchMissingMetadataHelp:
      "Sucht bei Scans automatisch nach Metadaten auf TMDB und MusicBrainz für Einträge ohne Beschreibung oder Cover.",
    lastScanned: "Zuletzt gescannt",
    neverScanned: "Noch nie gescannt",
    pluginDemoUnavailable: "Demo — Wiedergabe nicht verfügbar",
    pluginDemoHelp:
      "Dieser Anbieter enthält Katalogbeispiele. Streaming ist nicht verbunden; die Einträge können nicht abgespielt werden.",
    // Navigation
    home: "Startseite",
    libraries: "Mediathek",
    search: "Suche",
    liveTv: "Live-TV",
    webtv: "WebTV",
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
    castDiagnosticsTitle: "Diagnostic Cast",
    castDiagnosticsHelp:
      "Journal local du dernier essai Cast. Paramètres URL et identifiants masqués. Aucun envoi.",
    castDiagnosticsCopy: "Copier le journal Cast",
    castDiagnosticsCopied: "Copié",
    castDiagnosticsFallback: "Sélectionnez et copiez le journal ci-dessous.",
    castDiagnosticsLog: "Journal de diagnostic Cast",
    onlineRecent: "Lectures en ligne récentes",
    onlinePlay: "Lire avec reprise",
    onlinePlayerTitle: "Lecteur en ligne",
    onlinePlayerLoading: "Vérification de la lecture…",
    onlinePlayerError: "Lecture impossible ou accès révoqué. Vérifiez la source.",
    onlinePlayerHelp:
      "La position est enregistrée pour ce compte. Le HLS VOD non chiffré passe par MBL.",

    onlineProviderSettings: "Paramètres de recherche",
    onlineProviderHelp:
      "Utilisez Mwmbl ou votre instance SearXNG HTTPS publique avec recherche JSON. Le classement utilise le titre et des indices de lecture ; la lecture reste à vérifier.",
    onlineProviderSave: "Enregistrer la recherche",
    onlineProviderSaved: "Enregistré. Relancez la recherche du titre.",

    onlineTitle: "WebTV — Médias en ligne",
    onlineDescription:
      "Recherchez et diffusez des chaînes, séries et vidéos en ligne directement dans Media Lounge Box.",
    onlineSearchLabel: "Titre ou série",
    onlineSearchAction: "Rechercher sur le Web",
    onlineSearching: "Recherche sur le Web…",
    onlineSearchError: "La recherche Internet a échoué ou a expiré. Réessayez.",
    onlineSourceUrlLabel: "Vérifier une adresse source publique",
    onlineCheck: "Vérifier la source",
    onlineChecking: "Vérification de la source…",
    onlineSourceError:
      "Impossible de vérifier la source. Utilisez une adresse HTTP(S) publique et réessayez.",
    onlineOpen: "Ouvrir le site",
    onlinePreview: "Aperçu",
    onlinePreviewHelp:
      "Cet aperçu utilise un média direct et ne met pas à jour l’historique de la bibliothèque.",
    onlinePreviewError:
      "Le navigateur n’a pas pu lire cette source. Ouvrez le site ou vérifiez-la à nouveau.",
    onlinePreviewLoading: "Chargement de l’aperçu…",
    onlinePreviewReady: "Utilisez les commandes du lecteur pour démarrer la lecture.",
    onlinePlaying: "Lecture en cours",
    onlineUnchecked: "Non vérifiée",
    onlineExternalPage: "Page Web trouvée",
    onlineExternalPlayer: "Source HLS — vérifier dans le lecteur",
    onlineMediaDetected: "Échantillon multimédia vérifié",
    onlineRestricted: "Accès restreint ou connexion requise",
    onlineUnavailable: "Indisponible",
    onlineUnknown: "Lecture non confirmée",
    onlineSeason: "Saison",
    onlineEpisode: "Épisode",
    onlineCheckedSource: "Source vérifiée",
    onlineProvider: "Service de recherche",
    onlineResults: "Résultats Web",
    onlineNoResults: "Aucun résultat. Essayez d’autres mots ou une adresse source connue.",
    onlineIndexed: "Mes sources vérifiées",
    onlineVerifying: "Vérification des sources…",
    onlineNoVerified:
      "Aucune source lisible vérifiée trouvée. Essayez d'autres mots-clés ou une adresse de source connue.",
    onlineNotVerified:
      "La source n'a pas pu être vérifiée comme lisible dans MBL ; elle n'est donc pas listée.",
    onlineVerifiedResults: "Sources vérifiées",
    catalogueLoading: "Chargement du catalogue…",
    catalogueFailed: "Le catalogue n’a pas pu être chargé ou le délai a expiré. Réessayez.",
    catalogueEmpty: "Aucun élément dans le catalogue.",
    retry: "Réessayer",
    folderRoots: "Dossiers de bibliothèque",
    folderPath: "Chemin du dossier sur le serveur",
    folderOpen: "Ouvrir",
    folderCurrent: "Actuel",
    folderServerRoots: "Racines du serveur",
    folderAddCurrent: "Ajouter ce dossier",
    folderLoading: "Chargement des dossiers…",
    folderUp: "Dossier parent",
    folderAdd: "Ajouter",
    folderEmpty: "Aucun sous-dossier. Le dossier actuel peut être ajouté.",
    folderRemove: "Retirer le dossier",
    folderRequestFailed: "Impossible d’ouvrir le dossier",
    folderAccessHelp:
      "Le compte du service doit pouvoir lire ce dossier et traverser ses dossiers parents. Les paquets Linux utilisent le compte mbl. Saisir un chemin ne modifie pas les permissions.",
    folderTimeout: "Le chargement des dossiers a expiré. Réessayez.",
    folderRootWarning:
      "Un système de fichiers ou disque entier est sélectionné. Il contient des fichiers système et empêche les bibliothèques imbriquées. Choisissez un dossier multimédia dédié.",
    apiReference: "Référence API",
    distributedNodes: "Nœuds distribués",
    mediaRequests: "Demandes de médias",
    libraryScanningTitle: "Analyse de la médiathèque & Automatisation",
    libraryScanningDesc:
      "Surveillance des dossiers en temps réel, recherche des métadonnées manquantes et analyses périodiques",
    autoScanOnChange: "Analyser lors de modifications",
    autoScanOnChangeHelp:
      "Surveille les dossiers multimédias en temps réel et analyse automatiquement les fichiers ajoutés ou modifiés.",
    periodicScanInterval: "Intervalle d'analyse automatique",
    periodicScanDisabled: "Désactivé (Manuel uniquement)",
    periodicScan15m: "Toutes les 15 minutes",
    periodicScan30m: "Toutes les 30 minutes",
    periodicScan1h: "Toutes les heures (Recommandé)",
    periodicScan6h: "Toutes les 6 heures",
    periodicScan24h: "Toutes les 24 heures",
    autoFetchMissingMetadata: "Rechercher automatiquement les métadonnées manquantes",
    autoFetchMissingMetadataHelp:
      "Interroge automatiquement TMDB et MusicBrainz lors de l'analyse pour les médias sans résumé ni affiche.",
    lastScanned: "Dernière analyse",
    neverScanned: "Jamais analysé",
    pluginDemoUnavailable: "Démo — lecture indisponible",
    pluginDemoHelp:
      "Ce fournisseur contient des exemples de catalogue. Le streaming n’est pas connecté et ces éléments ne peuvent pas être lus.",
    // Navigation
    home: "Accueil",
    libraries: "Médiathèques",
    search: "Rechercher",
    liveTv: "TV en direct",
    webtv: "WebTV",
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
    castDiagnosticsTitle: "Diagnóstico Cast",
    castDiagnosticsHelp:
      "Registro local del último intento de Cast. Parámetros URL y credenciales ocultos. No se envía nada.",
    castDiagnosticsCopy: "Copiar registro Cast",
    castDiagnosticsCopied: "Copiado",
    castDiagnosticsFallback: "Seleccione y copie el registro de abajo.",
    castDiagnosticsLog: "Registro de diagnóstico Cast",
    onlineRecent: "Reproducciones en línea recientes",
    onlinePlay: "Reproducir y reanudar",
    onlinePlayerTitle: "Reproductor en línea",
    onlinePlayerLoading: "Verificando reproducción…",
    onlinePlayerError: "Error de reproducción o acceso revocado. Compruebe la fuente.",
    onlinePlayerHelp: "La posición se guarda para esta cuenta. MBL retransmite HLS VOD sin cifrar.",

    onlineProviderSettings: "Ajustes de búsqueda",
    onlineProviderHelp:
      "Use Mwmbl o su instancia pública HTTPS de SearXNG con búsqueda JSON. El orden usa títulos e indicios de reproducción; aún hay que comprobarla.",
    onlineProviderSave: "Guardar ajustes de búsqueda",
    onlineProviderSaved: "Guardado. Busque el título de nuevo.",

    onlineTitle: "WebTV — Medios en línea",
    onlineDescription:
      "Busque y reproduzca canales, series y vídeos en línea directamente en Media Lounge Box.",
    onlineSearchLabel: "Título o serie",
    onlineSearchAction: "Buscar en la Web",
    onlineSearching: "Buscando en la Web…",
    onlineSearchError: "La búsqueda falló o agotó el tiempo. Inténtalo de nuevo.",
    onlineSourceUrlLabel: "Comprobar una dirección pública",
    onlineCheck: "Comprobar fuente",
    onlineChecking: "Comprobando fuente…",
    onlineSourceError:
      "No se pudo comprobar la fuente. Usa una dirección HTTP(S) pública e inténtalo de nuevo.",
    onlineOpen: "Abrir sitio web",
    onlinePreview: "Vista previa",
    onlinePreviewHelp:
      "Esta es una vista previa de medios directos y no actualiza el historial de la biblioteca.",
    onlinePreviewError:
      "El navegador no pudo reproducir esta fuente. Abre el sitio o vuelve a comprobarla.",
    onlinePreviewLoading: "Cargando vista previa…",
    onlinePreviewReady: "Usa los controles del reproductor para iniciar la reproducción.",
    onlinePlaying: "Reproduciendo",
    onlineUnchecked: "Sin comprobar",
    onlineExternalPage: "Página web encontrada",
    onlineExternalPlayer: "Fuente HLS — comprobar en el reproductor",
    onlineMediaDetected: "Muestra multimedia verificada",
    onlineRestricted: "Acceso restringido o inicio de sesión necesario",
    onlineUnavailable: "No disponible",
    onlineUnknown: "Reproducción sin confirmar",
    onlineSeason: "Temporada",
    onlineEpisode: "Episodio",
    onlineCheckedSource: "Fuente comprobada",
    onlineProvider: "Servicio de búsqueda",
    onlineResults: "Resultados web",
    onlineNoResults: "Sin resultados. Prueba otras palabras o una dirección conocida.",
    onlineIndexed: "Mis fuentes comprobadas",
    onlineVerifying: "Verificando fuentes…",
    onlineNoVerified:
      "No se encontró ninguna fuente reproducible verificada. Prueba otras palabras o una dirección de fuente conocida.",
    onlineNotVerified:
      "La fuente no pudo verificarse como reproducible en MBL, por lo que no se lista.",
    onlineVerifiedResults: "Fuentes verificadas",
    catalogueLoading: "Cargando catálogo…",
    catalogueFailed: "No se pudo cargar el catálogo o se agotó el tiempo. Inténtalo de nuevo.",
    catalogueEmpty: "No se encontraron entradas.",
    retry: "Reintentar",
    folderRoots: "Carpetas de biblioteca",
    folderPath: "Ruta de la carpeta en el servidor",
    folderOpen: "Abrir",
    folderCurrent: "Actual",
    folderServerRoots: "Raíces del servidor",
    folderAddCurrent: "Añadir actual",
    folderLoading: "Cargando carpetas…",
    folderUp: "Carpeta superior",
    folderAdd: "Añadir",
    folderEmpty: "Sin subcarpetas. Puedes añadir la carpeta actual.",
    folderRemove: "Quitar carpeta",
    folderRequestFailed: "No se puede abrir la carpeta",
    folderAccessHelp:
      "La cuenta del servicio necesita leer esta carpeta y acceder a sus carpetas superiores. Los paquetes Linux usan la cuenta mbl. Introducir una ruta no cambia los permisos.",
    folderTimeout: "La solicitud de carpetas agotó el tiempo. Inténtalo de nuevo.",
    folderRootWarning:
      "Se ha seleccionado un sistema de archivos o unidad completos. Incluye archivos del sistema e impide bibliotecas superpuestas. Elige una carpeta dedicada a medios.",
    apiReference: "Referencia de API",
    distributedNodes: "Nodos distribuidos",
    mediaRequests: "Solicitudes de medios",
    libraryScanningTitle: "Escaneo de biblioteca y automatización",
    libraryScanningDesc:
      "Monitoreo de carpetas en tiempo real, búsqueda de metadatos faltantes y escaneos programados",
    autoScanOnChange: "Escanear al detectar cambios",
    autoScanOnChangeHelp:
      "Supervisa las carpetas en tiempo real y analiza automáticamente los archivos nuevos o modificados tras completarse.",
    periodicScanInterval: "Intervalo de escaneo periódico",
    periodicScanDisabled: "Desactivado (Solo manual)",
    periodicScan15m: "Cada 15 minutos",
    periodicScan30m: "Cada 30 minutos",
    periodicScan1h: "Cada hora (Recomendado)",
    periodicScan6h: "Cada 6 horas",
    periodicScan24h: "Cada 24 horas",
    autoFetchMissingMetadata: "Buscar metadatos faltantes automáticamente al escanear",
    autoFetchMissingMetadataHelp:
      "Consulta automáticamente TMDB y MusicBrainz al escanear para contenidos que carecen de sinopsis o portada.",
    lastScanned: "Último escaneo",
    neverScanned: "Nunca escaneado",
    pluginDemoUnavailable: "Demo — reproducción no disponible",
    pluginDemoHelp:
      "Este proveedor contiene ejemplos de catálogo. El streaming no está conectado y estas entradas no se pueden reproducir.",
    // Navigation
    home: "Inicio",
    libraries: "Bibliotecas",
    search: "Buscar",
    liveTv: "TV en vivo",
    webtv: "WebTV",
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
