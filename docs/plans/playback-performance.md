# Playback Performance & Format Optimization Plan

Denna plan sammanfattar granskningen av MBL:s uppspelningsarkitektur (både server- och webbklient) genomförd 2026-11-08. Den beskriver exakt vilka format och codecs som stöds, hur de levereras, vilka prestandaflaskhalsar som identifierats i källkoden, samt en prioriterad åtgärdsplan för nästa agent.

---

## 1. Mål & Principer
1. **Sub-sekund uppstart (<500 ms):** Omedelbar start av strömmen oavsett om det är Direct Play eller Direct Stream (remux).
2. **Skonsam resursanvändning på Raspberry Pi 5:**
   - Pi 5 saknar hårdvarubaserad H.264-kodare i kisel (Broadcom BCM2712 har endast HEVC-hårdvaruavkodning via DRM).
   - All video-transkodning sker på ARM Cortex-A76 CPU:n.
   - **Direct Play** och **Direct Stream (Remux med `-c:v copy`)** måste prioriteras kompromisslöst så att CPU-belastningen förblir <3 %.
3. **Mjuk spolning (Seeking) & återupptagning:**
   - Spolning bakåt ska ske direkt ur webbläsarens cache utan nya nätverksanrop.
   - Återupptagning (`start_ms > 0`) ska inte starta FFmpeg vid 0 s för att sedan döda processen.
4. **Högsta ljud- och bildkvalitet:**
   - Direkt passthrough för flerkanalsljud (AC-3, E-AC-3, FLAC, Opus) när hårdvaran stöder det.
   - Automatisk lättviktig ljudtranskodning till AAC/Opus vid TrueHD/DTS medan videon kopieras orörd.

---

## 2. Formatmatris – Vad vi stöder och leveransmetod

### A. Behållare (Containers)
| Behållare | Webbläsare | Android (ExoPlayer) | Serverns hantering |
|---|---|---|---|
| **MP4 / M4V / MOV** | Ja (Native) | Ja (Native) | **Direct Play** (HTTP 206 Range) när codecs stöds. |
| **WebM** | Ja (Chrome/Firefox/Edge) | Ja (Native) | **Direct Play** med VP8/VP9/AV1 + Opus/Vorbis. |
| **MKV (Matroska)** | Kräver fMP4 HLS | Ja (Native) | **Direct Stream (Remux):** Servern packar om MKV till HLS fMP4 med `-c:v copy` och `-c:a copy` (eller audio transcode). Noll kvalitetsförlust, ~1 % CPU. |
| **MPEG-TS (`.ts`, `.m2ts`)** | Kräver fMP4 HLS | Ja (Native) | **Direct Stream (Remux)** via HLS fMP4. |
| **AVI / WMV / VOB / FLV** | Nej | Delvis | **Full Transcode** till H.264 + AAC i HLS fMP4. |

### B. Video-codecs & Profiler
| Codec | Profiler & Begränsningar | Leveransmetod |
|---|---|---|
| **H.264 (AVC)** | Baseline, Constrained Baseline, Main, High upp till Level 5.2. 8-bit. | **Direct Play** överallt. (10-bit H.264 transkodas p.g.a. brist på hårdvaruavkodning). |
| **HEVC (H.265)** | Main, Main 10 (8/10-bit), HDR10, HLG, upp till 4K@60fps. | **Direct Play / Direct Stream** på Safari, Edge, Android, Smart TV och Chrome (om OS-dekoder finns). |
| **AV1 (AV01)** | Main Profile, 8/10-bit, upp till 4K. | **Direct Play** på kompatibla webbläsare och Apple-enheter. Android-klienten kan välja Direct Play när den rapporterar en kompatibel dekoder i sin profil. |
| **VP9** | Profile 0 (8-bit), Profile 2 (10-bit HDR), upp till 4K. | **Direct Play** i WebM och MP4 på Chrome, Firefox, Edge, Android. |
| **VP8** | Standard 8-bit. | **Direct Play** i WebM på PC och Android. |
| **MPEG-2 / VC-1** | Gamla TV-sändningar, DVD, tidiga Blu-ray. | **Transkodas** till H.264. |

### C. Ljud-codecs & Kanaler
| Codec | Max kanaler | Leveransmetod |
|---|---|---|
| **AAC (LC, HE)** | Stereo, 5.1 | **Direct Play** överallt. Universellt transkodningsmål. |
| **MP3** | Stereo | **Direct Play** överallt. |
| **Opus** | Stereo, 5.1 | **Direct Play** i moderna webbläsare och Android. |
| **FLAC** | Stereo, 5.1, 7.1 | **Direct Play** (Lossless) i Chrome, Firefox, Safari, Android. |
| **AC-3 (Dolby Digital)** | Upp till 5.1 | **Direct Play** på Safari, Edge, Android, Chromecast; snabb **ljudtranskodning** till AAC (med video copy) om webbläsaren saknar licens. |
| **E-AC-3 (Dolby Digital Plus / Atmos)** | Upp till 7.1 | **Direct Play** på Edge, Safari, Android, Smart TV. |
| **TrueHD / DTS / DTS-HD MA** | 5.1, 7.1 | Webbläsare saknar stöd: **ljudtranskodning till AAC/Opus medan videon kopieras (`-c:v copy`)**. Tar <3 % CPU på Pi 5. |
| **ALAC** | Stereo, 5.1 | **Direct Play** på Apple-enheter, transkodas annars till AAC/FLAC. |

### D. HDR & Färghantering
- **HDR10 & HLG:** Skickas orört vid Direct Play och Direct Stream till HDR-kompatibla skärmar.
- **Dolby Vision (DV):** Stöds via Safari / Apple TV och kompatibla Android TV-enheter (Profile 5 och 8).
- **HDR Tonemapping:** På Raspberry Pi 5 är CPU-baserad tonemapping blockerad ([`crates/mbl-playback/src/decide.rs#L355`](file:///home/marcus/mbl/crates/mbl-playback/src/decide.rs#L355)) eftersom mjukvarufiltret sänker frameraten till <5 fps. Direct Play / Direct Stream prioriteras alltid.

### E. Undertexter
- **SRT, WebVTT, mov_text:** Konverteras/extraheras on-demand till WebVTT-sidofiler ([`subtitles.rs`](file:///home/marcus/mbl/crates/mbl-stream/src/subtitles.rs)) och renderas via `<track>` i spelaren. Noll CPU-påverkan.
- **ASS / SSA:** Konverteras till WebVTT och renderas i webbläsaren utan videotranskodning.
- **PGS (Blu-ray) & VobSub (DVD):** Bildbaserade undertexter tvingar idag fram inbränning (`SubtitleDelivery::BurnIn`), vilket triggar full CPU-videotranskodning.

---

## 3. Granskade flaskhalsar (Audit Findings)

### Flaskhals 1: Synkron I/O-loop i `request_segment`
- **Fil & rader:** [`crates/mbl-transcode/src/session.rs#L425-L450`](file:///home/marcus/mbl/crates/mbl-transcode/src/session.rs#L425-L450) samt [`#L1101-L1150`](file:///home/marcus/mbl/crates/mbl-transcode/src/session.rs#L1101-L1150).
- **Problem:** När en klient begär segment `{n}.m4s` körs en `loop` med `sleep(25ms)`. Var 25:e millisekund anropas `refresh_head_segment()`, som kör `std::fs::read_dir()` över sessionskatalogen och för *varje* fil anropar `is_segment_complete()`, som i sin tur läser hela `ffmpeg.m3u8` med synkron `std::fs::read_to_string()`. På Pi 5 med SD-kort eller USB-lagring skapar detta kraftig I/O-belastning och blockerar Tokios asynkrona trådpool.
- **Lösning:**
  1. Håll koll på högsta klara segment i minnet (`AtomicU32` eller sessionsstatus).
  2. När segment $n+1$ har skapats är segment $n$ garanterat stängt och klart – en enda snabb `metadata()`-kontroll räcker.
  3. Läs aldrig `ffmpeg.m3u8` 40 gånger i sekunden för alla filer.

### Flaskhals 2: Hela segmentet läses till RAM
- **Fil & rader:** [`crates/mbl-stream/src/hls/mod.rs#L33-L50`](file:///home/marcus/mbl/crates/mbl-stream/src/hls/mod.rs#L33-L50).
- **Problem:**
  1. `serve_segment_file()` läser hela `.m4s`-filen (ofta 2–8 MB) till en `Vec<u8>` med `tokio::fs::read(path).await`. Vid hög bitrate skapar detta onödiga minnesallokeringar och GC-press.
  2. En spolning eller ett kodarbyte kan ersätta HLS-segment och init-data på samma sessions-URL. När segmentet har lämnat spelarens buffert behöver klienten därför kunna hämta den aktuella filen.
- **Lösning:**
  - Strömma filen via `ReaderStream` med 128 KiB buffert och behåll `Cache-Control: no-store`. Versionssatta segment-URL:er behövs innan servern kan utlova oföränderliga segment.

### Flaskhals 3: 4 KB standardbuffert i Direct Play `ReaderStream`
- **Fil & rader:** [`crates/mbl-stream/src/direct.rs#L154-L182`](file:///home/marcus/mbl/crates/mbl-stream/src/direct.rs#L154-L182).
- **Problem:** `ReaderStream::new(file.take(length))` använder standardbufferten på 4 096 bytes (4 KB). För en 4K-remux på 40 Mbit/s innebär detta över 1 200 systemanrop och context switches per sekund per klient på ARM Linux.
- **Lösning:** Använd `ReaderStream::with_capacity(file.take(length), 128 * 1024)` (128 KB buffert).

### Flaskhals 4: Spurious dödande och omstart av FFmpeg vid återupptagning
- **Fil & rader:** [`crates/mbl-service/src/playback.rs#L1264-L1270`](file:///home/marcus/mbl/crates/mbl-service/src/playback.rs#L1264-L1270).
- **Problem:** Vid återupptagning (`start_ms > 0`) med videokopiering tvingar koden `initial_seg = 0` för att förbereda segment 0 för eventuell Cast-probing. När webbspelaren sedan direkt begär det faktiska segmentet (t.ex. segment 300 vid minut 30), upptäcker `session.rs` att segment 300 ligger utanför head-fönstret, dödar FFmpeg och startar om den på nytt vid minut 30.
- **Lösning:** Starta pipelinen direkt på det begärda segmentet när klienten begär `start_ms > 0` och inte explicit är en äldre Chromecast.

### Flaskhals 5: Webbläsarens kapacitetsdetektering kastas bort
- **Fil & rader:** [`web/src/api/deviceProfile.ts#L116`](file:///home/marcus/mbl/web/src/api/deviceProfile.ts#L116) & [`web/src/components/Player.tsx#L623`](file:///home/marcus/mbl/web/src/components/Player.tsx#L623).
- **Problem:** `detectBrowserCapabilities()` testar om webbläsaren faktiskt klarar HEVC, AV1, AC3 och EAC3 via `MediaSource.isTypeSupported()`. Men `getPreferredProfile()` returnerar bara en hårdkodad sträng (`"chrome"`, `"firefox"` etc.).
  - Chrome utan Microsoft Store HEVC-tillägg får HEVC ändå -> avkodningsfel/svart skärm.
  - Maskiner med hårdvarustöd för AC3/EAC3 får ljudet transkodat i onödan eftersom `chrome.toml` saknar AC3.
- **Lösning:** Skicka med de detekterade egenskaperna via backendens befintliga [`DeviceProfileOverlay`](file:///home/marcus/mbl/crates/mbl-playback/src/profile.rs#L105).

### Flaskhals 6: hls.js saknar prestandaflaggor i webbspelaren
- **Fil & rader:** [`web/src/components/Player.tsx#L684-L690`](file:///home/marcus/mbl/web/src/components/Player.tsx#L684-L690).
- **Problem:** Standardkonfigurationen saknar fragment-prefetching och progressiv fMP4-matning.
- **Lösning:**
  ```typescript
  const hls = new Hls({
    enableWorker: true,
    startFragPrefetch: true,       // Hämtar första segmentet parallellt med spellistan
    progressive: true,             // Progressiv fMP4-parsning under nedladdning
    maxBufferLength: 30,           // 30 s framåtbuffert (optimerat minne)
    maxMaxBufferLength: 60,
    backBufferLength: 30,          // 30 s bakåtbuffert för omedelbar spolning bakåt
    capLevelToPlayerSize: true,    // Begränsar ABR till skärmens faktiska storlek
    testBandwidth: false,          // Skippar onödig bandbreddstestning
    startPosition: startMs > 0 ? startMs / 1000 : -1,
    manifestLoadPolicy: hlsLoadPolicy(),
    playlistLoadPolicy: hlsLoadPolicy(),
    fragLoadPolicy: hlsLoadPolicy(),
  });
  ```

### Flaskhals 7: Android-klientens AV1-stöd behöver detekteras
- **Fil & rader:** [`crates/mbl-playback/profiles/android-exoplayer.toml#L10`](file:///home/marcus/mbl/crates/mbl-playback/profiles/android-exoplayer.toml#L10).
- **Åtgärd:** Låt Android-klienten lägga till `av1` i både `directPlay` och `codecLimits` i sin `DeviceProfileOverlay` efter enhetsdetektering. Android erbjuder avkodning från Android 10 men kräver den först från Android 14. [Androids formatdokumentation](https://developer.android.com/media/platform/supported-formats#video-formats)

---

## 4. Implementationsplan för nästa agent

### Fas 1: Snabbstart och buffring

- [x] Aktivera förhämtning och progressiv fragmentladdning i webbspelaren. Sätt buffertmålet till 30 s framåt, taket till 60 s och bakåtbufferten till 30 s. Behåll spelarens bandbreddstest vid start.
- [x] Strömma hela Direct Play-svar, byteintervall och HLS-filer med en buffert på 128 KiB. Behåll `no-store` för HLS-filer som kan ersättas på samma URL.
- [x] Verifiera att servern väljer Direct Play för AV1 när en Android-klient annonserar stöd. Enhetsdetekteringen tillhör Android-klienten.
- [ ] Mät starttid och minnesanvändning vid uppspelning på Pi 5.

### Fas 2: Minska diskavläsningarna vid segmentväntan

- [x] Håll färdiga segment och FFmpegs position i minnet per bildvariant. Läs den växande EVENT-spellistan asynkront när storleken ändras. Använd publicering i spellistan som bekräftelse även för sista segmentet och kontrollera att den begärda filen finns kvar.
- [x] Återställ spellistans status vid varje omstart. Behåll tidigare bekräftade fragment före ett vanligt spolmål. Vid kodarbyte behövs fragment som hör till den nya kodarens init-data.
- [x] Använd samma spårning för Cast-förberedelsen och fortsätt leverera FFmpegs faktiska spellista med dess tidsstämplar.

### Fas 3: Dynamisk `DeviceProfileOverlay` från klienten

- [x] Detektera HEVC, AV1, AC-3 och E-AC-3 separat för direkt MP4, direkt WebM och fMP4 via HLS. Använd MSE-prober för hls.js och filprober när webbläsaren väljer inbyggd HLS.
- [x] Skicka samma `browserCodecs` i förhandsbesked, lokal start, kvalitets- och spårbyten. Cast väljer mottagarens egen profil.
- [x] Verifiera med API-test att rapporterat AC-3-stöd ger Direct Play och att en profil utan HEVC-stöd väljer H.264 i testfallet.
- [ ] Verifiera faktisk uppspelning på en Chrome-installation med AC-3-stöd.

### Fas 4: Smart resume (`start_ms > 0`) vid videokopiering — Estimat: 2 timmar
1. I [`crates/mbl-service/src/playback.rs`](file:///home/marcus/mbl/crates/mbl-service/src/playback.rs#L1264):
   - Tillåt `initial_seg = segment_index_for_ms(start_ms)` även när `is_video_copy == true`, förutsatt att klienten inte begärt Cast-kompatibilitet.
2. **Verifiering:** Mät uppstartstid vid spolning/återupptagning (ska gå från ~3 sekunder till <500 ms).
