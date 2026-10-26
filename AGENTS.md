# AGENTS.md — Agent Handoff, Environment Guide & Work Log

Detta dokument ligger i roten av repot så att alla AI-agenter (Antigravity, Claude Code, Cursor, Codex m.fl.) och utvecklare direkt hittar aktuell status, miljöspecifika regler, arkitekturbeslut och handoff-logg.

---

## 1. Aktuell Status (Snabböverblick)

- **Senaste release:** `v0.5.0` (byggd och verifierad lokalt, M6 färdigställd)
- **Avklarade milstolpar:**
  - **M0:** Foundations (Workspace, CI, config, SQLite, syntetisk testmedia)
  - **M1:** Library Core (Scanner, parser, ffprobe, TMDB/NFO-metadata, bilder, FTS5, WebSocket-events, OpenAPI)
  - **M2:** Playback MVP & MCP v0 (`v0.1.0` — Direct play, HLS fMP4-transkodning, undertexter, watch state, MCP-server med scopade API-tokens och audit-logg, Web MVP)
  - **M3:** Modern UI v1 (`v0.2.0` — Design tokens, 4 teman + accenter, komponentbibliotek, responsiv desktop/mobil/TV-navigering, kommandopalett, i18n, bundle-budget)
  - **M4:** Hardware Acceleration (`v0.3.0` — `mbl-hwaccel`, plattformsfingeravtryck, FFmpeg-probing, självtest i `0003_hw_capabilities.sql`, VAAPI/QSV/NVENC/Pi V4L2/DRM/Windows D3D11/AMF, fallback-kedjor, HDR tone mapping, ABR-stege, HEVC/AV1 fjärrpreferens, Hailo-8/10H NPU kooperativ `VDevice`-delning, Admin Hardware UI, MCP HW-verktyg)
  - **M5:** Pi 5 excellence + TV client & media intelligence (`v0.4.0` — Termisk/spänningsvakt, profilstandarder, tmpfs-transkodning, förtranskodade optimerade versioner med automatisk DirectPlay för inkompatibla klienter, trickplay-skrubb JPEG-ark, intro- och eftertextsdetektering med markörer i PlaybackResponse, `mbl-tv` 10-fotsklient med rumslig navigering/CEC/gamepad/uppdateringsfrekvensmatchning, OS-avbild `pi-gen` & APT-repo, Hailo-8/10H NPU multimodal scensökning, taggning och spoiler-fria sammanfattningar med `hailo-ollama`)
  - **M6:** Users, sharing, remote access, Windows (`v0.5.0` — Biblioteks-ACL per användare, hanterade profiler med föräldra-PIN och åldersgränser, tidsbegränsade inbjudningslänkar, TOTP 2FA, nätverksklassificering och trusted proxy, rättvis bandbreddsdelning på WAN, `mbl-remote` Tailscale LocalAPI-klient med spoofing-skydd, CGNAT-diagnostik, admin LAN/tailnet-vakt, Windows-tjänst/tray/WiX MSI-paket, Docker multi-arch och nvidia/vaapi/tailscale-compose, MCP mutationsramverk med `dry_run` och `confirm`/elicitering, 15 nya MCP-verktyg till 42 admin-verktyg, OAuth 2.1 PKCE och dynamisk klientregistrering för MCP)
- **Nästa milstolpe:** **M7 — Media breadth** (`M7.1`–`M7.20` i [`IMPLEMENTATION.md`](IMPLEMENTATION.md)); börja med M7.1.

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
   - När nya MCP-verktyg läggs till måste verktygsräkningen i `crates/mbl-mcp/tests/tools_list.rs` och `crates/mbl-server/tests/mcp.rs` samt snapshots i `crates/mbl-mcp/tests/snapshots/` uppdateras (just nu: `21` för `read`, `23` för `read+library`, `42` för `admin`).

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

### 2026-10-26 — Milstolpe M6 (Users, sharing, remote access, Windows) & Release v0.5.0
- **Genomfört (`M6.1`–`M6.17`):**
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
