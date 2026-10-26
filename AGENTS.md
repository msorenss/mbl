# AGENTS.md — Agent Handoff, Environment Guide & Work Log

Detta dokument ligger i roten av repot så att alla AI-agenter (Antigravity, Claude Code, Cursor, Codex m.fl.) och utvecklare direkt hittar aktuell status, miljöspecifika regler, arkitekturbeslut och handoff-logg.

---

## 1. Aktuell Status (Snabböverblick)

- **Senaste release:** `v0.4.0` (byggd och verifierad lokalt, M5 färdigställd)
- **Avklarade milstolpar:**
  - **M0:** Foundations (Workspace, CI, config, SQLite, syntetisk testmedia)
  - **M1:** Library Core (Scanner, parser, ffprobe, TMDB/NFO-metadata, bilder, FTS5, WebSocket-events, OpenAPI)
  - **M2:** Playback MVP & MCP v0 (`v0.1.0` — Direct play, HLS fMP4-transkodning, undertexter, watch state, MCP-server med scopade API-tokens och audit-logg, Web MVP)
  - **M3:** Modern UI v1 (`v0.2.0` — Design tokens, 4 teman + accenter, komponentbibliotek, responsiv desktop/mobil/TV-navigering, kommandopalett, i18n, bundle-budget)
  - **M4:** Hardware Acceleration (`v0.3.0` — `mbl-hwaccel`, plattformsfingeravtryck, FFmpeg-probing, självtest i `0003_hw_capabilities.sql`, VAAPI/QSV/NVENC/Pi V4L2/DRM/Windows D3D11/AMF, fallback-kedjor, HDR tone mapping, ABR-stege, HEVC/AV1 fjärrpreferens, Hailo-8/10H NPU kooperativ `VDevice`-delning, Admin Hardware UI, MCP HW-verktyg)
  - **M5:** Pi 5 excellence + TV client & media intelligence (`v0.4.0` — Termisk/spänningsvakt, profilstandarder, tmpfs-transkodning, förtranskodade optimerade versioner med automatisk DirectPlay för inkompatibla klienter, trickplay-skrubb JPEG-ark, intro- och eftertextsdetektering med markörer i PlaybackResponse, `mbl-tv` 10-fotsklient med rumslig navigering/CEC/gamepad/uppdateringsfrekvensmatchning, OS-avbild `pi-gen` & APT-repo, Hailo-8/10H NPU multimodal scensökning, taggning och spoiler-fria sammanfattningar med `hailo-ollama`)
- **Pågående arbete (Aktiv agent):** **M6 — Users, sharing, remote access, Windows** (`M6.1`–`M6.17` i [`IMPLEMENTATION.md`](IMPLEMENTATION.md)).

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
   - När nya MCP-verktyg läggs till måste verktygsräkningen i `crates/mbl-mcp/tests/tools_list.rs` och `crates/mbl-server/tests/mcp.rs` samt snapshots i `crates/mbl-mcp/tests/snapshots/` uppdateras (just nu: `21` för `read`, `23` för `read+library`, `27` för `admin`).

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
- **Nästa steg för kommande agent:**
  - Påbörja **M6 (M6.1)** i [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (Användare, behörigheter, delning och fjärråtkomst).
- **Verifierad CI:** Alla sex jobb i körning `40239141501` passerade för `cb2fe56`, inklusive Windows-regressionen med filspärr.
- **Releasekontroll:** GitHub-releasen var fortfarande ett opublicerat utkast. Båda Debian-paketen saknade serverbinären och innehöll bara systemd-fil och licensinformation.
- **Korrigering:** `crates/mbl-server/Cargo.toml` inkluderar nu `usr/bin/mbl-server` med läge `755`. Release-workflowen verifierar binärens närvaro i både amd64- och ARM64-paketen. `CHANGELOG.md` beskriver paketfixen och Windows-städningen.
- **Godkänt av användaren:** Flytta den befintliga `v0.3.0`-taggen till korrigerad releasecommit och bygg om utkastets artefakter innan publicering, i stället för att skapa `v0.3.1`. Detta ersätter tidigare beslut nedan att lämna taggen oförändrad.
- **Slutresultat:** `v0.3.0` publicerad som senaste GitHub-release från `3ad44cb`. Alla sex CI-jobb i `40240087820` och samtliga releasejobb i `40240088240` lyckades. SHA-256 verifierad för alla tre paket; Debian-binärernas ELF-arkitektur och körbehörighet samt Windows-binärens PE-arkitektur och README kontrollerade.
- **Nästa steg:** Fortsätt med M5.1.

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
