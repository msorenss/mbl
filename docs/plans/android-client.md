# Plan: MBL Android-klient (M11.1, förberedd för M11.2)

Status: förslag, 2026-11-03. Underlag: `IMPLEMENTATION.md` M11.1/M11.2, `PLAN.md` §8.3, §10.3, §10.9, `docs/openapi.json` (v1.2.2, 165 paths) och aktuell serverkod.

## 1. Mål och avgränsning

**Mål för första användbara version (MVP):** en native Android-app för telefon och surfplatta som kan:

1. hitta och ansluta till en MBL-server (LAN, Tailscale, publik URL),
2. logga in (lösenord, TOTP, Quick Connect) och byta profil (med PIN),
3. bläddra i hem, bibliotek, serier → säsonger → avsnitt och sökning,
4. spela video med Media3/ExoPlayer: direktuppspelning i första hand, servernegotierad HLS annars, med resume, spårbyte, undertexter, hoppa över intro/eftertexter, Up Next, PiP och gester,
5. rapportera tittartillstånd exakt som webbspelaren gör (heartbeat, stop, återupptagning efter utgången session).

**Därefter i ordning (beslut §9):** Android TV-appen (M11.2) som återanvänder samma kärnmoduler, sedan musik i bakgrunden (låsskärm, Android Auto), offline-nedladdningar, fjärrstyrning/Cast och Live TV/DVR. Distribution via Google Play.

**Utanför scope tills vidare:** Quest/VR (M11.3/M11.4), iOS och push-notiser via FCM.

## 2. Vad servern redan erbjuder (verifierat i koden)

| Behov | Befintligt stöd |
|---|---|
| Serveridentitet | `GET /api/v1/system/info` (namn, version, commit, plattform) |
| Inloggning | `POST /api/v1/auth/login` (`clientName`, `deviceName`, `totpCode`) → Bearer-token; 30 dagars glidande session (`SESSION_TTL_MS`) |
| TV-/kodinloggning | `POST /api/v1/auth/quick-connect` + `/authorize` + `/poll` |
| Profiler | `GET /api/v1/users/profiles`, `POST /api/v1/users/{id}/pin/verify` |
| Preferenser | `GET/PATCH /api/v1/users/me/preferences` (tema, accent, språk, undertextstil, ljudsynk) |
| Bläddring | `/hubs/home`, `/libraries/{id}/items` (cursor, sort, filter), `/items/{id}`, `/items/{id}/children`, `/search`, `/images/{id}?w=&q=&blur=` |
| Uppspelning | `POST /api/v1/playback` med **fullständig dynamisk `DeviceProfile`** (`crates/mbl-playback/src/profile.rs`), `waitForReady`, `maxBitrate`, ljud-/undertextindex, `audioOffsetMs`, `startMs`; svar med metod, URL, skäl, spår, markörer och `hlsGeneratedTimeline` |
| Sessionslivscykel | `/playback/{session}/progress` (även vid paus), `/stop`, `/diagnostics`; 404/410 betyder att sessionen gått ut och att uppspelningen ska återupptas |
| Media | `/stream/direct/{partId}` (Range, `?token=`), HLS fMP4, `/stream/subtitles/{partId}/{target}`, trickplay-index/sprites, `/stream/audio/{partId}` |
| Musik | `/stream/audio`, OpenSubsonic `/rest/*` |
| Fjärrstyrning | `POST /api/v1/players`, `GET /players/{id}/connect` (WebSocket-kommandon `play`/`pause`/`resume`/`seek`/…), `/players/{id}/command` |
| Händelser | `GET /api/v1/events` (WebSocket, Bearer fungerar) |
| Live TV | kanaler, EPG, serverägt HLS-relä med lease, DVR |

**Slutsats:** klienten kan byggas mot nuvarande API till och med MVP utan att API:t bryts. De luckor som finns ligger i §3.

## 3. Serverändringar som behövs (Rust, små och tidiga)

Varje ny endpoint följer AGENTS §3.5–3.6: registrera den i `openapi.rs`, regenerera `docs/openapi.json` och TS-typerna, och lägg till ett MCP-verktyg eller en `EXCLUSIONS.toml`-post.

**Status 2026-11-03:** S1–S7 är implementerade på servern (S7 även i webben), se §3.1.

| # | Ändring | Varför | Fas |
|---|---|---|---|
| S1 ✅ | **mDNS-annonsering** `_mbl._tcp.local.` (t.ex. `mdns-sd`), TXT: `id`, `version`, `https_port`, `path=/` | Idag annonserar servern ingenting. `mbl-tv` probar bara fasta URL:er. Krävs för automatisk upptäckt via Android NSD. | A0 |
| S2 ✅ | **Stabilt server-ID + anslutningskandidater** i `system/info` (eller ny `GET /api/v1/system/endpoints`): LAN-URL:er, tailnet-URL, publik URL och CA-fingeravtryck | Appen ska kunna växla LAN ↔ Tailscale ↔ WAN utan ny inloggning och känna igen samma server | A0 |
| S3 ✅ | **Inbyggd basprofil `android-exoplayer`** (TOML) och regressionstest med typisk telefonprofil (HEVC Main10, HDR10, E-AC-3, ASS → WebVTT) | Ger en säker reserv om den dynamiska profilen saknas eller är ofullständig, och testar `deny_unknown_fields`-kontraktet | A3 |
| S4 ✅ | **Offline-API:** `POST /api/v1/items/{id}/downloads` (original eller kvalitetsförval → filjobb via optimize-pipelinen), status/radering och nedladdnings-URL med Range | Saknas helt idag | A5 |
| S5 ✅ | **Batchsynk av offline-progress** med klienttidsstämplar (senaste skrivning vinner per objekt) | Offline-tittande måste kunna synkas utan att skriva över nyare position | A5 |
| S6 ✅ | **Synkade sångtexter** (LRC från sidofil/tagg), om de inte redan exponeras via OpenSubsonic `getLyricsBySongId` | Behövs för M11.1-kravet "synced lyrics" | A4 |
| S7 ✅ | **Kontoradering/begäran om radering** från appen och webben | Play-krav när appen kan skapa konton (`register-invite`) | A1 (före första Play-inskick) |

### 3.1 Implementerat (S1–S7)

| # | Implementation |
|---|---|
| S1 | `crates/mbl-server/src/mdns.rs`: `_mbl._tcp.local.` med TXT `id`, `version`, `path=/` och `https_port` när HTTPS är på. Styrs av `server.mdns` (standard `true`). |
| S2 | Stabilt `serverId` (32 hex, lagras i `settings`) i `GET /api/v1/system/info` och oautentiserad `GET /api/v1/system/endpoints` (`lanUrls`, `tailnetUrls`, `publicUrl`, `httpsPort`, `caSha256`). |
| S3 | `crates/mbl-playback/profiles/android-exoplayer.toml` med snapshot-regression i `mbl-playback/tests/decide.rs`. |
| S4 | Migration `0014_downloads.sql`, `POST /api/v1/items/{id}/downloads` (`original`, `universal_1080p`, `universal_720p`), `GET /api/v1/downloads`, `GET`/`DELETE /api/v1/downloads/{id}` och `GET /api/v1/downloads/{id}/file` (Range/HEAD, `Content-Disposition`). Optimerade kvaliteter körs som jobbet `download.transcode` och cachas per part/kvalitet. |
| S5 | `POST /api/v1/users/me/progress/sync`: upp till 500 poster med `updatedAt`, senaste skrivning vinner. Utfall per post: `applied`, `stale` eller `not_found`. |
| S6 | `GET /api/v1/items/{id}/lyrics`: `.lrc`-sidofil först, annars inbäddad tagg. Synkade rader i ms. |
| S7 | `DELETE /api/v1/users/me` med lösenord. Hanterade profiler och sista admin nekas. Knapp på kontosidan i webben. |

Alla nya vägar är undantagna i `crates/mbl-mcp/EXCLUSIONS.toml` (klientfunktioner, inte adminverktyg). Integrationstester: `crates/mbl-server/tests/android.rs`.

## 4. Teknisk arkitektur

### 4.1 Stack

| Område | Val | Motivering |
|---|---|---|
| Språk/UI | Kotlin 2.x, Jetpack Compose, Material 3 med eget MBL-tema | Krav i M11.1; delas med Compose for TV i M11.2 |
| SDK-nivåer | `minSdk 26` (Android 8.0), `target/compileSdk 35+` | PiP, `MediaCodecInfo`-prestanda och adaptiva ikoner; täcker i praktiken alla aktiva enheter |
| Spelare | AndroidX Media3: ExoPlayer, `media3-exoplayer-hls`, `media3-session`, `media3-ui-compose`, `DownloadManager` | HW-avkodning, HLS fMP4, MediaSession och nedladdningar i samma familj |
| Nätverk | OkHttp + Retrofit + kotlinx.serialization; Media3 `OkHttpDataSource` med samma klient | En gemensam autentiserings-/TLS-stack för API, bilder och media |
| API-modeller | Genereras från `docs/openapi.json` (openapi-generator `kotlin`, kotlinx-serialization) och checkas in; `DeviceProfile`/`PlayerCommand` skrivs för hand | OpenAPI 3.1-stödet i generatorn är ofullständigt (`type: [x, "null"]`, `oneOf`). Ett kontraktstest jämför modellerna mot schemat (se §7). |
| Arkitektur | Unidirectional data flow: Compose → ViewModel (`StateFlow`) → Repository → API/Room | Testbart och standard i Android |
| DI | Hilt | Standard och bra stöd för ViewModel/WorkManager/Service |
| Bilder | Coil 3 via OkHttp, alltid `?w=` efter faktisk vybredd | Sparar bandbredd på mobilnät |
| Listor | Paging 3 mot cursor-API:t | `/libraries/{id}/items` är redan cursorbaserat |
| Lokal data | Room (cache och nedladdningar), DataStore (inställningar), Keystore-krypterad lagring för token | Token får aldrig ligga i klartext |
| Navigation | Navigation Compose (typsäkra rutter), adaptiv layout (`NavigationSuiteScaffold`: bottenflik på telefon, rail på surfplatta) | Följer PLAN §10.3 |

### 4.2 Modulstruktur (`clients/android/`, separat Gradle-bygge utanför Cargo)

```
clients/android/
  settings.gradle.kts, gradle/libs.versions.toml (versionskatalog)
  build-logic/                  konventionsplugins (android-app, android-lib, compose, hilt)
  app-mobile/                   telefon/surfplatta: Activity, navigation, PiP, manifest
  app-tv/                       (M11.2) Compose for TV, återanvänder core/feature-logik
  core/model                    rena Kotlin-modeller och DeviceProfile
  core/network                  genererad API-klient, auth-interceptor, TLS/CA-pinning, mDNS (NSD)
  core/data                     repositories, Room, DataStore, sessionshantering, multi-server
  core/designsystem             genererade tokens (färg, typografi, radier, rörelse) + komponenter
  core/player                   Media3-integration, profilbyggare, uppspelningssession, MediaSession
  core/i18n                     genererade strings.xml (en, sv, de, fr, es)
  feature/onboarding            serverval, inloggning, Quick Connect, profilväljare
  feature/home, library, detail, search
  feature/player                spelar-UI, gester, spår, Up Next, trickplay, statistik
  feature/music                 bibliotek, kö, Now Playing, sångtexter
  feature/downloads             (A5)
  feature/remote                (A6) fjärrkontroll och Cast
  feature/livetv                (A7)
  feature/settings
```

### 4.3 Delade källor med resten av repot

- **Designtokens:** utöka `cargo xtask tokens` så att `design/tokens.json` också genererar `core/designsystem/.../MblTokens.kt`, och låt `--check` täcka den filen. Då förblir webb och Android i synk.
- **Översättningar:** ny `cargo xtask android-strings`, eller ett Gradle-steg, som exporterar delmängden av `web/src/i18n` till `values-*/strings.xml` med `--check`. Appspecifika strängar läggs till i samma källa.
- **API-modeller:** en `xtask`/Gradle-uppgift regenererar Kotlin-klienten från `docs/openapi.json`. CI misslyckas om den är inaktuell, på samma sätt som `openapi-check`.

### 4.4 Uppspelningsflöde (måste matcha webbspelarens verifierade semantik)

1. **Profilbyggare:** `MediaCodecList` (codecs, profiler/nivåer, max upplösning), `Display.hdrCapabilities`, `AudioCapabilities` (passthrough), skärmstorlek och nätverkstyp (mobil, Wi-Fi eller VPN) ger en `DeviceProfile`. Profilen cachas per enhet och visas i statistikpanelen.
2. `POST /playback` med profil, `startMs` (heltal; heltalsgolvet var en tidigare Cast-bugg), valda spår och `waitForReady: true`.
3. **Direct:** progressiv källa via OkHttp med Range. **HLS:** `HlsMediaSource`. Bearer-header används för API och HLS, och `?token=` för direct URL:er som servern signerar.
4. **Heartbeat:** var 5:e sekund, även vid paus/buffring och när appen återkommer till förgrunden. Först en verklig `isPlaying` bekräftar sessionen.
5. **404/410 på heartbeat:** visa "Fortsätt uppspelning" vid aktuell position och behåll spår, synk och kvalitet.
6. **Spår-, kvalitets- och synkbyte:** förhandla en ny session vid aktuell position. Byt först när den nya sessionen är klar och stoppa sedan den gamla (samma regler som batch 6, 14 och 15).
7. **`hlsGeneratedTimeline`:** sök bortom indexerat prefix genom ny förhandling, inte med ExoPlayer-seek.
8. **Stop:** ett idempotent anrop med timeout. Nästa avsnitt väntar på att stoppet blir klart (som batch 8).
9. **Fel:** permanenta HTTP-/avkodningsfel ger en tydlig vy med Retry/Resume. Övriga fel får begränsade återförsök (som `hlsRecovery.ts`).

## 5. Byggmiljö och verktyg

**Begränsning:** Pi:n är aarch64 med 16K sidstorlek och cirka 4 GB ledigt. Google levererar inte Android build-tools/`aapt2` för linux-arm64, och SDK plus Gradle-cache kräver flera GB.

| Väg | Användning |
|---|---|
| **GitHub Actions (primär)** | Ny `.github/workflows/android.yml` (ubuntu, JDK 21, Gradle-cache) kör `lint`, enhetstester och `assembleDebug`, och laddar upp APK som artefakt vid varje push. `release.yml` bygger en signerad release-APK och bifogar den till GitHub Release. |
| **PC med Android Studio (rekommenderad för utveckling)** | Emulator, layoutinspektör, profilering och felsökning på telefon |
| **Pi:n** | Endast `adb` (`apt install adb`) för att installera CI-byggd APK på telefonen och läsa `logcat`. Inget lokalt Android-bygge om inte disk frigörs och en community-byggd arm64-`aapt2` används via `android.aapt2FromMavenOverride` (rekommenderas inte). |

## 6. Faser med leverabler och acceptanskriterier

Varje fas avslutas med grön CI, uppdaterade `IMPLEMENTATION.md`/`AGENTS.md` och verifiering på en riktig telefon mot servern.

### A0: Beslut, skelett och serverförutsättningar
- ADR `docs/adr/0008-android-client-stack.md` (stack, moduler, byggväg, distribution).
- `clients/android/` med versionskatalog, konventionsplugins, tom app och tema från genererade tokens.
- `android.yml` i CI: lint, enhetstester och debug-APK som artefakt.
- Server: S1 (mDNS) och S2 (server-ID och anslutningskandidater) med tester, OpenAPI och MCP-paritet.
- **Klart när:** CI producerar en installerbar APK som startar, och `dns-sd`/NSD hittar servern på LAN.

### A1: Anslutning, inloggning och profiler
- Serverväljare: NSD-upptäckt, manuell URL, flera sparade servrar och automatiskt val av första nåbara kandidat (LAN före tailnet före WAN).
- TLS: HTTPS med systemets CA. **MBL Local CA** godkänns via TOFU: hämta `ca.pem`, visa SHA-256-fingeravtryck, låt användaren bekräfta och pinna det per server. Ingen global användar-CA krävs. Klartext-HTTP tillåts bara mot privata adresser och tailnet-adresser och ger varning i UI.
- Inloggning med TOTP-steg. Alternativt Quick Connect: appen visar en kod som godkänns i webben. Omvänt flöde, där telefonen godkänner TV-kod, kommer i A6.
- Profilväljare med PIN och åldersbegränsade profiler. Token lagras krypterad. Utloggning revokerar sessionen på servern.
- **Klart när:** inloggning mot 8420 överlever omstart, profilbyte respekterar PIN och rating, och växling Wi-Fi → mobil + Tailscale behåller inloggningen.

### A2: Bläddring
- Hem-hubbar (Continue Watching, Up Next, Recently Added) enligt användarens `homeLayout`/`hiddenHubs`.
- Bibliotek med Paging, sortering och filter. Detaljvy för film, serie, säsong och avsnitt med korrekt navigering, vilket webben behövde rätta i batch 6.
- Sökning, sett/osett och resume-indikator. Room-cache för snabb kallstart och läsning utan nät.
- Tema och accent från `preferences`. Fem språk.
- **Klart när:** Dark Matter-serien kan bläddras Show → Season → Episode, bilder laddas i rätt storlek och listorna scrollar utan hack på en mellanklassmobil.

### A3: Videospelare (MVP-kärnan)
- Profilbyggare (§4.4) och server-S3. Direct/HLS, resume, heartbeat, stop och återupptagning.
- Spelar-UI i Compose: gester (svep för ljusstyrka/volym, dubbeltryck ±10 s, nyp för zoom/fyll), låst läge, PiP och automatisk rotation.
- Ljud- och undertextval via omförhandling, undertextstil från preferenser, kvalitetsgräns för Wi-Fi/mobil och ljudsynk.
- Markörer (Skip Intro/Credits), Up Next med automatisk fortsättning (opt-in) och trickplay-förhandsvisning vid scrubbning.
- Statistikpanel: metod, serverns skäl, codecs, bitrate, buffert och tappade bildrutor (`/diagnostics`).
- Spelaren registreras som `device_type: "android"` i `PlayerRegistry`.
- **Klart när:** H.264/E-AC-3 MKV spelas, HEVC 10-bit direktuppspelas på en kapabel telefon, spårbyte bevarar positionen, PiP fungerar, en 10 minuters paus följt av fortsättning fungerar, och progress syns direkt i webben.

> **MVP = A0–A3.** Kan installeras och användas på riktigt.

### A4: Musik och bakgrundsljud
- `MediaLibraryService` + `MediaSession`: notis, låsskärm, Bluetooth-knappar och ljudfokus.
- Musikbibliotek: artist, album, spår, spellistor och kö. Gapless-uppspelning. `/stream/audio` med transkodning efter nätverkstyp.
- Android Auto genom att samma `MediaLibraryService` exponerar ett bläddringsträd.
- Synkade sångtexter (S6).
- **Klart när:** musik spelar vidare med skärmen avstängd, låsskärmskontroller fungerar och Android Auto (Desktop Head Unit) kan bläddra och spela.

### A5: Offline-nedladdningar
- Server S4 och S5.
- Media3 `DownloadManager` och foreground service. Förval: Original, 1080p/8 Mbps, 720p/4 Mbps och 480p/1,5 Mbps. Endast-Wi-Fi-inställning, lagringsöversikt, radering och nedladdning av hela säsong.
- Offline-uppspelning med lokal progress som synkas vid anslutning, där senaste tidsstämpel vinner.
- **Klart när:** ett avsnitt laddas ned, spelas i flygplansläge, och positionen syns i webben efter återanslutning.

### A6: Fjärrstyrning och Cast
- Telefonen som mål: WebSocket `/players/{id}/connect` gör att webb, Home Assistant och MCP kan styra telefonens spelare.
- Telefonen som fjärrkontroll för andra MBL-spelare (lista via `GET /players`) och godkännande av Quick Connect-koder (telefonen loggar in TV:n).
- Google Cast-sändare (`play-services-cast-framework`) med serverns `chromecast`-profil och samma lärdomar som webbens Cast-arbete: fMP4-segmentformat, heltals-`startMs`, inga sidecar-spår i HLS när undertext är av, och receiver-nåbar URL.
- **Klart när:** webben pausar telefonen, och telefonen castar till Bedroom TV med resume och undertextbyte.

### A7: Live TV, DVR och övrigt
- Kanaler, nu/nästa, EPG-rutnät, liveuppspelning med lease-förnyelse och release, DVR-schemaläggning och inspelningslista. Eventuellt WebTV och fotos.
- **Klart när:** en kanal kan ses, kanalbyte släpper tunern och en inspelning kan schemaläggas och spelas.

### A8: Härdning och release
- R8/minifiering, baseline profiles, TalkBack/tillgänglighet (WCAG-mål som webben), stora teckensnitt, mörkt/OLED-tema och surfplattelayout.
- Signerad AAB till Google Play (internt test → stängt test → produktion), Play App Signing, store-listning, data safety och integritetspolicy. APK finns kvar som CI-artefakt för sideload.
- Lokal kraschlogg som kan exporteras, i stil med webbens Cast-diagnostik (maskerad och utan tredjepartstjänst).
- Dokumentation: `docs/book/src/android.md`.

### Därefter: M11.2 Android TV
`app-tv` med Compose for TV, D-pad-fokus, AFR (`Surface.setFrameRate`/`Display.Mode`), HDR-matchning, passthrough via `AudioCapabilities` och Watch Next-kanaler. `core/*` och spelarsessionslogiken återanvänds utan ändring, vilket är skälet till modulgränserna ovan.

## 7. Teststrategi

| Nivå | Verktyg | Täcker |
|---|---|---|
| Enhet (JVM) | JUnit, Turbine, kotlinx-coroutines-test, MockWebServer | ViewModels, repositories, sessionslivscykel (heartbeat, 404/410, stop-ordning) och profilbyggare med fejkade codec-listor |
| Kontrakt | Test som läser `docs/openapi.json` och avserialiserar exempel/fixtures till Kotlin-modellerna | Att API-ändringar på servern upptäcks i CI |
| Compose UI | Robolectric + Compose test | Navigering, felvyer, PIN och spelarkontroller |
| End-to-end (A3+) | CI-jobb som bygger `mbl-server`, startar det med `test-media/`, kör emulator (`android-emulator-runner`) och instrumenterat test: logga in, spela, kontrollera progress via API | Verklig integration utan fysisk enhet |
| Manuellt | Riktig telefon mot 8420 och 18421, Bedroom TV för Cast | Hårdvaruavkodning, HDR, PiP, Bluetooth och Android Auto (DHU) |

## 8. Risker och motåtgärder

| Risk | Motåtgärd |
|---|---|
| OpenAPI 3.1 genereras dåligt till Kotlin | Generera, checka in och handskriv problemtyper. Kontraktstest i CI. |
| Inget lokalt Android-bygge på Pi:n och lite disk | CI-först. Android Studio på PC. Endast `adb` på Pi:n. |
| Fel eller för optimistisk codec-profil ger svart bild eller tyst ljud | Konservativ profil med verifierade codecs och `MediaCodecInfo.isHardwareAccelerated`. Vid avkodningsfel: automatisk ny förhandling med lägre profil och visat skäl. |
| Egen CA och klartext-HTTP | TOFU-pinning per server. Klartext bara privat/tailnet och med varning. |
| Pausade sessioner går ut när Android dödar appen | Heartbeat vid förgrund. 404/410 → Resume vid sparad position (samma modell som webben). |
| Seek i genererad HLS-tidslinje | Respektera `hlsGeneratedTimeline` och förhandla om. |
| Google Play Services saknas (Huawei, F-Droid) | Cast i separat flavor. Appen fungerar fullt utan den. |
| Scope-glidning (VR, TV och allt samtidigt) | Strikt fasordning. TV direkt efter A3, sedan A4 och vidare. |
| Play-avvisning (granskaråtkomst, kontoradering, foreground services) | Kraven i §9.1 byggs in från A0. Demoserver/demoläge och S7 är klara före första inskick. |

## 9. Beslut (tagna 2026-11-03)

| Fråga | Beslut | Konsekvens för planen |
|---|---|---|
| Byggmiljö | GitHub Actions + Android Studio på PC | §5 gäller oförändrad. Pi:n används bara med `adb`. |
| Distribution | **Google Play Store** | Se §9.1. Release-artefakten blir AAB (Android App Bundle) med Play App Signing. En APK byggs fortsatt som CI-artefakt för sideload-test. |
| Google Cast | Ja | `play-services-cast-framework` ingår direkt. Någon separat flavor utan Play Services behövs inte vid Play-distribution. |
| Prioritet efter MVP | **Android TV (M11.2)** före musik och offline | Ny fasordning: A0 → A1 → A2 → A3 → **TV** → A4 → A5 → A6 → A7 → A8. Play-releasekraven (§9.1) arbetas in löpande från A0, inte först i A8. |
| Testenheter | Samsung Galaxy S24, OnePlus Pad, Chromecast med Google TV | Se §9.2. |
| Applikations-ID | `se.mbl.android` | TV-appen publiceras i samma Play-listning, antingen som samma paket med `leanback`-stöd eller som separat paket `se.mbl.android.tv`. Valet görs i TV-fasens ADR. |

### 9.1 Play Store-krav som påverkar designen redan nu
- **Target SDK:** Play kräver aktuell API-nivå för nya appar och uppdateringar. Vi följer den senaste stabila nivån och kontrollerar kravet vid varje release.
- **Foreground service-typer:** spelaren använder `mediaPlayback`, nedladdningar använder `dataSync`. Båda deklareras och motiveras i Play Console.
- **Data safety och integritetspolicy:** appen pratar bara med användarens egen server och har ingen spårning. Policyn publiceras (t.ex. i `docs/book`) och länkas från listningen.
- **Kontoradering:** appen kan skapa konton via inbjudan (`register-invite`). Play kräver då en väg att begära kontoradering, både i appen och via webben. Detta kan kräva en ny serverfunktion (självradering eller begäran till admin). Läggs till som **S7**.
- **Granskningsåtkomst:** Play-granskare behöver en fungerande server. Planera en demoserver eller ett demoläge med `test-media`, annars riskerar appen att avvisas.
- **Android TV i Play:** kräver `LEANBACK_LAUNCHER`, banner, `touchscreen required=false` och full D-pad-navigering. TV-granskningen är strängare och görs separat.
- **Klartext-HTTP:** tillåts bara mot privata adresser och tailnet med tydlig varning (§A1). Detta motiveras i granskningen.

### 9.2 Testmatris (förväntade egenskaper, verifieras i A3/TV)
| Enhet | Roll | Särskilt att verifiera |
|---|---|---|
| Galaxy S24 | Primär telefon | HEVC Main10/HDR10/HDR10+, AV1-avkodning, att Dolby Vision troligen **inte** stöds (Samsung) och därmed HDR10-reserv, PiP och gester |
| OnePlus Pad | Surfplatta | Rail-layout och stor skärm, rotation, HDR-skärm och PiP i fönsterläge |
| Chromecast med Google TV | TV (M11.2) och Cast-mål | D-pad, AFR, HDR/DV-matchning och ljudpassthrough (DD/DD+/Atmos; DTS kan behöva transkodning). Fungerar också som Cast-mottagare för A6. |
