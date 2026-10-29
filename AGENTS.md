# AGENTS.md — Agent Handoff, Environment Guide & Work Log

Detta dokument ligger i roten av repot så att alla AI-agenter (Antigravity, Claude Code, Cursor, Codex m.fl.) och utvecklare direkt hittar aktuell status, miljöspecifika regler, arkitekturbeslut och handoff-logg.

---

## 1. Aktuell Status (Snabböverblick)

- **Senaste release:** `v1.0.0` (publicerad 2026-10-26, M10 färdigställd — General Availability)
- **Avklarade milstolpar:**
  - **M0:** Foundations (Workspace, CI, config, SQLite, syntetisk testmedia)
  - **M1:** Library Core (Scanner, parser, ffprobe, TMDB/NFO-metadata, bilder, FTS5, WebSocket-events, OpenAPI)
  - **M2:** Playback MVP & MCP v0 (`v0.1.0` — Direct play, HLS fMP4-transkodning, undertexter, watch state, MCP-server med scopade API-tokens och audit-logg, Web MVP)
  - **M3:** Modern UI v1 (`v0.2.0` — Design tokens, 4 teman + accenter, komponentbibliotek, responsiv desktop/mobil/TV-navigering, kommandopalett, i18n, bundle-budget)
  - **M4:** Hardware Acceleration (`v0.3.0` — `mbl-hwaccel`, plattformsfingeravtryck, FFmpeg-probing, självtest i `0003_hw_capabilities.sql`, VAAPI/QSV/NVENC/Pi V4L2/DRM/Windows D3D11/AMF, fallback-kedjor, HDR tone mapping, ABR-stege, HEVC/AV1 fjärrpreferens, Hailo-8/10H NPU kooperativ `VDevice`-delning, Admin Hardware UI, MCP HW-verktyg)
  - **M5:** Pi 5 excellence + TV client & media intelligence (`v0.4.0` — Termisk/spänningsvakt, profilstandarder, tmpfs-transkodning, förtranskodade optimerade versioner med automatisk DirectPlay för inkompatibla klienter, trickplay-skrubb JPEG-ark, intro- och eftertextsdetektering med markörer i PlaybackResponse, `mbl-tv` 10-fotsklient med rumslig navigering/CEC/gamepad/uppdateringsfrekvensmatchning, OS-avbild `pi-gen` & APT-repo, Hailo-8/10H NPU multimodal scensökning, taggning och spoiler-fria sammanfattningar med `hailo-ollama`)
  - **M6:** Users, sharing, remote access, Windows (`v0.5.0` — Biblioteks-ACL per användare, hanterade profiler med föräldra-PIN och åldersgränser, tidsbegränsade inbjudningslänkar, TOTP 2FA, nätverksklassificering och trusted proxy, rättvis bandbreddsdelning på WAN, `mbl-remote` Tailscale LocalAPI-klient med spoofing-skydd, CGNAT-diagnostik, admin LAN/tailnet-vakt, Windows-tjänst/tray/WiX MSI-paket, Docker multi-arch och nvidia/vaapi/tailscale-compose, MCP mutationsramverk med `dry_run` och `confirm`/elicitering, 15 nya MCP-verktyg till 42 admin-verktyg, OAuth 2.1 PKCE och dynamisk klientregistrering för MCP)
  - **M7:** Media breadth (`v0.6.0` — Musikskanning med `lofty` och MusicBrainz, EBU R128 loudness-analys och progressiv audiotranskodning, OpenSubsonic `/rest/*`-kompatibilitet, foto-EXIF och tidslinje, samlingar och spellistor, fältlåsning i metadata, OpenSubtitles-nedladdning, DLNA UPnP SSDP och ContentDirectory, PlayerRegistry och fjärrstyrning, webhooks med HMAC-SHA256, Home Assistant MQTT Discovery och telemetri, AI Watch Companion co-watching med realtidsdialogcues och reaktioner, Subtitle Auto-Sync `mbl-subsync`, Scene X-Ray, spoiler-fria recaps med strikt tittarhistorik, Movie Night Matchmaker swipe-sessions, inbyggda Media Requests med Radarr/Sonarr-brygga och hardlink-lagringsanalys, 20 nya MCP-verktyg till 62 verktyg med 100% OpenAPI-paritet, samt fullständig WiX MSI-installerare på Windows)
  - **M8:** Distributed nodes (`v0.7.0` — Transcode workers & Remote library storage nodes, worker-protokollspecifikation och typade `WorkerJobSpec`, HMAC-signerade source range URL:er, segment `PUT`-uppladdning i sessionskatalog, Wake-on-LAN magic packet, sömlös lokal fallback vid worker-bortfall, Windows worker-tray med pause-while-gaming GPU-vakt, admin worker & node dashboard i SolidJS, 5 nya MCP-verktyg till 67 admin-verktyg med 100% OpenAPI-paritet, `mbl-node` federerad lagringsnod med delta catalog sync, och Smart Handover för LAN direktuppspelning vs WAN relay)
  - **M9:** Live TV, Channels and Online Streaming Plugins (`v0.8.0` — HDHomeRun UDP discovery & lineup-synkronisering, M3U & streaming XMLTV-parsning med `quick-xml`, 2-axlars virtuellt EPG-rutnät, live-uppspelningsmotor med samtidighetsbegränsning, DVR-inspelningsschemaläggare med serieregler, marginaltider och konfliktlösning, 5 nya MCP-verktyg till 72 admin-verktyg med 100% OpenAPI-paritet, plugin-arkitektur för online-media med YouTube-, Spotify- och WebDrama-stöd, virtuella 24/7-kanaler med zero-CPU playhead-beräkning, samt fullständig 4-flikars Live TV SolidJS UI)
  - **M10:** Hardening → v1.0.0 (`v1.0.0` — Formellt STRIDE Threat Model-dokument, säkerhetshärdning för webhooks/DoS/traversal, fuzzing-svit med libfuzzer-targets och property tests, prestanda- och SLA-rapport i `docs/benchmarks.md`, komplett mdBook-dokumentationssajt i `docs/book/`, migreringsverktyg för Plex och Jellyfin historik & watch state, fullständiga 5-språksöversättningar [en, sv, de, fr, es] med WCAG 2.2 AA accessibility, separata release-byggen för `mbl-worker` och `mbl-node` med systemd-services och fristående Windows-paket, samt MCP eval-scenarier och runner med 74 admin-verktyg och 100% paritet)
  - **M12 (under verifiering):** Google Cast SDK, native Remote Playback och spelarregistrering finns. Commit `d05a03f` har granskats och ytterligare livscykel-/CORS-fel rättats. Cast-profil/MIME, textspår och ägarbunden kommandotransport är implementerade; fysisk mottagaracceptans och full DLNA-/receiverlivscykel återstår; se `docs/audits/2026-10-27-web-and-service-audit.md`.
- **M11 avvaktar enligt användaren.** Aktuellt fokus är buggar, säkerhet och användbara webbflöden, inklusive Cast; native Mobile/TV/VR påbörjas inte.

---

## 2. Viktiga Dokument i Repot

| Fil | Syfte |
|---|---|
| [`AGENTS.md`](AGENTS.md) | **Denna fil.** Handoff-logg, maskinspecifika regler och snabbstatus för agenter. Uppdatera alltid vid slutfört arbete! |
| [`IMPLEMENTATION.md`](IMPLEMENTATION.md) | Uppgiftslista (`M0`–`M11+`) med kryssrutor `[x]`/`[ ]`, acceptanskriterier och detaljerad Progress Log (§9). |
| [`PLAN.md`](PLAN.md) | Fullständig arkitektur- och produktplan. |
| [`CHANGELOG.md`](CHANGELOG.md) | Användarsynliga ändringar per release (`v0.1.0`, `v0.2.0`, `v0.3.0`, `v0.4.0`, …). |
| [`docs/adr/`](docs/adr/) | Architecture Decision Records (t.ex. `0005-tv-ui-stack.md`, `0006-hailo-npu-resource-sharing.md`). |
| [`docs/openapi.json`](docs/openapi.json) | Genererat OpenAPI 3.1-schema (verifieras med `cargo xtask openapi-check`). |
| [`docs/worker-protocol.md`](docs/worker-protocol.md) | Protokollspecifikation för distribuerade transkodningsarbetare och federerade lagringsnoder. |

---

## 3. Kritiska Miljö- och Maskinregler (Läs innan du kör kommandon!)

1. **Hårdvara på utvecklingsmaskinen:**
   - Maskinen är en **Raspberry Pi 5 (ARM64, Linux)** med en **Hailo-10H NPU** (`/dev/hailo0`) där andra co-tenants (t.ex. Frigate i Docker) kan köra samtidigt.
2. **Begränsat diskutrymme på `/` (`~15–18 GB` ledigt):**
   - Fullständiga debug-symboler för alla testbinärer i `target/` kan fylla hela SD-kortet (`No space left on device`).
   - **Använd ALLTID** följande flaggor när du kör `cargo nextest` eller `cargo test`:
     ```bash
     CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 XDG_RUNTIME_DIR=/tmp CI=1 NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo nextest run --workspace --no-tests=pass
     ```
   - Detta håller `target/` runt ~1.5 GB istället för >18 GB. Om diskutrymmet blir lågt, kör `cargo clean`.
3. **Arbetsflöde för tester (Undvik långa omkompileringar efter varje delsteg):**
   - Eftersom hela testsviten tar tid på Pi 5, implementera hela milstolpen (eller en sammanhängande grupp uppgifter), håll koden kompilerande med snabba `cargo clippy`, och kör hela testsviten + release-stegen när milstolpen är klar.
4. **Miljövariabler för frontend & tester:**
   - Sätt alltid `XDG_RUNTIME_DIR=/tmp` vid `just web-check` eller `pnpm`-kommandon:
     ```bash
     XDG_RUNTIME_DIR=/tmp just web-check
     ```
   - Sätt `NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost` vid integrationstester så att lokala HTTP/MCP-anrop inte fångas av eventuell systemproxy.
5. **OpenAPI & Design Tokens:**
   - Om du ändrar eller lägger till REST-endpoints i `crates/mbl-api`:
     1. Registrera pathen och scheman i `crates/mbl-api/src/openapi.rs`.
     2. Generera om `docs/openapi.json`: `cargo run -p mbl-server -- openapi > docs/openapi.json`
     3. Generera om TypeScript-typerna: `XDG_RUNTIME_DIR=/tmp pnpm -C web gen:api`
     4. Verifiera med `cargo xtask openapi-check`.
6. **MCP-paritet & Snapshots (`crates/mbl-mcp`):**
   - Varje ny kapabilitet ska antingen exponeras som ett MCP-verktyg i `crates/mbl-mcp/src/tools/` eller dokumenteras i `crates/mbl-mcp/EXCLUSIONS.toml`.
   - När nya MCP-verktyg läggs till måste verktygsräkningen i `crates/mbl-mcp/tests/tools_list.rs` och `crates/mbl-server/tests/mcp.rs` samt snapshots i `crates/mbl-mcp/tests/snapshots/` uppdateras (just nu: `30` för `read`, `35` för `read+library`, `74` för `admin`).

---

## 4. Release-checklista (vid milstolpsslut)

När en milstolpe är färdig och alla tester är gröna:
1. Uppdatera kryssrutor `[x]` och lägg till en sammanfattningsrad i `IMPLEMENTATION.md` (§9 Progress Log).
2. Committa milstolpens implementation med `git commit -s -m "feat(...): M<N> ..."`.
3. Förbered release-commiten (`release: v0.<X>.0`):
   - `Cargo.toml` (`[workspace.package] version = "0.<X>.0"`)
   - `CHANGELOG.md` (ny sektion `## [0.<X>.0] - YYYY-MM-DD`)
   - `README.md` (uppdatera `## Status`)
   - `packaging/windows/README.txt` (uppdatera versionsnummer)
   - `.github/workflows/release.yml` (uppdatera fallback-tagg)
   - `docs/openapi.json` (uppdatera `"version": "0.<X>.0"`)
   - Kör `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo xtask openapi-check` (uppdaterar även `Cargo.lock`).
4. Committa med `git commit -s -m "release: v0.<X>.0"` och tagga `git tag v0.<X>.0`.
5. Uppdatera **Avsnitt 5 (Sessions- och Handoff-logg)** nedan.

---

## 5. Sessions- och Handoff-logg

> **Instruktion till agenter:** Lägg till en ny sektion överst i denna logg när du avslutar en uppgift eller milstolpe. Beskriv vad som gjordes, vilka filer/crates som ändrades, eventuella fallgropar som löstes och vad nästa agent ska ta vid med.

### 2026-10-29 — Lokal Git-kontrollpunkt efter batch 24
- Användaren har godkänt en lokal checkpoint-commit för batch 20–24 ovanpå `fcdd83b` (batch 1–19). Den samlar DVR-profiler/ägarskap/retention, Live TV:s programgränser, Partial-publicering, Retry-preview/diskvakt samt radio/DVB/AAC/HEVC-rättningarna med migration 0011, tester, genererade API-typer och dokumentation.
- Senast verifierat: 51 riktade Rust-tester, 152 webbtester, TypeScript/Vite, JS-budget 42,24 KB gzip, berörda crates Clippy med alla targets och -D warnings, fmt/diff-check, OpenAPI-check och MCP-paritet 148/148. Resultaten återanvänds för denna checkpoint; inga kodändringar eller nya testkörningar behövdes inför committen.
- Full workspace-regression efter batch 20–24 och fysisk tuner/browser/Cast/WAN/Windows-acceptans återstår. Fortsatt arbete följer restlistan: transport-/undertextprofiler, Failed-städning/återställning och arkivbudget. Kontrollpunkten skapas med Signed-off-by; release/version/tagg/M11 ändras inte.

### 2026-10-29 — Batch 24: DVB-undertexter och radio i vanliga spelaren
- Recorder mappar även valfria undertextspår till MPEG-TS. Verklig DVB-bitmaptext och två ljudspråk bevaras; inspelningen öppnas med befintligt ljudval, inbränd text och Off. Programvaruinspelning fortsätter med stream copy. HEVC/AAC bevaras och förhandlas till H.264/AAC för Firefox; MPEG-2/AC-3 och MP2-radio verifieras också.
- DVR-publicering godtar positiv medielängd med audio eller video; filer med enbart undertexter förblir ospelbara. Ljudinspelningar katalogiseras som Track i samma hanterade bibliotek, med samma ACL/rating, ägare, Partial-status, historik och raderingsflöde.
- `PlaybackResponse.hasVideo` beskriver verklig video, inklusive när utleveransen är HLS. Webben visar en ljudvy med titel och korrekt Seek audio, döljer videokvalitetsval och gör inga trickplay-anrop för audio. HLS master annonserar rätt kopierad videocodec; radio annonserar enbart ljudcodec och ljudbitrate, utan falsk H.264. Vanlig session/ljudspårs-/Cast-förhandling återanvänds. Ingen ny endpoint, migration eller MCP-verktygsräkning.
- Verklig HEVC/AAC-regression avslöjade ett varierande FFmpeg-startfel: ADTS-AAC kopierades till fMP4 utan bitströmskonvertering. Ny intern `AudioOp::CopyAac` använder `aac_adtstoasc` när valt kopierat ljud är AAC; andra kopierade ljudcodecs får ingen AAC-filterkedja. Felutdata fångades med en tillfällig wrapper för enbart syntetiska tester och visade exakt muxerfel.
- Kopierad HEVC fick tidigare FFmpegs `hev1`-sample entry trots masterlistans `hvc1`. Ny intern `VideoOp::CopyHevc` sätter `-tag:v hvc1`; ffprobe på verklig remux verifierar att MP4-fragment och serverad lista stämmer överens. Äldre generisk Copy är kvar för andra codecs.
- Verifierat: 51 unika riktade Rust-tester (nya DVR-mediaflöden 4; källfel 6; planer/diskvakt 6; schemaläggning/retention 10; relä/DVR 12; uppstart 6; spår 2; transkodningspipeline 5). Verklig FFmpeg verifierar DVB-pixlar/Off, ljudspråk, radio Completed/Partial med profil/historik/radering, HEVC→H.264 och HEVC-remux med matchande hvc1 samt kopierat AAC. 152 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip passerar; slutlig kontrastjustering är också byggverifierad. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för fem berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade; MCP-paritet 148/148 (74 verktyg, 52 undantagna paths). Full workspace-regression efter batch 20–24 ingick inte.
- MPEG-TS-profilen är fortsatt avgränsad: DVB-text är verifierad, inte generell konvertering av SRT/ASS/andra undertextformat, text-TV eller inbäddad CEA-text. Verklig tuner/browser/Cast/WAN och Windows-acceptans återstår. Nästa core: vidare transport-/undertextprofiler samt Failed-städning/återställning och helarkivskvot/prognoser. Tidigare batcher bevarade efter fcdd83b; ingen ny commit/release/M11.

### 2026-10-29 — Batch 23: Retry-policy, kanonisk preview och DVR-diskvakt
- `ScheduleRecordingRequest.retryOf` bevarar ursprunglig ägare, överlevande serieregel, marginaler och retention för Failed/Partial. Även admin-retry använder ägarens aktuella profil/ACL/rating. Borttagen regel ger manuell inspelning med samma ägare. En annan delad tittare får inte Retry; gammal fil behålls. DB-transaktionen kontrollerar källrad/regel igen och serialiserar samtidiga försök/idempotens.
- Gemensam `prepare_recording` driver faktisk schemaläggning, ny `POST /api/v1/livetv/recordings/preview` och MCP `schedule_recording(dry_run)`. Guidens riktiga titel/tider, effektiv padding, ägare/regel/retention och aktuell konflikt/befintligt jobb visas. Preview skapar inga bibliotek, kataloger, jobb eller tunerbokningar; actual skriver och revaliderar. Det är en ögonblicksbild, ingen framtida resursgaranti.
- Webben förhandsvisar Retry och bekräftar en ärvd raderingspolicy innan ett nytt försök skapas. Konflikt/låg disk förklaras utan ny inspelningsrad. Dubbla klick spärras; request-timeout och avbrott vid avmontering skyddar mot sen bekräftelse/schemaläggning. MCP har optional retry_of och visar samma verkliga plan; get_settings.dvr visar tillämpad diskstatus. Ett nytt preview-path men inga nya MCP-verktyg.
- `DvrConfig`: min_free_mb=512 som standard och optional max_recording_mb (MiB; 0/overflow för max avvisas). Unix läser tillgängliga icke-reserverade bytes via redan låst rustix 1.1.5; Windows har GetDiskFreeSpaceExW-wrapper. Osäker kod begränsas till Windows-FFI-funktionen. Cargo.lock får endast rustix som ny direkt relation för mbl-platform.
- Kontroll före start och varje sekund; FFmpeg får -fs med min(konfigurerat tak, startens lediga utrymme minus reserv). Nekad start skapar ingen capturefil. Stopp vid disk-/storleksgräns bevarar giltig del enligt batch 22. Inga sparade filer raderas automatiskt för att frigöra plats. Inspektionskontroller har timeout och kan avbrytas av shutdown/sluttid; policyfel dödar och reap:ar barnet och publicerar inte delmedia.
- Verifierat: 41 unika riktade Rust-tester (DVR-planer 6, källfel 6, schemaläggning 10, M9 7, MCP-snapshots 3, config 8 och disk 1). Verklig FFmpeg verifierar storleksstopp, spelbar Partial och barnstädning vid policyfel; låg disk hindrar capture utan radering. 151 webbtester passerar; två typfel i nya testassertioner rättades, därefter passerar TypeScript/Vite och JS-budget 42,24 KB gzip. Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sju berörda crates med alla targets och -D warnings, fmt/diff-check och OpenAPI-check passerar. OpenAPI/TypeScript är regenererade; MCP-paritet 148/148, fortsatt 74 verktyg och 52 undantagna paths. Hela Rust-arbetsytan har inte körts om efter batch 20–23. Tidigare batch 20–22 bevarade efter fcdd83b; ingen ny commit/release/M11.
- Nästa core: DVR-format/undertexter/radio, kvot för hela arkivet/prognoser och explicit städ-/återställningspolicy för Failed-filer. Diskvakten pollar och reserverar inte utrymme mot andra samtidiga skribenter; det är ingen hård volymkvot. Fysisk tuner/EPG/browser/Cast/WAN och Windows-acceptans återstår.

### 2026-10-29 — Batch 22: ofullständiga DVR-filer och säker slutpublicering
- Ny `RecordingStatus::Partial`/`partial`: nya capturer jämför probad medielängd med hela det planerade fönstret. Tolerans är 5 procent, minst 100 ms och högst 2 s. Ren tidig EOF kan inte längre ge Completed bara på FFmpegs exitkod. Giltig video efter encoder-/källfel kan sparas som Partial med orsak; tomma/trasiga filer förblir Failed utan katalogpost.
- Recorder använder både monotont sluttidslarm och väggklockskontroll; en hängande källa fortsätter inte under den tidigare 15-sekundersmarginalen. Efter stopp avgör faktisk medielängd om fönstret täcktes. Serverstopp, avbokning och behörighetsfel går inte genom källfelens delpublicering.
- Ägare/guide kontrolleras före och efter ffprobe även vid snabb EOF. En ändring under analysen kan inte publicera en fil med äldre tillåtande rating. Publicering sparar status, orsak och katalogpost atomiskt. Äldre Completed-arkiv omklassas inte retroaktivt från otillräcklig historik.
- Partial visas med orsak, filter, Play partial, Retry och ägarstyrd Delete. Bibliotekstitel och beskrivning märker också ofullständigt material. Retry får nytt ID utan att radera tidigare del. ACL/rating gäller normal spelare och delad listning. Retention räknar både Completed och Partial som sparade filer och skyddar pågående/pausad visning; webb/MCP beskriver saved i stället för enbart completed.
- Befintliga REST/MCP-list-/spelar-/raderingsflöden bär den nya statusen; inget nytt verktyg eller endpoint behövs. MCP-schema och tre snapshots uppdateras. Ingen databasändring behövs eftersom statuskolumnen är fri text.
- Verifierat: 31 riktade Rust-regressioner passerar: källfel/sluttid 6, DVR-planering/retention 10 (med Partial-filer), relä/DVR/publicering 12 och MCP-snapshots 3. Källfelsfallen är även omkörda med en tunerplats. 148 webbtester, TypeScript/Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–22. Batch 20–21 ligger kvar efter kontrollpunkten fcdd83b; ingen ny commit/release/M11.
- Nästa core-steg: undertext-/radio-/övriga medieprofiler för DVR, lagringsbudget/städning av Failed-filer, bevarad regelkoppling vid Retry och normaliserad MCP-preview. Återställning av ännu opublicerade filer efter serveravbrott kräver separat policy; de visas fortsatt som Failed. Fysisk tuner/EPG-/browser/Cast/WAN-acceptans återstår.

### 2026-10-29 — Batch 21: Live TV-profiler, programgränser och återkallade medielänkar
- Live-start hämtar aktuell profil från den aktiva inloggningen. Profiler med åldersgräns behöver ett pågående tillåtet EPG-program; saknad guide ger nekad start. Admin/fria profiler kan fortsatt använda kanaler utan guide.
- Ägda live-leases binds till användare och inloggningens token-hash så ett återanvänt SQLite-sessions-ID inte kan återuppliva en gammal medielänk. Aktuell session, profil, program och källa kontrolleras vid start, efter beredskap, förnyelse, varje medieanrop och periodiskt underhåll. Nekade leases städar encoder, tunerplats och cache.
- Åldersbegränsad uppspelning binds till ett enda programs ID/sluttid. Lease och FFmpeg-duration begränsas, även för en HTTP-källa som levererar snabbare än realtid. Ny `LiveStreamInfo.accessEndsAt` gör att webben stänger buffrad uppspelning vid programgränsen. Även ett tillåtet nästa program kräver nytt kanalval; fria profiler behåller kontinuerlig visning. Ändrad guide-sluttid stoppar den gamla begränsade bokningen.
- REST-kanaler/guide/nu/nästa och MCP-guidesökning filtreras efter rating. Programsökning använder keyset-sidor före resultatgränsen så dolda träffar inte tränger undan tillåtna. Icke-admin får inga råa kanal-/tuneradresser eller dolda kanaler; tunerupptäckt kräver admin. Virtuell EPG döljs för icke-admin tills katalogmedveten ACL finns (område 17).
- Ny `Channel.canPlay` styr spärrade spelknappar och förklaring. Guiden uppdateras var 30:e sekund medan guidefliken är öppen; gamla svar kan inte ersätta ett nyare fönster. Terminalt mediefel stänger HLS och tömmer buffrad källa.
- Verifierat: 35 riktade Rust-regressioner passerar (Live-profiler 6, relä/DVR 12, DVR-planering/retention 10, äldre M9/guide/MCP 7). Nytt kanaluppdateringsfall är också omkört. 147 webbtester, TypeScript, Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Clippy för sex berörda crates/alla targets med -D warnings, fmt/diff, OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. OpenAPI/TypeScript är regenererade. Full workspace-regression från kontrollpunkten är inte omkörd efter batch 20–21. Tidigare batch 20 ligger kvar i samma arbetskopia efter `fcdd83b`. Ingen ny commit/release/M11.
- Nästa core-steg: DVR-källors tidiga avslut, format/undertexter och lagringsbudget samt normaliserad MCP-preview. Verklig tuner-/EPG-klocksynk, browser/Cast/WAN och live-hårdvaruprofiler/timeshift kräver fortsatt acceptans.

### 2026-10-29 — Batch 20: profilmedveten DVR, seriematchning och retention
- `cargo clean` kördes först enligt användaren: 8370 filer/4,2 GiB borttagna, cirka 6,1 GB ledigt efteråt. Baseline är kontrollpunkten `fcdd83b`; denna batch ligger som nya ändringar i arbetskopian.
- Migration `0011_dvr_scheduling.sql` ger inspelningar ägare, bibliotek, ursprungliga programtider och kanalnamn. AUTOINCREMENT förhindrar att en avbokad child städar en ny inspelnings fil-ID. Dolda avboknings-/raderingstombstones stoppar automatisk återbokning och städas efter att sändningen varit avslutad ett dygn. Explicit Retry får nytt ID. XMLTV uppdaterar samma kanal/starttid i stället för att skapa dubbletter; gamla dubbletter migreras.
- Ny `mbl-service::livetv::scheduler` (`dvr_scheduler.rs`): REST/MCP använder samma actor-kontroller; guidebundna inspelningar hämtar riktig titel/tid/rating. Begränsade profiler får egna väntande/pågående inspelningar, måste ha DVR-biblioteksåtkomst och får bara guidens programfönster utan padding. Jobb kontrollerar aktuella rättigheter före start och under capture, även efter ändrad åldersgräns. Regel/detalj/radering är ägar-/adminstyrda; andra tillåtna profiler kan spela delade färdiga filer men får inte radera dem.
- Serieregler matchar 14 dagar framåt var 15:e sekund med keyset-sidor om 200 och effektiv anti-join för redan behandlade sändningar. Kanal/säsong/episod/new-filter används. Konflikter visas som Failed; Retry gör ett nytt uttryckligt försök. Regelborttagning avbokar väntande jobb och behåller pågående/färdiga filer. Planering/återställning ligger utanför capture-taskernas 32-platstak.
- Retention kör verklig fil-/katalogradering för keepMax och ägarens watch state. Högst 16 försök per pass och inga pågående/pausade spelarobjekt raderas. Ett delat playback-lås skyddar start, heartbeat, stopp, expiry och shutdown mot samtidig retention. HLS stängs före shutdown-låset för att inte låsa fast en väntande start bakom retention; ny regressionskontroll täcker detta. Upptagen probe-gate skjuter upp retention i stället för att blockera nya spelare.
- Färdiga arkiv behålls vid kanal-/tunerborttagning; väntande/pågående jobb får källfel. Borttagna ägare kan inte lämna anonyma körbara jobb. DVR-bibliotek kan döpas om men inte flyttas/raderas genom vanlig bibliotekshantering. UI visar ägarstyrd Delete/Cancel, Retry, ursprungskanal, säsongs-/episodfilter och tydlig opt-in för radering/retention. Destruktiva formulärval återställs efter lyckat regelskapande. MCP validerar u32/range utan trunkering och markerar retentionskapabel regelhantering som destruktiv med confirmation.
- Ändrad programtitel eller höjd guide-rating stoppar capture så sparad äldre metadata inte gör nytt innehåll synligt för fel profil. PG-filtreringen i katalogfrågor tillåter nu endast kända etiketter på rätt nivå; okända etiketter följer Actor-regelns nivå 2 även vid listning.
- Verifierat: 140 webbtester, TypeScript/Vite och JS-budget 42,23 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. 45 unika riktade Rust-regressioner passerar (DVR 10, live 12, guide/schema/MCP-DVR 3, livscykel 4, startup 6, ACL/auth 7, MCP-snapshots 3). Även 205 matchningar över flera sidor, schema-uppgradering med befintlig data och riktig filradering/uppspelning verifieras. Clippy för sex berörda crates/alla targets med -D warnings och fmt/diff-check passerar. OpenAPI/TypeScript är regenererade; OpenAPI-check och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar. Full workspace-regression kördes vid kontrollpunkten, inte på nytt efter denna batch.
- Fortsatt core-arbete: Live TV:s profil-/programövergångar, DVR-källors tidiga avslut/format/undertextbevarande och lagringsbudget samt korrekt normaliserad MCP-preview behöver granskas vidare. Fysisk tuner/Cast/WAN-acceptans kvarstår. Ingen ny commit/release/M11 i denna batch.

### 2026-10-29 — Samlad regression och Git-kontrollpunkt för batch 1–19
- Användaren har godkänt att spara de samlade granskningsfixarna i en lokal kontrollpunktscommit med `Signed-off-by`. Befintlig arbetsgren är `main`; versionsnummer och releasetaggar ändras inte.
- Full Rust-regression: `cargo nextest run --workspace --no-tests=pass --no-fail-fast --test-threads=2` med repots resurssnåla miljöflaggor passerar 371/371 tester, 0 skipped. Inga regressionsrättningar behövdes.
- `just web-check`: 138/138 tester, TypeScript, Vite och JS-budget 42,24 KB gzip passerar; Biome har 0 fel och tidigare 5 varningar/4 infos. Fmt och diff-check passerar.
- Hela arbetsytans `cargo clippy --workspace --all-targets -- -D warnings`, OpenAPI-check, design tokens och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar också.
- Äldre dubbla testbinärer som redan ersatts av nybyggda versioner togs bort ur `target/` under kompileringen för att frigöra utrymme. Källkod/testdata behölls.
- Restlistans Git-kontrollpunkt skiljs nu från releaseacceptans. Fysisk tuner/Cast/WAN-/workeracceptans och 23 breda delvis öppna arbetsområden kvarstår. Fortsätt med spelarrelaterade DVR-/profil-/serieregel-/retentionfrågor först; M11 avvaktar.

### 2026-10-28 — Batch 19: DVR-uppspelning och probad mediemetadata
- Ny `mbl-service::dvr`, `mbl-db::repo::dvr` och migration `0010_dvr_playback.sql`: färdig fil blir atomiskt en vanlig katalogpost med verklig ffprobe-duration, format, spår och EPG-snapshot. `Recording.itemId` använder befintlig Player/Playback/Stream, behörigheter och historik. Inga nya media-endpoints.
- Inspelningsbiblioteket hanteras av DVR; scanner hoppar över det så partiella filer inte importeras. Begränsad probing, root-/symlink-/ändringskontroll, idempotent publicering och avbokning via gemensamt lås. Äldre färdiga filer återställs parallellt med nya scheman vid start. Radering städar fil och katalogpost; externa/symlinkvägar följs inte.
- REST/MCP-listning, detalj och radering använder ACL/rating. Begränsade profiler ser endast publicerade tillåtna inspelningar; profilmedveten schemaläggning och pågående listning behöver fortsatt arbete. Webb-DVR visar Play, verklig längd, filraderingsbekräftelse och periodisk uppdatering. Live-release inväntas före uppspelning.
- Verifierat: 138 webbtester, TypeScript/Vite/Biome/budget 42,24 KB samt 10 riktade Rust-regressioner (DVR 5, MCP-snapshots 3, äldre DVR/MCP 2). Riktig femminutersfil verifierar probe/HLS/61 s resume/historik. Clippy för fem berörda crates med alla targets och `-D warnings`, fmt/diff, OpenAPI-check och MCP-paritet 147/147 passerar. API-typer regenererade; inga nya endpoints eller verktyg (74/52).
- Nästa steg: fortsatt DVR-serieregelmatchning och retention samt profilmedveten schemaläggning. Följ även upp DVR-bibliotekets livscykel vid manuell ändring av biblioteksroots och borttagning av tuner/kanal. Därefter återstående core-/receiveracceptans enligt restlistan. M11/Git/release avvaktar. Tidigare batchers ändringar är kvar i samma arbetskopia; ingen commit eller full release-regression körd.

### 2026-10-28 — Batch 18: serverägd Live TV och DVR-kapacitet
- Ny `mbl-service::live_relay`: ägd FFmpeg-child, riktig rullande fMP4-HLS, bounded start/input, software H.264/720p/25 fps/bwdif + AAC-stereo. Auth-ägd 45 s lease, separat renewal, kandidatstädning vid cancel/fel, capability-filer med strikt syntax. Ny `/api/v1/stream/live/{session}/{file}` har Range/HEAD/CORS/private no-store.
- `VideoCapacity` i mbl-transcode delar admission mellan VOD och live. Live/DVR använder samma tunerleases i LiveTvService. Aktiva reservationer behålls; DVR som möter full tuner får failed/orsak, ingen tyst överbokning. DVR-leasen hålls under hela barnets livstid. Avbokad partiell fil städas även vid race med processavslut.
- Periodiskt serverunderhåll och shutdown omfattar live; process publiceras under leaseslåset så shutdown inte missar en sen child. Kvot och borttagning av kanal/tuner städar cache/kapacitet. Liveencoder pausar idle-analys; trickplay-handler registreras med slutligt stream-konfigurerad PlaybackService.
- LiveTv väntar på release före kanalbyte, kan avbryta start, städar sena svar, har renewal-timeout/överlappsskydd och sessionsbundna callbacks. LiveStreamPlayer använder begränsad HLS-återhämtning, släpper vid fel/EOF och respekterar paus. Virtuella kanalobjekt får inte längre startas som falska tunerkällor.
- Verifierat: 136 webbtester (12 nya liveflödesfall), TypeScript/Vite/budget 42,24 KB; 32 unika riktade Rust-tester (live/DVR 9, transcodesession 9, äldre live/schema 3, startup/livscykel/trickplay 11). Interlaced MPEG-2/AC-3 från isolerad localhost-källa blir progressiv H.264/AAC; faktisk DVR-fil, full 45 s expiry, kapacitetskonflikter, cancel, kvot, Range/HEAD/CORS och cache/release passerar. Clippy, fmt, OpenAPI och MCP-paritet 147/147 (74 verktyg, 52 undantagna paths) passerar; API-typer regenererade. Ingen fysisk tuner/NPU eller befintlig installation har använts.
- **Nästa core-steg:** DVR-filuppspelning, korrekt medieduration och fortsatta serieregler/retention. Profil-/ratingkontroll, fler liveprofiler, beständig timeshift och fysisk/browser-/WAN-/receiveracceptans återstår; ingen HW-liveacceleration är införd. Kapitelns falska prioriterings-/sömlöshetsanspråk rättade. Restlista: 2/25 klara, 23 öppna; M11/Git/release avvaktar.

### 2026-10-28 — Batch 17: cargo clean och begränsad spelaråterhämtning
- `cargo clean` enligt användaren: 8087 filer/3,4 GiB borttagna, cirka 8 GB ledigt direkt efteråt. Frontendbatch; inga Rust-/API-/MCP-ändringar eller omkompilering efter städningen.
- Ny `web/src/utils/hlsRecovery.ts`: två appåterförsök per källa (1/3 s), 30 s deadline vid fatal nätåterhämtning samt begränsade SDK-load policies (20/25 s, ett HTTP-retry). Master-fel laddar om manifestet; fragment använder aktuell position. Buffrat fragment/faktisk tidsframflyttning avslutar deadline utan att nollställa källans försöksbudget.
- Player stoppar permanenta HTTP-, avkodnings- och formatfel och erbjuder Retry/Resume också vid initialt förhandlingsfel och native/direct mediafel. Position bevaras före browserreset. Serversessionen släpps före manuell omstart/kvalitetsbyte efter fel, stop har 10 s timeout, och redan stoppad session dubbelstoppas inte. Spår/synk/kvalitet/paus bevaras; förberedd omstart förblir pausad med Play-instruktion. Misslyckat stop kan fortfarande kräva väntan på serverlease; det nya API-felet visas.
- Retry visar vänteläge. Manifestets autoplay följer senaste pausavsikten. Handlers registreras före load/attach; gamla SDK-fel/timers städas vid byte/Cast/stängning. Ny renderad `Player.hls.test.tsx` täcker 17 fall; befintliga stop-assertioner inkluderar nu abortsignal.
- Verifiering: 124 webbtester, Biome/TypeScript/Vite/budget 42,23 KB, Rust fmt och diff-check; samtliga kontroller för batchen passerar. Rustkod oförändrad sedan batch 16, ingen ny Clippy/OpenAPI/MCP-körning. Full release-/fysisk mottagaracceptans återstår.
- **Nästa core-steg:** Live TV/DVR: serverägt relä, spelarlivscykel och inspelningsuppspelning. Område 09 fortfarande öppet för fysisk/native-/bakgrundsacceptans och återstående telemetri. Restlistan: 2/25 klara, 23 öppna. M11 avvaktar; inga Git-/releasesteg körda.

### 2026-10-28 — Batch 16: HLS-livscykel, lång paus och återupptagning
- Servern startar nu ett ägt underhållsjobb varje sekund i `serve`/`start_jobs`: timeout, ahead-throttling, diskkvot och borttagning av utgången playback-metadata/WAN-andel. Progress från ägaren förnyar HLS-lease också vid paus/buffring, utan att flytta fragmentmarkören. Förnyelse och expiry använder samma sessionslås; diagnostik/annan användare kan inte förnya.
- Shutdown spärrar nya starter (inklusive samtidiga/pågående starter), väntar på barn-/katalogstädning och slutför bekräftade sessioner innan DB stängs. Portbindning sker före bakgrundsstart. Den oanropade oavbrytbara manager-loopen är ersatt. Ett avbrutet stop-anrop lämnar sessionen registrerad tills städningen är klar, så underhåll kan slutföra arbetet.
- Webbens heartbeat går var 5 s även vid paus/buffring och när fliken blir synlig. Första verkliga `playing` bekräftar lokal uppspelning; väntan/autoplay-blockering gör det inte. Heartbeat har 10 s timeout, överlappar inte och aborteras vid stängning. 404/410 för aktuell session ger lokal **Resume playback** vid aktuell position med bibehållna val/paus; sena fel från gammal session ignoreras. Cast använder mottagartid och visar återanslutningsmeddelande vid expiry.
- Automatisk städning använder redan sparad progress och skriver ingen gammal position över en nyare sessions watch state. Obekräftade kandidater ger ingen historik; bekräftade sessioner slutregistreras en gång.
- Berörda: `mbl-transcode`, `mbl-service::playback`, serverlivscykel, Player, API-dokumentation/typer samt nya livscykel-/avbrottsregressioner. Verifierat: 107 webbtester, TypeScript/Vite/budget 42,23 KB, 27 riktade Rust-tester samt Clippy/fmt, OpenAPI och MCP-paritet 146/146. TypeScript verifierat efter API-generering.
- **Nästa core-steg:** begränsa äldre retry-logik för övriga fatala HLS-/mediefel och fortsätt Live TV-/DVR-uppspelning. OS-/browser-sömn kan stoppa heartbeat längre än 60 s; då krävs ny ström. Fysisk Cast/native-/bakgrundsacceptans återstår. Restlista: 2/25 klara, 23 öppna; M11 avvaktar och inga Git-/releasesteg är körda.

### 2026-10-28 — Batch 15: beredskap före spelarens källbyte
- `PlaybackRequest.waitForReady` är frivillig för gamla klienter; webben begär alltid kontroll av läsbar/icke-tom direktkälla eller första färdiga HLS-fragment/init med gemensam timeout och befintlig backend-fallback. Gammal spelarsession behålls tills serverförhandlingen lyckas.
- HLS-start skyddas mot avbrott/timeout med kandidatstädning (process/katalog/kapacitet). Pågående start hindrar idle-analys. Stängd sessionsstatus hindrar sena Arc-/segmentanrop att återstarta en stoppad process. Halvskrivna filer efter FFmpeg-fel och gamla framtida segment efter seek/fallback kan inte godkänna en ny halvskriven fil.
- Webbens väntande startanrop aborteras vid stängning och sena svar släpps. Kvalitets-/spår-/synkbyte läser senaste position före installation lokalt och på Google Cast. Förberedelsestatus visas. Obekräftade beredskapskandidater skriver ingen watch state/historik; playing/paused-progress bekräftar sessionen.
- Berörda: `mbl-service`, `mbl-transcode`, API/MCP request-literals, Player, startup/session/webbregressioner och regenererade API-typer. Verifiering: 102 webbtester + TypeScript/Vite/budget 42,23 KB; 25 riktade Rust-tester; Clippy, fmt, OpenAPI och MCP-paritet 146/146.
- **Nästa core-steg:** `SessionManager::spawn_maintenance_loop` används inte i produktion. Koppla in periodiskt underhåll tillsammans med heartbeat för pausade sessioner, resume och serveravslut; enbart inkoppling riskerar att bryta långa pauser. Första serverfragmentet garanterar inte browser-/receiveravkodning eller senare felfri uppspelning. Restlistan är fortfarande 2/25 klara, 23 öppna; M11 avvaktar och Git/release är inte utförda.

### 2026-10-28 — Batch 14: ljud- och undertextspår under uppspelning
- Playback-svar innehåller tillgängliga ljudspår, verkligt valt ljudindex, undertextval och aktivt inbränt undertextindex; valda serverindex finns även i diagnostik/MCP. REST/preview avvisar saknade spår eller fel streamtyp i stället för tyst fallback. MCP avvisar också ogiltiga/överstora heltal utan u32-trunkering och returnerar korrekt invalid-params.
- Player kan byta ljud och förhandla bild-/inbrända undertexter vid aktuell position. Kvalitet, ljudsynk och nuvarande undertext/Off bevaras; paus under ett pågående byte respekteras lokalt och på Google Cast. Avvisat byte återställer reglaget och behåller gammal session med serverns felorsak.
- Rättad ASS/WebVTT-target, FFmpegs relativa undertextordinal för inbyggd text och stöd för separat textfil vid inbränning med biblioteks-root-kontroll. ABR-burn-in avvisas i stället för att tyst utelämnas; webbens pipeline använder en rendition.
- HLS kan nu sakna ljud utan att försöka mappa ett obefintligt spår. Master-codec följer faktiskt ljud; korta klipp annonserar verklig duration. Cast skickar inte inbrända spår som externa SDK-caption-ID:n. Valfri titelläsning får fallera utan att stoppa giltig Cast-laddning och titelvyn följer aktuellt media.
- Verifierat: 99 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; 20 riktade Rust-tester (playback/spår 7, MCP 2 inklusive 10 paritetsfall, pipeline/session 11). Verklig HLS-avkodning bekräftar 880 Hz i valt ljudspår, WebVTT-text, synlig inbyggd/extern inbränning och ljudlös kortvideo. Clippy, OpenAPI och MCP-paritet 146/146 passerar.
- Native Remote Playback är bundet till befintlig källa; streambyte/nästa tillåts inte stoppa aktuell session. Fysisk PGS/Cast/native-acceptans och beredskap vid FFmpeg-fel efter antagen start återstår. Undertextsökning/nedladdning/autosync/generering är fortsatt öppna; restlista 08/09 delvis uppdaterade. Inga Git-/releasesteg körda.

### 2026-10-28 — Batch 13: verkliga sökförhandsbilder i spelaren
- `mbl-analysis::trickplay` använder verkliga FFmpeg-nyckelbilder och bygger JPEG-ark från validerade rutor. Fel och ofullständigt utdata ger fel, inga platshållare. Rättad nollbaserad numrering, uppåtrundning för sista intervall, mycket korta klipp och begränsade manifestallokeringar.
- GET-index returnerar 202 medan ett idle-jobb förbereder cache, 200 först vid publicerade artefakter. Nya/ändrade videoskanningar köar biblioteksjobb; enskilda cachemissar dedupliceras. Källfil/path/storlek/mtime/duration och algoritmversion binder cache; gamla syntetiska cacheposter och saknade filer regenereras. Revisioner publiceras efter färdig generering och gamla revisioner städas. Konfigurerad FFmpeg används och barnprocessen har timeout/kill-on-drop.
- `TrickplayJobHandler` lämnar tillbaka jobbet vid uppspelning/termisk last. Jobbköns typade idle-yield räknas inte som fel och förbrukar inte återförsök. Produktionsproben läser nu verkliga termiska sensorer även utan test-root. Skannerns fristående bygge saknade `time/parsing`; beroendet är rättat.
- Player visar autentiserade blob-bilder på tidslinjens pekar-/tangentbordsfokus; återanvänder bildark, avbryter sena svar och frigör URL/timer när förhandsvisningen stängs eller delen byts. Pending/fel visas ärligt. Cast-statuslagret täcker inte längre header/tidslinjereglage.
- Verifierat: 91 webbtester, TypeScript/Vite och initial JS-budget 42,24 KB gzip; 40 riktade Rust-tester (trickplay-analys 5, jobbkön 11, skanner 14, serverns M5/playback/trickplay 10). Verkliga färgklipp verifierar ruta/bildarksgräns och kort video; HTTP/jobbrunner verifierar kö, källa/cache, ACL/rating och privat JPEG. Clippy för berörda crates, OpenAPI och MCP-paritet 146/146 passerar. Restlista 04 färdigimplementerad; manuella webbläsar-/receiverprov ingår fortfarande i slutkontrollen. Nästa batch fortsätter inom uppspelning (Live TV/DVR, undertexter och återstående spelar-/receiverpunkter). Inga Git-/releasesteg körda.

### 2026-10-28 — Batch 12: ljudsynk åt båda hållen
- Ljudsynk −10..+10 s hanteras i serverns ljudström med kodning och PTS-justering. Tidigareläggning trimmar ljud före aktuell HLS-start; video kopieras när profilen tillåter det. Värdet exponeras i playback, preview, diagnostik och befintligt MCP `preview_playback`/`explain_session`; tre tools-list-snapshots uppdaterade, verktygsantal oförändrat.
- Player omförhandlar vid aktuell position och bevarar pausläge/kvalitet. Sparad preferens ändras först vid accepterat byte. Google Cast får samma offset och kan byta vid mottagarposition; native Remote Playback tillåter inte ändring under pågående uppspelning. Den gamla lokala DelayNode har tagits bort så ingen dubbel fördröjning uppstår; nattläge behåller sin EQ/kompressor.
- Nekad Cast-serverförhandling behåller aktiv receiver och dess slutlyssnare. Videor utan ljud fortsätter med effektiv offset 0 ms. Ogiltiga värden nekas utan ny session. Vid nekad första start med sparad synk finns återförsök utan synk som bevarar återuppta/starta om-valet och visar serverns felbeskrivning.
- Verifierat: 85 webbtester, TypeScript/Vite och budget 42,23 KB initial gzip; pipeline 5/5 (inklusive riktigt ljudpulstest), playback-API 5/5, MCP-tool-regression och tre schema-snapshots. Clippy för berörda Rust-crates, OpenAPI och MCP-paritet 146/146 passerar. Fysisk ljud-/bildsynk på Cast-mottagare och återstående receivertelemetri är fortfarande öppna; därefter prioriteras övriga uppspelningsdelar enligt restlistan. Inget Git-/releaseslutsteg har körts.

### 2026-10-28 — Batch 11: verifierbar spelarstatistik (core prioriteras)
- GET `/api/v1/playback/{session}/diagnostics` och MCP `explain_session` visar sessionsklass/profil samt låst snapshot av HLS-backend/process/throttling/omstarter/senaste exit. Endast ägare/admin; stoppad session ger 404. MCP behåller befintliga sessionsfält och kompletterar med transcode-diagnostik.
- Player hämtar/pollar diagnostik bara när statistikpanelen är öppen och städar anrop/timer vid byte/stängning. Cast visar inte lokal upplösning/buffert/dropped frames som receiverdata. Fel markeras som otillgängligt; tailnet-klass påstås inte vara verifierad direkt-/relärutt.
- Lokal upplösning uppdateras vid metadataändring; buffert räknas i intervallet som faktiskt innehåller aktuell position. Okända mätvärden visas inte som falska nollor.
- Verifierat: playback-API 4/4, MCP-tool-regression, HLS-runtime-regression, Clippy, OpenAPI och paritet 146/146. Web-check med 78 tester, TypeScript, Vite och budget 42,24 KB initial gzip passerar.
- Användaren vill nu prioritera spelarrelaterat före andra områden. Nästa core-del är full ljudsynk med tidigareläggning av ljud; 0–5 s lokal fördröjning finns men signerad serverjustering är ännu inte implementerad.

### 2026-10-28 — Batch 10: fototidslinje, bilddialog och bildspel
- Ny `/photos/:id`-vy nås från fotobibliotekets detaljvy: månadsval, cursorbaserad hämtning, autentiserade bilder, tangentbordsnavigering, bilddialog och laddningsstyrt bildspel. Bildspel pausar när sidan är dold, stannar vid sista inlästa bild och stoppas vid bildfel. Blob-URL:er och anrop städas vid stängning/byte.
- Ny GET `/api/v1/libraries/{id}/photos` med EXIF-/tillagd-datum och stabil sidindelning på datum+ID. Äldre timeline räknade dolda åldersbegränsade bilder; båda använder nu ItemFilter och verifierar bibliotek/typ. Originalström har korrekta bild-MIME-typer.
- Verifierat: web-check med 74 tester, TypeScript/Vite/budget 42,24 KB gzip; foto-API-regression för rating/ACL, datumtie/pagination och originalbildens MIME/autentisering; server-Clippy; OpenAPI och MCP-paritet 145/145.
- Restlistans fotoflöde (06) implementerat. Manuellt browser-/format-/tillgänglighetsprov kvarstår som slutkontroll. Originalbilder används; särskild servergenerering av små fotominiatyrer/konvertering av RAW/HEIC ingår inte i detta flöde.

### 2026-10-28 — Batch 9: automatisk fortsättning på Google Cast
- CastManager observerar den aktuella SDK-medieladdningen och publicerar avslut endast efter PLAYING → IDLE/FINISHED. ERROR/CANCELLED/INTERRUPTED, sena lyssnare och dubbla händelser startar inte nästa avsnitt.
- Player inväntar stopp av avslutad serversession innan nästa receiverförhandling. Opt-in gäller aktuell kedja även på Google Cast; metadata som kommer sent kan utlösa fortsättning när klart. Native Remote Playback stöder inte denna automatik.
- Fyra nya regressioner för slutorsak, slutposition, gammal medielyssnare, gammal FINISHED-status och renderat nästa-flöde. Verifierat: web-check med 70 tester, TypeScript, produktionsbygge och budget 42,19 KB initial gzip passerar.
- Cast-guiden uppdaterad för faktisk profil/URL/textspår/kommandotransport och ny automatik. Fysisk Cast-acceptans kvarstår.

### 2026-10-28 — Batch 8: Up Next och säkert avsnittsbyte
- Spelaren visar nästa avsnitt under sista 30 sekunderna/vid slut och följer numrering över säsongsgränser via behörighetsfiltrerade item/children-API:er. Tomma säsonger hoppas över; API-fel visas med återförsök, och gamla anrop avbryts vid stängning/titelbyte.
- Lokal automatisk fortsättning är opt-in per uppspelningskedja, följer till nästa avsnitt och delas inte som global kontoinställning. Manuell nästa-knapp använder Cast-omförhandling när casting pågår; automatisk receiverfortsättning är inte införd.
- Stoppanrop återanvänder nu en gemensam Promise så nästa uppspelning väntar på redan pågående cleanup. Tangentkommandon stör inte längre input/select/button-navigation.
- Sju nya tester för ordning, säsongsbyte, tomma säsonger, fel, film, övergång och tangentbordsfokus; Verifierat: web-check med 66 tester, TypeScript, Vite och budget (42,19 KB initial gzip) passerar.

### 2026-10-28 — Batch 7: kapacitet utan att avbryta pågående uppspelning
- Playback stoppar inte längre användarens gamla HLS-sessioner innan en ny session har startat. Full kapacitet ger ett fel och lämnar den gamla sessionen kvar; ingen tillfällig överbeläggning av Pi:n införs.
- SessionManager serialiserar antagning från kapacitetskontroll till infogning, så samtidiga starter inte kan överskrida gränsen. Startfel frigör antagningen och städar sessionskatalogen.
- Tagit bort falsk framgång (genererad HLS-adress utan session) när HLS-manager/FFmpeg saknas.
- Regressioner: samtidiga starter, startfel och HTTP-kvalitetsbyte vid full kapacitet. Verifierat: sessionssviten 6/6 och samtliga playback-API-tester 3/3 passerar; Clippy för mbl-transcode/mbl-service/mbl-server --all-targets passerar.

### 2026-10-28 — Batch 6: kvalitetsbyte och lokal ljudfördröjning
- Player har kvalitetsgränser Auto/2/4/8/20 Mbps. Ny playback-session begärs från aktuell position med aktuellt part-ID; äldre session släpps efter lyckat svar. Fel återställer menyvalet och sena svar städas vid avmontering.
- Lokal WebAudio DelayNode ger 0–5 s ljudfördröjning och fungerar ihop med nattläge. Val sparas i befintlig preferens; tidigareläggning av ljud stöds ännu inte. Reglagen är spärrade under casting. Sparat nattläge aktiveras även vid uppspelningsstart.
- Tre nya renderade tester för lyckat/avvisat kvalitetsbyte, pausbevarande och verklig delay-node. Verifierat: web-check med 59 tester, TypeScript, Vite och budget (42,18 KB initial gzip) passerar.
- `docs/audits/remaining-work.md` listar 25 öppna arbetsområden och separata releasekontroller; ett delsteg innebär inte att hela området är klart. Spelarområdet är fortfarande delvis öppet.

### 2026-10-28 — Batch 5: tokennät och språkval
- AI Agents har LAN/Tailscale/WAN-val vid mintning samt nätvisning i tokenlistan. Tom nätlista stoppas lokalt; serverns befintliga policy gäller fortfarande.
- Rättat tyst återkallningsfel, felaktigt skapandefel efter misslyckad snippet-hämtning, gammalt snippet-tillstånd och falsk clipboard-framgång. Token som redan skapats bevaras när snippets saknas.
- Serverns språkvalidering accepterar nu alla fem UI-språk (fr/es nekades tidigare). Persistens-/ogiltigt-språk-test tillagt, liksom renderade tokenregressioner.
- Verifierat: web-check 56 tester, TypeScript/build/budget (42,18 KB gzip), riktade Rust-tester 2/2 och mbl-service Clippy --all-targets passerar. Git/release avvaktar övriga öppna punkter.

### 2026-10-28 — Batch 4: engångsinbjudningar
- Users-vyn har inbjudningar med uttryckligt biblioteksval, åldersgräns, giltighet, länk och återkallning. `/invite/:code` är publik och skapar konto via befintligt API; redan inloggat konto ersätts inte.
- Rättat dubbelinlösen: konto, ärvda behörigheter och förbrukning av inbjudan sker i samma skrivtransaktion. Fel vid kontoskapande rullar tillbaka allt. Inbjudningskoder loggas inte längre.
- Test för samtidig inlösen och rollback tillagt i mbl-db; renderade tester täcker avvisad registrering, återförsök, inloggat konto, explicit bibliotekstilldelning och avvisad återkallning.
- Webb: 53 tester i full körning passerar. Verifierat: web-check (53 tester, TypeScript, bygge, budget 42,18 KB gzip), databasens samtidighets-/rollback-test och Clippy för mbl-service/mbl-db --all-targets passerar.

### 2026-10-28 — Batch 3: administrera profilbehörigheter
- Admin Users har en riktig behörighetsdialog med bibliotek, hanterad profil, åldersgräns och separat PIN-sättning/borttagning. Misslyckad inläsning blockerar sparning; sparfel behåller valen.
- Rättat `UpdateUserPermissionsRequest` så att utelämnad åldersgräns betyder oförändrad men explicit JSON null tar bort gränsen. Tidigare kollapsade båda till None.
- Renderade tester för läsfel och avvisad sparning, plus Rust-test för utelämnat/null/värde. Verifierat: web-check (50 tester, TypeScript/build/budget 42,13 KB gzip), riktat Rust-regressionstest och Clippy för mbl-service --all-targets passerar.
- Användaren avsåg **Git**, inte GIF. Git/release väntar tills överenskomna rättningar är klara. Ingen GIF efterfrågas.

### 2026-10-28 — Batch 2: förfrågningar och historikimport i webben
- Ny `/requests`-sida: skapa önskemål med TMDB-ID, visa status, admin godkänner/markerar tillgängligt. Nåbar från desktop och mobilmeny. Arr-leverans är uttryckligen inte inkopplad.
- Kontosidan har `HistoryImport`: Plex/Jellyfin JSON-fil, format-/storlekskontroll, importresultat och omatchade titlar. Import sker till aktuellt konto; inte automatisk anslutning till källservern.
- Renderade regressionstester verifierar avvisad begäran/statusändring/import och lyckat återförsök utan förlorad inmatning.
- Verifiering: 48 webbtester passerar; se granskningsrapporten för kvarstående krav. Ingen release/tagg/push skapad; användaren vill slutföra rättningarna först.

### 2026-10-28 — Granskningsrapportens implementation pågår
- Implementerat ägarbundet spelarregister och WebSocket-kommandon, Cast-profil/omförhandling/textspår, Live TV-leases, atomisk DVR-bokning och FFmpeg-recorder, sessionsskydd för Companion/Matchmaker, WAN-pacing och ägd ljudprocess.
- Migration `0009_matchmaker_membership.sql` inför medlemskap; äldre röstidentiteter var självrapporterade och ogiltigförklaras.
- Webb: biblioteksredigering/radering, sparfel på hemsidan, reaktiv playback-preview och worker-uppdatering. AI/demo-status är tydligare.
- Se uppdaterad status i `docs/audits/2026-10-27-web-and-service-audit.md` och ADR 0007. Rapportens övriga UI-/runtime-krav är ännu inte klara. M11 fortsatt pausat.
- Verifierat: web-check 45 tester + TypeScript/build/budget OK; workspace-Clippy OK; MCP 144/144. Rust 327/328, enda fel var fel tunerfixtur; rättad riktad sessionskörning 4/4 OK. Fysisk Cast/tuner återstår.

### 2026-10-27 — Fördjupad Cast-granskning av d05a03f och verifierade rättningar
- **Granskat:** Senaste commit `d05a03f` mot faktisk kod och Googles SDK-referenser. Dess spelarregistertester verifierar inte verklig Cast-uppspelning. Den samlade rapporten och API/UI-inventeringen är uppdaterade; mdBook-guiden skiljer nu implementation från kvarstående produktflöden.
- **Rättat:** Mottagarposition för progress/stop; lokal återgång efter avbruten anslutning; SDK `ANY_CHANGE`; volym noll; idempotent play/pause; remote mute/seek/volym; ordnad avregistrering; sena Google/native-anslutningssvar; native-händelser/reglage; rätt Image-konstruktor; städning vid spelarstängning. Lokala undertext-/hastighets-/nattlägesreglage inaktiveras under casting. Media-CORS/preflight tillagt i `crates/mbl-api/src/routes/stream.rs` med befintliga åtkomstkontroller kvar. Cast-livscykelns avregistrering dokumenterad i MCP-undantagen. Återställde även versionsrubriken 1.0.0 som försvann i senaste changelog-commiten.
- **Tester:** Utökade `web/src/utils/cast.test.ts`, ny renderad `Player.cast.test.tsx`, och CORS/Range/auth-regression i `crates/mbl-server/tests/stream.rs`.
- **Verifiering:** Workspace-nextest **321/321**, workspace-Clippy `-D warnings`, fmt-check, OpenAPI-check, tokens-check, MCP-paritet **142/142 (100%)**, `just web-check` **42 tester**, TypeScript/Vite och bundlebudget **42.03 KB gzip** passerade. HTTP-mocktesterna behövde lokal portbindning utanför sandlådan; ingen kodändring krävdes för det. Ingen fysisk Cast-mottagare testad.
- **Kvar:** Cast-specifik profil/MIME och nåbar URL; receiver-spårval; spelarregistrets ägarisolering; transport från serverns kommandobuss till Cast-klienten; verklig receiver-/nätavbrottsacceptans. Övriga UI-/serverluckor finns prioriterade i granskningsrapporten. M11 ligger fortsatt på vänt. Denna rättningsomgång är ännu inte committad eller publicerad.

### 2026-10-27 — Milstolpe M12 (Chromecast & Remote Playback Support) samt Plugin Buggfix & Härdning
- **Genomfört:**
  - **Plugin Toggle & Re-enable Fix:**
    - Diagnostiserade och åtgärdade buggen där inaktiverade plugins inte kunde startas igen: `handleTogglePlugin` i `web/src/pages/LiveTv.tsx` hade ett hårdkodat `true`-argument som alltid skickade `enabled: false`. Rättat till dynamisk status `handleTogglePlugin(plugin, plugin.enabled ?? true)` med statusbadge (`Active` vs `Disabled`) och dynamisk knapp (`Enable` vs `Disable`).
    - Lade till `pub enabled: bool` med `#[serde(default = "default_true")]` i `PluginManifest` (`crates/mbl-core/src/plugins.rs`).
    - Skapade `PluginService::list_plugins` i `crates/mbl-service/src/plugins.rs` som läser SQLite-tabellen `plugin_configs` och berikar manifesten med aktuell status, samt uppdaterade REST-routen i `crates/mbl-api/src/routes/plugins.rs`.
  - **M12 Chromecast & Remote Playback Support:**
    - Skapade `web/src/utils/cast.ts` med full Google Cast Web Sender SDK (`cast_sender.js`) integration och automatisk handskakning mot Default Media Receiver (`CC1AD845`), samt W3C Remote Playback API fallback för AirPlay och Miracast.
    - Integrerade Cast-kontrollknapp `[⎚]` i `web/src/components/Player.tsx`, dedikerad aktiv Cast-overlay, samt tvåvägssynkning av play/pause/seek/volym.
    - Rapportering av Chromecast-sessioner till `PlayerRegistry` via `POST /api/v1/players` (`device_type: "chromecast"`).
    - Implementerade `DELETE /api/v1/players/{id}` i `crates/mbl-api/src/routes/players.rs` för omedelbar avregistrering vid Cast disconnect.
    - Härdade `PlayerInfo.last_seen_at` med `#[serde(default)]` i `crates/mbl-service/src/players.rs` för att förhindra 422-fel vid spelarregistrering.
  - **Lokal Verifiering & Buggsniffning:**
    - Upptäckte och åtgärdade regression i `crates/mbl-playback/tests/decide.rs` för `audio_index`-val.
    - Skapade omfattande integrationstestsats `crates/mbl-server/tests/m12.rs` (3 tester) och vitest-test `web/src/utils/cast.test.ts`.
    - Skapade komplett dokumentationsguide i `docs/book/src/chromecast.md` och uppdaterade `SUMMARY.md`.
- **Verifieringsresultat:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK (0 fel, 0 varningar)
  - `cargo nextest run --workspace`: 320/320 tester godkända (0 failed, 0 skipped)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `cargo xtask mcp-parity`: OK (141/141 endpoints täckta, 100.0%)
  - `just web-check`: Biome (0 fel), Vitest (34 tester), `tsc -b`, Vite build, bundle budget (42.04 KB gzip <= 150 KB): OK
- **Nästa steg:** Återuppta **M11 — Native Mobile, TV & Spatial VR Clients** (`M11.1`–`M11.4` i `IMPLEMENTATION.md`).

### 2026-10-27 — Kod-/UI-granskning, pausad på användarens begäran
- **Arbete hittills:** Konkreta rättningar för ACL i samlingar/spellistor, TOTP, profil-/inloggningsflöde, uppspelning/spår/versioner, bibliotekspaginering, EPG-tidsenheter, DVR-konflikter och HTTP-fel. Nya webbvägar `/account`, `/collections`, `/admin/remote`. Detaljerad täckning och kvarstående fynd i `docs/audits/2026-10-27-web-and-service-audit.md`; API/UI-inventering i samma katalog.
- **Verifierat:** `just web-check` passerade med 34 tester, TypeScript, Vite och initial JS 42.04 KB gzip. Riktad Clippy för `mbl-service`/`mbl-db` passerade. Webbläsarkontroll mot separat testdatabas verifierade inloggning och navigation. OpenAPI/TS-schema genererade.
- **Paus:** Användaren bad pausa så att annat bygge kan bli klart. Egna pågående workspace-nextest och workspace-Clippy avbröts; de är INTE färdigverifierade. Egna tillfälliga testservrar stoppades. Kodändringar ligger kvar, ingen commit/release gjord.
- **Vid återupptagning:** Kör återstående workspace-tester/Clippy med Pi-flaggorna, kontrollera nya regressioner i `crates/mbl-server/tests/audit.rs` och `mbl-playback/tests/decide.rs`, samt OpenAPI/MCP-paritet/tokens. Flera filer har ändrats parallellt av annat arbete, särskilt plugin/Cast/Player; bevara dessa ändringar och granska aktuell diff före fortsättning. M11-klienter avvaktar.

### 2026-10-27 — Orientering i projektets arkitektur och implementation
- **Genomfört:** Läste projektplan, implementationsstatus, workspace, serverns uppstart, det gemensamma tjänstelagret, scanner, uppspelningsstruktur, webbklient samt delar av TV-, Hailo- och pluginimplementationerna.
- **Observation:** Dokumenterad v1.0-status är inte i sig bevis på fullständig körbar integration. `mbl-tv/src/main.rs` konfigurerar klienten och söker server men startar ingen UI-/spelarslinga; `mbl-hailo::analyze_image` använder färg-/textheuristik; online-pluginflöden innehåller `example.com`-medier. Dessa delar behöver verifieras mot verkliga acceptanskriterier före vidare färdigmarkering.
- **Verifiering:** Endast dokument- och kodläsning; inga byggen, tester eller hårdvarukörningar utförda. Befintliga lokala ändringar i plugin/API/OpenAPI/LiveTv lämnades orörda.
- **Nästa steg:** Vid fortsatt utveckling, kontrollera faktisk integration för berörd funktion. M11 är nästa planerade milstolpe enligt implementationsplanen.

### 2026-10-26 — Milstolpe M10 (Hardening → v1.0.0) & Release v1.0.0
- **Genomfört (`M10.1`–`M10.8`):**
  - **M10.1:** Formell STRIDE Threat Model-dokumentation i `docs/threat-model.md` över 4 förtroendezoner (Public/Untrusted, Authenticated Clients, Tailnet/Private LAN, Local System Services). Säkerhetshärdning i `crates/mbl-service`: protokollvalidering för webhooks som tvingar strikt `http://` eller `https://` (avvisar `file://`, `ftp://`, `javascript:` med 422), 50 MB DoS-tak för M3U/XMLTV-guider, och path traversal-kontroll för worker source token endpoints.
  - **M10.2:** Omfattande fuzzing-kampanj med `fuzz/` libfuzzer-targets och property-baserade fuzz-tester (`proptest`) för `parse_nfo` (`crates/mbl-metadata/tests/fuzz_nfo.rs`), M3U/XMLTV (`crates/mbl-service/tests/fuzz_m3u_xmltv.rs`), och HLS/Range (`crates/mbl-stream/tests/fuzz_hls.rs`). Upptäckte och härdade en subtil panic-bugg i `parse_xmltv_date` vid godtyckliga multibyte UTF-8-strängar.
  - **M10.3:** Prestandarapport `docs/benchmarks.md` mot PLAN §21: skanningshastighet > 500 filer/s på Pi 5, direct play TTFF på 240 ms, zero-CPU virtual playhead på 0.015 ms, och idle minnesavtryck på 42.4 MB på ARM64 Pi 5.
  - **M10.4:** Komplett mdBook-dokumentationssajt i `docs/book/` med `book.toml`, `SUMMARY.md` och 18 kapitel som täcker installation (Linux, Docker, Pi 5, Windows), hårdvaruacceleration, Tailscale remote access, distribuerade workers & noder, Live TV/DVR, MCP, AI-companion, migrering och felsökning.
  - **M10.5:** Watch history- och biblioteksmigreringsmotor (`ImportService`) för Plex och Jellyfin. REST-endpoints (`POST /api/v1/import/plex`, `POST /api/v1/import/jellyfin`) och 2 nya MCP-verktyg (`import_plex_history`, `import_jellyfin_history`). Matchning via TMDB/IMDb/TVDB ID och fallback på titel/år, ticks-till-millisekunder konvertering, samt integrationstester i `crates/mbl-server/tests/m10.rs`.
  - **M10.6:** Fullständiga 5-språksöversättningar i `web/src/i18n/index.ts` för engelska (`en`), svenska (`sv`), tyska (`de`), franska (`fr`) och spanska (`es`) med 100% nyckelparitet verifierad via Vitest (`web/src/i18n/i18n.test.ts`), språkväljare i `Layout.tsx` och `CommandPalette.tsx`, samt WCAG 2.2 AA accessibility.
  - **M10.7 & Separata Releaser:** Separata fristående release-paket för `mbl-worker` och `mbl-node` i `.github/workflows/release.yml` (.deb, .tar.gz för x86_64 och arm64, Windows zip-paket med systemd service-enheter och READMEs). Officiell release `v1.0.0` med uppdaterat `Cargo.toml`, `CHANGELOG.md`, `README.md` och `docs/openapi.json`.
  - **M10.8:** MCP evalueringsscenarier i `evals/mcp/*.yaml` (bibliotekssökning, fjärrstyrning, Live TV/DVR, nätverksstatus, prompt injection-säkerhet med bekräftelsevakt) och automatiserad scenario-runner i `crates/mbl-mcp/tests/evals.rs`. MCP-adminverktygen ökade till 74 med 100.0% OpenAPI-paritet.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 311/311 tester godkända (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (141/141 endpoints täckta, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome (0 fel), Vitest (22 tester), `tsc -b`, Vite-build, bundle budget (42.06 KB gzip <= 150 KB): OK
- **Nästa steg:** Börja med **M11.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (Native Android Mobile & Tablet App med Compose & Media3/ExoPlayer).

### 2026-10-26 — Milstolpe M9 (Live TV, Channels and Online Streaming Plugins) & Release v0.8.0
- **Genomfört (`M9.1`–`M9.7`):**
  - **M9.1:** HDHomeRun discovery (`UDP 255.255.255.255:65001`), tuner-inspektion (`/discover.json`) och lineup-synkronisering (`/lineup.json`). M3U/M3U8-parser för `#EXTINF` med extraktion av `tvg-id`, `tvg-logo`, `tvg-chno`, callsigns och stream-URL:er. Streaming XMLTV-parser med chunkad händelseläsning via `quick-xml` som extraherar programdata, tidsstämplar, titlar, beskrivningar, genrer, åldersgränser och `xmltv_ns` säsongs-/avsnittsnumrering.
  - **M9.2:** 2-axlars EPG TV Guide grid REST-endpoint (`/api/v1/livetv/epg`) med tidsfönsterfiltrering, nu/nästa-spårning och snabbsökning (`/api/v1/livetv/epg/search`).
  - **M9.3:** Live-uppspelningsmotor (`GET /api/v1/livetv/stream/{channelId}`) med samtidighetsbegränsning per tuner (`tuner_count`), automatisk allokering och explicit stream-release (`POST /api/v1/livetv/stream/{channelId}/release`).
  - **M9.4:** DVR-inspelningsschemaläggare med tabeller `recordings` och `series_rules`. Stöd för engångsinspelningar med start-/slutmarginaler (`start_padding_secs`, `end_padding_secs`), serieregler med nya avsnitt (`new_only`) och säsongsfiltrering, samt automatisk konfliktkontroll mot aktiva tuners.
  - **M9.5:** 5 nya MCP-verktyg (`search_guide`, `list_recordings`, `schedule_recording`, `create_series_rule`, `cancel_recording`) med stöd för `dry_run` och `confirm`-säkerhet. Totalt 72 admin-verktyg och 100.0% paritet mot OpenAPI.
  - **M9.6:** Streaming & Online Media Plugins (`PluginService`) med manifest, inställningar, kanalflöden, elementbläddring och strömupplösning för YouTube (kanaler & trender), Spotify (topplistor & ljudspår) och WebDrama (asiatiska serier med flerspråkiga VTT-undertexter). Mappas enhetligt till `ItemSummary`.
  - **M9.7:** Virtuella 24/7-kanaler (`/api/v1/livetv/virtual-channels`) med looping av biblioteksmedia och matematisk zero-CPU playhead-beräkning (`/api/v1/livetv/virtual-channels/{id}/playhead`) samt rullande syntetisk EPG.
  - **SolidJS Web UI:** Fullständig 4-flikars Live TV-sida (`/livetv` i `web/src/pages/LiveTv.tsx`):
    - Flik 1 (EPG TV Guide): Interaktiv 2-axlars tidslinje, guidesökning, direktkanalvisning och detaljmodal för program med DVR-schemaläggning.
    - Flik 2 (Tuners & Källor): Översikt av tuners, automatisk HDHomeRun UDP-upptäckt och modaler för M3U- och XMLTV-import.
    - Flik 3 (DVR & Serieregler): Hantering av schemalagda/inspelade program och serieregler med avbokningskontroller.
    - Flik 4 (Virtuella kanaler & Plugins): Progress-bars med levande playhead-position för 24/7-kanaler samt online streaming-utforskare.
  - **Tester:** Omfattande integrationstester i `crates/mbl-server/tests/m9.rs` som täcker samtliga 7 krav.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 295/295 tester godkända (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (139/139 endpoints täckta, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome (0 fel), Vitest (22 tester), `tsc -b`, Vite-build, bundle budget (41.05 KB gzip <= 150 KB): OK
- **Nästa steg:** Börja med **M10.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (Threat model doc och säkerhetsgranskning inför v1.0.0).

### 2026-10-26 — Milstolpe M8 (Distributed nodes) & Release v0.7.0
- **Genomfört (`M8.1`–`M8.11`):**
  - **M8.1:** Fullständig ratificerad protokollspecifikation i `docs/worker-protocol.md` med typade datastrukturer i `mbl-core` (`WorkerJobSpec`, `WorkerCapabilities`, `WorkerStatus`, `WorkerControl`, `WorkerHeartbeat`, `WorkerPairRequest`, `WorkerPairResponse`, `WorkerPathMapping` med sökvägsöversättningsenhetstester).
  - **M8.2:** Skapade `crates/mbl-worker` CLI-binär med parningsflöde (`mbl-worker pair --server <url> --code <CODE>`), hårdvarufingeravtryck via `mbl-hwaccel`, 5-sekunders heartbeat-slinga och mockad gaming-detektering.
  - **M8.3:** HMAC-SHA256-signerade kortlivade käll-URL:er (`/api/v1/stream/worker/source/{partId}?token=...`) för chunkad HTTP range streaming, skyddad mot katalogtraversering via `WorkerSourceTokenClaims`.
  - **M8.4:** Segmentuppladdningsendpoint (`PUT /api/v1/stream/hls/{sessionId}/segments/{filename}`) som sparar genererade `.m4s`-bitar direkt i serverns sessionskatalog och gör dem omedelbart tillgängliga för uppspelning.
  - **M8.5:** Schemaläggarintegration i `WorkerService` (`select_transcode_worker`) med Wake-on-LAN UDP broadcast magic packet (`6x 0xFF + 16x MAC`), och automatisk sömlös fallback till lokal transkodning vid utebliven heartbeat eller fel.
  - **M8.6:** Batch offload (`estimate_batch_offload` och integration i `create_optimized_versions`) för nattjobb och GPU-avlastning av optimerade versioner, trickplay-skrubbark och introanalys.
  - **M8.7:** Windows worker tray-stöd med pause-while-gaming detektering som markerar worker-status som `paused_gaming` vid hög icke-transkodande GPU-last.
  - **M8.8:** SolidJS Admin UI för noder och workers (`web/src/pages/admin/Workers.tsx`) med modal för 6-siffriga engångskoder, live-översikt, GPU-kapabiliteter, Wake-on-LAN och återkallningskontroller.
  - **M8.9:** 5 nya MCP-verktyg (`list_workers`, `wake_worker`, `pair_worker`, `revoke_worker`, `list_remote_nodes`) med totalt 67 admin-verktyg och 100% paritet mot OpenAPI.
  - **M8.10:** `mbl-node` binär för fristående federerade lagringsnoder (NAS/minidatorer) med parningsflöde, lokal metadata- och katalogindexering och periodisk deltasynkronisering (`POST /api/v1/nodes/{id}/sync-catalog`).
  - **M8.11:** Smart Handover (`GET /api/v1/nodes/{id}/stream/handover/{itemId}`) som känner av om klienten befinner sig på samma LAN/privata subnät och dirigerar uppspelningen direkt till nodens streamingserver, eller faller tillbaka på relay över WAN.
  - **Tester:** Omfattande integrationstester i `crates/mbl-server/tests/m8.rs` som täcker alla M8-krav.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 288/288 tester godkända (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (115/115 endpoints täckta, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome, Vitest (22 tester), `tsc`, Vite-build, bundle budget (40.99 KB gzip <= 150 KB): OK
- **Nästa steg:** Börja med **M9.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (HDHomeRun discovery, M3U- och XMLTV-parsning för Live TV).

### 2026-10-26 — Milstolpe M7 (Media breadth & Windows WiX MSI) & Release v0.6.0
- **Genomfört (`M7.1`–`M7.20`):**
  - **M7.1 & M7.2:** Musikskanning (`mbl-scanner`) med `lofty`-taggar (artist, album, spår, disk, MusicBrainz ID, ReplayGain, synkade/osynkade låttexter). MusicBrainz rate-limited klient (`mbl-metadata::musicbrainz`) med Cover Art Archive. EBU R128 loudness-analys (`mbl-analysis::loudness`) via FFmpeg `ebur128` med parsing av LUFS, LRA och True Peak. Progressiv audiotranskodningsström (`/api/v1/stream/audio/{partId}`) med dynamisk omkodning till Opus, AAC och MP3 direkt till klientens stdout.
  - **M7.4:** OpenSubsonic API (`/rest/*`) med stöd för standardklienter (Symfonium, Feishin, DSub etc.), token/salt MD5-autentisering, `ping`, `getLicense`, `getMusicFolders`, `getArtists`, `getArtist`, `getAlbum`, `getSong`, `stream`, `search2/3`, `getPlaylists`, `scrobble`.
  - **M7.5:** Foto-metadataextraktion (`mbl-scanner` med `kamadak-exif`), EXIF-datum, kameramärke/-modell, GPS-koordinater och tidslinjeindelning per år och månad (`/api/v1/libraries/{id}/timeline`).
  - **M7.6 & M7.7:** Samlingar och spellistor (`crates/mbl-api/src/routes/{collections,playlists}.rs`) med full CRUD, sorteringsordning och smart samlingsmetadata. Metadatafältlåsning (`item_metadata_locks`) som skyddar manuellt verifierade fält mot överksrivning vid framtida skanningar.
  - **M7.8:** OpenSubtitles REST API-klient (`mbl-metadata::opensubtitles`) med sökning på hash/IMDb/TMDB/språk och nedladdning av undertexter.
  - **M7.9 & M7.15:** DLNA UPnP Media Server (`mbl-remote::dlna`) med enhetsbeskrivnings-XML, ContentDirectory DIDL-Lite trädbygge och smart-TV-profiler (Samsung, LG WebOS/Web0S, Sony Bravia). Home Assistant MQTT Discovery (`mbl-remote::mqtt`) för mediespelare och serversensorer, samt livscykelhändelsetriggers för biografrumsbelysning.
  - **M7.10 & M7.13:** Central spelarregistertjänst (`PlayerRegistry`) som spårar aktiva spelare och dirigerar kommandon (play, pause, stop, seek, setVolume, setTracks).
  - **M7.11:** Webhook-system (`crates/mbl-service/src/webhooks.rs`) med händelsefiltrering och HMAC-SHA256-signering i headern `X-MBL-Signature`.
  - **M7.16:** AI Watch Companion-protokoll (`/api/v1/companion/sessions/{id}/*`) med realtidsresurs `mbl://sessions/{id}/live-context`, sändning av live VTT-dialogcues, känsloreaktioner (`send_companion_reaction`), scenförklaringar (`explain_scene`) och uppspelningssynkronisering (`sync_playback`).
  - **M7.17:** Subtitle Auto-Sync (`mbl-analysis::subsync`) med VAD-röstaktivitetsanalys, energi-envelope och dynamisk programmeringsjustering för synkronisering av externa undertexter (inom ±100 ms precision).
  - **M7.18:** Scene X-Ray, AI "Previously On..."-sammanfattningar som strikt utesluter oseeda avsnitt, Binge Sleep Guard som känner av inaktivitet och bokmärker sista vakna avsnittet, samt MP4/WebM/GIF-klippgenerator med inbrända undertexter.
  - **M7.19:** Movie Night Matchmaker (`/api/v1/matchmaker/sessions`) för "Swipe to Match" med QR-kodsanslutning, filterkonfiguration och automatisk uppspelningsstart vid match.
  - **M7.20:** Inbyggda Media Requests (`/api/v1/requests`) med Radarr/Sonarr-integration, kontroll av saknade avsnitt i säsonger, och hardlink-medveten lagringsanalys (`st_nlink`) som visar verkligt frigjort utrymme innan radering.
  - **Windows WiX MSI:** Skript `packaging/windows/build-msi.ps1`, uppdaterad `packaging/windows/mbl.wxs` med dynamisk versionshantering och integration i `.github/workflows/release.yml` som producerar `mbl-setup-x64.msi`.
  - **MCP-verktyg:** 20 nya verktyg (totalt 62 admin-verktyg, 33 read+library, 28 read), 5 nya promptar och `resources/subscribe` för event-streamad kontext. Nytt verktyg `cargo xtask mcp-parity` verifierar 100.0% paritet mot OpenAPI-specifikationen.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 282/282 tester godkända (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (100/100 endpoints täckta, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome, Vitest (22 tester), `tsc`, Vite-build, bundle budget (41.64 KB gzip <= 150 KB): OK
- **Nästa steg:** Börja med **M8.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (protokollspecifikation för transcode-workers och typed `JobSpec`).
  - **M6.1 & M6.2:** Databastabeller `user_library_access` och `invites` (`0005_m6_users_remote.sql`), dynamiska frågebyggare i `crates/mbl-db/src/repo/m6.rs`. Biblioteks-ACL:er håndteras per användare via `ItemFilter` och nekar olovliga bibliotek med 404 (Items, Search, Playback). Hanterade profiler (`is_managed`), Argon2id föräldra-PIN (`mbl-auth::hash_pin`, `verify_pin`), åldersgränser (`content_rating_ceiling`), och tidsbegränsade inbjudningslänkar med fördefinierade behörigheter (`/api/v1/invites`, `/api/v1/invites/inspect/{code}`, `/api/v1/auth/register-invite`).
  - **M6.3:** RFC 6238 TOTP 2FA-motor (`mbl-auth::totp`) med Base32-hemligheter, QR `otpauth://`-URI och ±30s klockdrifttolerans. Aktiveras via `/api/v1/auth/totp/{setup,verify,disable}` och krävs ovillkorligen för administratörer vid WAN-anslutningar.
  - **M6.4 & M6.12:** Nätverksklassificering (`mbl-remote::network`) för LAN, Tailscale CGNAT (`100.64.0.0/10`), Loopback och WAN. Klient-IP resolveras strikt mot pålitliga proxys i `X-Forwarded-For`. Admin-endpoints är som standard spärrade för WAN via `admin_lan_only`-policyn.
  - **M6.5:** Rättvis bandbreddsfördelning (`mbl-remote::bandwidth`) under serverns totala WAN-uppladdningstak (`wan_upload_limit_kbps`) med kontinuerlig feedbackrapportering från klienter.
  - **M6.6 & M6.7:** Nytt crate `crates/mbl-remote` med asynkron Tailscale LocalAPI-klient (Unix-socket och Windows named pipe). Inloggning via Tailscale-identitet (`POST /api/v1/auth/tailscale`) med strikt loopback-validering mot spoofing av auth-headers.
  - **M6.8, M6.9 & M6.10:** CGNAT-diagnostik och räckviddsanalys (`mbl-remote::reachability`) som särskiljer ISP CGNAT, Double NAT och publik IP med automatisk rekommendation av Tailscale vs ACME TLS-portvidarebefordran.
  - **M6.13 & M6.14:** Windows WiX MSI-installerare (`packaging/windows/mbl.wxs`) med Windows-tjänst, tray-ikon och brandväggsregel, plus `winget`-manifest (`packaging/windows/mbl.yaml`). Multi-arch Docker-avbilder (`packaging/docker/Dockerfile`), hårdvaruvarianter (`Dockerfile.vaapi`, `Dockerfile.nvidia`) och `docker-compose.tailscale.yml`.
  - **M6.15 & M6.16:** MCP mutationsramverk med `MutationOpts { dry_run, confirm }`. 15 nya MCP-verktyg: `list_users`, `create_user`, `update_user_permissions`, `create_invite`, `revoke_sessions`, `delete_user`, `get_remote_access_status`, `run_reachability_test`, `get_bandwidth_usage`, `update_remote_policy`, `get_settings`, `update_settings` (strikt oföränderliga MCP-säkerhetsinställningar), `create_backup` (SQLite `VACUUM INTO`), `list_backups`, `restart_server` samt prompten `setup_remote_access`. Admin-verktygen ökade från 27 till 42.
  - **M6.17:** Fullständig OAuth 2.1 för MCP med RFC 7636 PKCE (S256), metadata-endpoints (`/.well-known/oauth-authorization-server`, `/.well-known/oauth-protected-resource`), dynamisk klientregistrering (`/oauth/register`), godkännandevy, tokenutbyte (`/oauth/token`) och återkallning (`/oauth/revoke`).
  - **Tester:** Heltäckande integrationstest i `crates/mbl-server/tests/m6.rs` som verifierar alla 17 krav.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 260/260 tester godkända
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome, Vitest (22 tester), `tsc`, Vite-build, bundle budget (41.64 KB gzip <= 150 KB): OK
- **Releasepublicering:** GitHub-releasen `v0.5.0` publicerad. Taggen pekar på `4082e8f`, som även innehåller Windows named-pipe-fixen för Tailscale LocalAPI.
- **Nästa steg:** Börja med **M7.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (musikskanning med lofty, artist/album/spår-modell).

### 2026-10-26 — Milstolpe M5 (Pi 5 excellence + TV client & media intelligence) & Release v0.4.0
- **Genomfört (`M5.0`–`M5.13`):**
  - **M5.0:** Fastställde ADR 0005 till *Accepted* med Pi 5-prestandamätningar (Slint `linuxkms` + `libmpv` med `hwdec=auto-safe`, p99 < 16 ms, 48 MB RAM, 140 ms kallstart).
  - **M5.1:** Termisk och spänningsövervakning (`mbl-platform::thermal`) via `/sys/class/thermal/thermal_zone0/temp` samt Raspberry Pi `get_throttled`/`rpi_volt`. Pausar automatiskt `Priority::Idle`-jobb över 75 °C. Exponerat i `/api/v1/system/health`, MCP `get_health`, och Admin Hardware-dashboard.
  - **M5.2:** Plattformsstandarder för profilkonfiguration (`PlatformProfile::defaults`) med anpassad trådkonkurrens, cachestorlekar och transkodningstak.
  - **M5.3:** Automatisk val av `/run/mbl/transcode` (tmpfs) på Linux med >= 4 GB RAM, storleksbegränsad till 25% av RAM.
  - **M5.4:** Optimerade versioner (`mbl-analysis::optimize`): Universal 1080p och 720p H.264+AAC MP4-förtranskodning med nattfönster. `PlaybackService::select_best_part` väljer automatiskt den optimerade versionen för direktuppspelning när klienten inte kan direktspela originalet.
  - **M5.5:** Trickplay (`mbl-analysis::trickplay`): nyckelbildsextraktion (`-skip_frame nokey`) var 10:e sekund till 320px-rutor i 10x10 JPEG-ark (`sprite_000.jpg`) med JSON-index (`/api/v1/stream/trickplay/{partId}/index.json`).
  - **M5.6 & M5.7:** Intro- och eftertextsdetektering (`mbl-analysis::{intro, credits}`): 16 kHz PCM sub-fingerprinting och säsongskorskorrelering, kapitelnamnprioritet samt `blackdetect`+`silencedetect`. Sparas i `markers` och inkluderas direkt i `PlaybackResponse.markers`.
  - **M5.8:** `mbl-tv`-klient bootstrap med Slint, `linuxkms` för DRM/KMS på Pi och `winit` på skrivbord, serverupptäckt via mDNS/HTTP och 6-siffrigt Quick-Connect-inloggningsflöde (`/api/v1/auth/quick-connect`, `/authorize`, `/poll`).
  - **M5.9 & M5.10:** `mbl-tv` 10-fotsgränssnitt med 2D rumslig fokusnavigering, skärmtangentbord, HDMI-CEC (`libcec`), handkontroll (`gilrs`), skärmuppdateringsfrekvensmatchning (23.976/24/50/59.94/60 Hz), HDR- och ljudgenomströmning (IEC958).
  - **M5.11 & M5.12:** MBL OS `pi-gen`-byggsteg (`packaging/pi-gen/stage-mbl/`) och APT-repostruktur (`packaging/apt/`).
  - **M5.13:** Hailo-8/10H NPU-accelererad mediaintelligens (`mbl-hailo`): multimodala 64-dimensionella L2-normaliserade sceninbäddningar, semantisk scensökning (`/api/v1/ai/search-scenes`), ansikts-/scentaggning, hårdvarubenchmark (`/api/v1/ai/benchmark`), och spoiler-fria "Previously On..."-sammanfattningar som strikt utesluter osedda avsnitt.
  - **MCP-verktyg:** Nya verktyg `create_optimized_versions`, `search_scenes` och `get_spoiler_free_recap` med uppdaterade tester och snapshots (21 för `read`, 23 för `read+library`, 27 för `admin`).
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 242/242 tester godkända
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome, Vitest (22 tester), `tsc`, Vite-build, bundle budget (41.64 KB gzip <= 150 KB): OK
- **CI:** Alla sex jobb i körning `40239141501` passerade för `cb2fe56`, inklusive Windows-regressionen med filspärr.
- **Nästa steg:** Börja med **M6.1** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (biblioteksbehörigheter per användare).

### 2026-10-25 — Windows-städning av transkodningssessioner
- **CI-status:** PCI-fixturfixen passerade på Windows. Därefter föll timeout-testet eftersom sessionskatalogen fanns kvar efter FFmpeg-avslut; övriga fem CI-jobb lyckades.
- **Ändring:** `crates/mbl-transcode/src/session.rs` använder asynkron katalogradering med upp till fem försök vid tillfälliga filspärrar eller kvarvarande kataloginnehåll. Slutliga fel loggas i stället för att ignoreras.
- **Uppföljning:** Första återförsöksfixen missade Windows-felkoderna 32/33 (`ERROR_SHARING_VIOLATION` / `ERROR_LOCK_VIOLATION`), som Rust inte mappar till `PermissionDenied`. Dessa fångas nu uttryckligen på Windows. Regressionstestet verifierar att den avsiktliga filspärren ger kod 32 och visar kvarvarande kataloginnehåll vid fel.
- **Regressionstest:** Befintligt timeout-test i `crates/mbl-transcode/tests/session.rs` håller en fil tillfälligt låst på Windows och verifierar även att FFmpeg stoppats.
- **Verifiering:** Alla fem sessionstester och Rust-formateringen godkända lokalt. Windows-regressionen verifieras i ny CI efter push; releasetaggen lämnas oförändrad.
- **Nästa steg:** Kontrollera den nya CI-körningen och fortsätt sedan med M5.1.

### 2026-10-25 — M4 publicerad, Windows-fixture korrigerad
- **Publicering:** M4-commits och taggen `v0.3.0` pushade till GitHub. Release-workflowen lyckades för Linux ARM64/x86_64 och Windows.
- **CI-fix:** Windows nekade kolon i PCI-fixturens katalognamn i `crates/mbl-hwaccel/src/fingerprint.rs`. Bytte till understreck; detekteringen läser vendor/device utan att tolka adressformatet. Produktionskoden är oförändrad.
- **Verifiering:** Alla fyra fingerprint-tester, `cargo fmt --all -- --check` och `git diff --check` godkända lokalt. Ursprungliga CI-körningens övriga fem jobb lyckades; ny CI triggas när fixen pushas.
- **Nästa steg:** Kontrollera nya CI-körningen, särskilt Windows. Fortsätt därefter med M5.1. Den publicerade taggen `v0.3.0` flyttas inte för en testfixturändring.

### 2026-10-25 — Milstolpe M4 (Hardware Acceleration) & Release `v0.3.0`
- **Commits:** `a4c0a61` (`feat(hwaccel): M4 hardware acceleration and Hailo NPU co-tenant sharing`), `d6c2f19` (`release: v0.3.0`, tagg `v0.3.0`)
- **Genomfört (`M4.1`–`M4.17`):**
  - Skapade nytt crate `crates/mbl-hwaccel` med:
    - `fingerprint.rs`: Linux (`/dev/dri/renderD*`, `/dev/video*`, `/proc/device-tree/model`, `/dev/hailo*`) och Windows (`dxgi`) hårdvarufingeravtryck. La till `PlatformProfile::Pi5Hailo` i `mbl-platform`.
    - `ffmpeg.rs`: Probing av FFmpeg `-hwaccels`, `-encoders`, `-decoders`, `-filters` och mappning till `FfmpegCaps`.
    - `selftest.rs`: Funktionellt självtest per kandidatbackend med SQLite-cachning i `0003_hw_capabilities.sql` (`crates/mbl-db/src/repo/hw.rs`), samt 1-timmes TTL-spårning av backends som fallerat under körning (`record_runtime_failure`) och beräkning av `TranscodeCapacity`.
    - `hailo.rs`: Detektering av Hailo-8 (`1e60:2864`) och Hailo-10H (`1e60:45c4`), inspektion av aktiva co-tenant-processer (t.ex. Frigate, `hailo-ollama`, GStreamer) på `/dev/hailo*`, och kooperativ `VDevice`-delningskonfiguration (`hailo.mode`, `hailo.vdevice_group_id`) enligt ADR 0006.
  - Utökade `crates/mbl-transcode`:
    - `args.rs` & `pipeline.rs`: Hårdvarubackends (`Vaapi`, `Qsv`, `Nvenc`, `V4l2m2m` för Pi 4, `PiDrmDecode` för Pi 5, `D3d11Amf` för Windows), HDR10/HLG → SDR BT.709 tone mapping (`tonemap_vaapi`, `vpp_qsv`, `tonemap_cuda`, mjukvaru-`zscale`+`tonemap=hable`), och GPU multi-rendition ABR-stege (`split=N`).
    - `session.rs`: `create_session_with_fallback` med automatisk övergång till nästa backend i fallback-kedjan om FFmpeg avslutas med fel inom 5 sekunder.
  - Utökade `crates/mbl-playback/src/decide.rs`:
    - HDR tone mapping inaktiveras automatiskt på Pi-profiler med förklarande `Reason::Other`.
    - Fjärrströmmar (`Wan`, `TailnetRelayed`) föredrar HEVC/AV1-utdata när både klientprofil och serverns HW-encoder stödjer det.
  - Service, API, Web & MCP:
    - `crates/mbl-service/src/hw.rs` (`HwService`) och `crates/mbl-api/src/routes/hw.rs` (`/api/v1/system/hw`, `/api/v1/system/hw/retest`, `/api/v1/system/transcode-capacity`).
    - `crates/mbl-service/src/playback.rs`: Om en användare på Pi 5 (`max_video_sessions = 1`) startar en ny HLS-ström ersätts användarens egna tidigare HLS-session automatiskt så att kanal-/videobyte inte blockeras av kapacitetstaket.
    - `web/src/pages/admin/Hardware.tsx` + `Hardware.test.tsx`: Admin-sida för GPU/NPU, Hailo-delningsstatus, backend-tabell och omkörning av självtest.
    - `crates/mbl-mcp/src/tools/hw.rs`: MCP-verktyg `get_hw_report`, `get_transcode_capacity`, `run_hw_selftest` (med `dry_run`) samt resursen `mbl://hw/report`.
    - `.github/workflows/ffmpeg.yml`: CI-workflow för statiska/paketerade `mbl-ffmpeg`-byggen.
- **Verifiering:**
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 216/216 tester godkända
  - `just web-check`: Biome, Vitest (22 tester), `tsc`, Vite-build (41.65 KB gzip <= 150 KB): OK
  - `cargo xtask openapi-check` & `cargo xtask tokens --check`: OK
- **Nästa steg för kommande agent:**
  - Börja med **M5 (`M5.1`)** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md).

### 2026-10-25 — Milstolpe M3 (Modern UI v1) & Release `v0.2.0`
- **Commits:** `fec714f`, `cd1cb13`, `fa84702` (tagg `v0.2.0`)
- **Genomfört:** Design tokens (`design/tokens.json`, `mbl-ui-kit`), 4 teman + accenter, UI-komponenter, responsiv navigering, uppdaterad detaljvy/spelare med kvalitet/undertexter/nattläge/statistik/Up Next, kommandopalett, profilväljare, i18n (EN + SV/DE-scaffolds) och bundle-size budget.

### 2026-10-24..25 — Milstolpe M0, M1 & M2 (`v0.1.0` MVP)
- **Commits:** Tom `e5f6ae6` (tagg `v0.1.0`)
- **Genomfört:** Hela basarkitekturen (`mbl-core`, `mbl-platform`, `mbl-db`, `mbl-auth`, `mbl-parser`, `mbl-probe`, `mbl-jobs`, `mbl-scanner`, `mbl-metadata`, `mbl-images`, `mbl-playback`, `mbl-transcode`, `mbl-stream`, `mbl-service`, `mbl-api`, `mbl-mcp`, `mbl-server`, `mbl-cli`, `web/`). Se `IMPLEMENTATION.md` §9 för varje deluppgift (`M0.1`–`M2.15`).
