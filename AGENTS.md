# AGENTS.md — Agent Handoff, Environment Guide & Work Log
This document lives at the repository root so every AI agent, contributor, and automation tool can find the current status, environment-specific rules, architectural decisions, and handoff notes quickly.

---
## 1. Current status (quick overview)
- Current work: v1.1.0 release tag pushed to GitHub; automated package and release publication is pending verification. Native M11 clients remain on hold. MCP currently exposes 75 admin tools (30 read, 35 read+library).
- Latest release tag: `v1.1.0` (pushed 2026-10-29; GitHub Actions publishes after packaging)
- Completed milestones:
  - M0: Foundations (workspace, CI, configuration, SQLite, synthetic test media)
  - M1: Library Core (scanner, parser, ffprobe, TMDB/NFO metadata, images, FTS5, WebSocket events, OpenAPI)
  - M2: Playback MVP & MCP v0 (direct play, HLS fMP4 transcoding, subtitles, watch state, MCP server with scoped API tokens and audit logging, web MVP)
  - M3: Modern UI v1 (design tokens, four themes + accents, component library, responsive desktop/mobile/TV navigation, command palette, i18n, bundle budget)
  - M4: Hardware Acceleration (`mbl-hwaccel`, platform fingerprinting, FFmpeg probing, self-test in `0003_hw_capabilities.sql`, VAAPI/QSV/NVENC/Pi V4L2/DRM/Windows D3D11/AMF, fallback chains, HDR tone mapping, ABR ladder, HEVC/AV1 remote preference, Hailo-8/10H NPU cooperative `VDevice` sharing, admin hardware UI, MCP hardware tools)
  - M5: Pi 5 excellence + TV client & media intelligence (thermal/voltage protection, platform standards, tmpfs transcoding, optimized pre-transcoded versions with automatic DirectPlay for incompatible clients, trickplay JPEG sprites, intro and credits detection with markers in `PlaybackResponse`, `mbl-tv` 10-foot client with spatial navigation/CEC/gamepad/frequency matching, OS image `pi-gen` and APT repo, Hailo-8/10H NPU multimodal scene search, tagging, and spoiler-free recaps with `hailo-ollama`)
  - M6: Users, sharing, remote access, Windows (per-user library ACLs, managed profiles with parent PIN and age ceilings, time-limited invite links, TOTP 2FA, network classification and trusted proxying, fair WAN bandwidth sharing, `mbl-remote` Tailscale LocalAPI client with spoofing protection, CGNAT diagnostics, admin LAN/tailnet guard, Windows service/tray/WiX MSI packaging, multi-arch Docker and nvidia/vaapi/tailscale-compose, MCP mutation framework with `dry_run`, `confirm`, and elicitation, 15 new MCP tools to 42 admin tools, OAuth 2.1 PKCE and dynamic client registration for MCP)
  - M7: Media breadth (music scanning with `lofty` and MusicBrainz, EBU R128 loudness analysis and progressive audio transcoding, OpenSubsonic `/rest/*` compatibility, photo EXIF and timeline, collections and playlists, metadata field locking, OpenSubtitles download, DLNA UPnP SSDP and ContentDirectory, PlayerRegistry and remote control, webhooks with HMAC-SHA256, Home Assistant MQTT discovery and telemetry, AI Watch Companion co-watching with live dialog cues and reactions, Subtitle Auto-Sync `mbl-subsync`, Scene X-Ray, spoiler-free recaps with strict watch-history filtering, Movie Night Matchmaker swipe sessions, built-in media requests with Radarr/Sonarr bridging and hardlink storage analysis, 20 new MCP tools to 62 total with 100% OpenAPI parity, and a complete WiX MSI installer for Windows)
  - M8: Distributed nodes (transcode workers and remote library storage nodes, worker protocol specification and typed `WorkerJobSpec`, HMAC-signed source range URLs, segment `PUT` uploads to session directories, Wake-on-LAN magic packets, seamless local fallback on worker failure, Windows worker tray with pause-while-gaming GPU protection, admin worker and node dashboard in SolidJS, 5 new MCP tools to 67 admin tools with 100% OpenAPI parity, `mbl-node` federated storage node with delta catalog sync, and Smart Handover for LAN direct play vs WAN relay)
  - M9: Live TV, channels and online streaming plugins (HDHomeRun UDP discovery and lineup sync, M3U and streaming XMLTV parsing with `quick-xml`, 2-axis virtual EPG grid, live playback engine with concurrency limits, DVR scheduling with series rules, margin handling and conflict resolution, 5 new MCP tools to 72 admin tools with 100% OpenAPI parity, plugin architecture for online media with YouTube, Spotify, and WebDrama support, virtual 24/7 channels with zero-CPU playhead calculations, and a full 4-tab Live TV SolidJS UI)
  - M10: Hardening → v1.0.0 (formal STRIDE threat model, hardening for webhooks/DoS/traversal, fuzzing suite with libFuzzer targets and property tests, performance and SLA report in `docs/benchmarks.md`, complete mdBook documentation site in `docs/book/`, migration tools for Plex and Jellyfin history and watch state, full 5-language translations [en, sv, de, fr, es] with WCAG 2.2 AA accessibility, separate release builds for `mbl-worker` and `mbl-node` with systemd services and standalone Windows packages, and MCP eval scenarios and runner with 74 admin tools and 100% parity)

---
## 2. Repository documents
| File | Purpose |
|---|---|
| [`AGENTS.md`](AGENTS.md) | This file. Handoff log, environment-specific rules, and quick status for agents. Update whenever work is completed. |
| [`IMPLEMENTATION.md`](IMPLEMENTATION.md) | Milestone checklist (`M0`–`M11+`) with `[x]`/`[ ]` tracking, acceptance criteria, and the detailed progress log (§9). |
| [`PLAN.md`](PLAN.md) | Full architecture and product plan. |
| [`CHANGELOG.md`](CHANGELOG.md) | User-visible changes per release (`v0.1.0`, `v0.2.0`, `v0.3.0`, `v0.4.0`, ...). |
| [`docs/adr/`](docs/adr/) | Architecture Decision Records (for example `0005-tv-ui-stack.md`, `0006-hailo-npu-resource-sharing.md`). |
| [`docs/openapi.json`](docs/openapi.json) | Generated OpenAPI 3.1 schema (verified with `cargo xtask openapi-check`). |
| [`docs/worker-protocol.md`](docs/worker-protocol.md) | Protocol specification for distributed transcode workers and federated storage nodes. |

---
## 3. Critical environment and machine rules
1. Hardware on the development machine:
  - The machine is a **Raspberry Pi 5 (ARM64, Linux)** with a **Hailo-10H NPU** (`/dev/hailo0`) where other co-tenants such as Frigate in Docker may run concurrently.
2. Limited disk space on `/` (`~15–18 GB` free):
  - Full debug symbols for all test binaries in `target/` can fill the SD card (`No space left on device`).
  - Always use the following flags when running `cargo nextest` or `cargo test`:
    ```bash
    CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 XDG_RUNTIME_DIR=/tmp CI=1 NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost cargo nextest run --workspace --no-tests=pass
    ```
  - This keeps `target/` around ~1.5 GB instead of >18 GB. If disk space becomes tight, run `cargo clean`.
3. Test workflow (avoid long recompiles after every step):
  - Because the full test suite takes time on the Pi 5, implement the full milestone or a coherent group of tasks, keep the code compiling with fast `cargo clippy`, and run the full verification pass once the milestone is ready.
4. Environment variables for frontend and tests:
  - Always set `XDG_RUNTIME_DIR=/tmp` for `just web-check` and `pnpm` commands:
    ```bash
    XDG_RUNTIME_DIR=/tmp just web-check
    ```
  - Set `NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost` around integration tests so local HTTP/MCP calls are not captured by a system proxy.
5. OpenAPI and design tokens:
  - If you change or add REST endpoints in `crates/mbl-api`:
    1. Register the path and schemas in `crates/mbl-api/src/openapi.rs`.
    2. Regenerate `docs/openapi.json`: `cargo run -p mbl-server -- openapi > docs/openapi.json`
    3. Regenerate the TypeScript types: `XDG_RUNTIME_DIR=/tmp pnpm -C web gen:api`
    4. Verify with `cargo xtask openapi-check`.
6. MCP parity and snapshots (`crates/mbl-mcp`):
  - Every new capability should either be exposed as an MCP tool under `crates/mbl-mcp/src/tools/` or documented in `crates/mbl-mcp/EXCLUSIONS.toml`.
  - When MCP tools are added, the counts in `crates/mbl-mcp/tests/tools_list.rs` and `crates/mbl-server/tests/mcp.rs`, plus snapshots in `crates/mbl-mcp/tests/snapshots/`, must be updated.

---
## 4. Release checklist at milestone completion
When a milestone is complete and the relevant checks are green:

1. Update the task checkboxes `[x]` and add a summary entry in `IMPLEMENTATION.md` (§9 Progress Log).
2. Commit the milestone implementation with `git commit -s -m "feat(...): M<N> ..."`.
3. Prepare the release commit (`release: v0.<X>.0`):
  - `Cargo.toml` (`[workspace.package] version = "0.<X>.0"`)
  - `CHANGELOG.md` (new section `## [0.<X>.0] - YYYY-MM-DD`)
  - `README.md` (update `## Status`)
  - `packaging/windows/README.txt` (update version number)
  - `.github/workflows/release.yml` (update fallback tag)
  - `docs/openapi.json` (update the `"version": "0.<X>.0"` entry)
  - Run `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo xtask openapi-check` (this also updates `Cargo.lock`).
4. Commit with `git commit -s -m "release: v0.<X>.0"` and tag it with `git tag v0.<X>.0`.
5. Update the session and handoff log below.

---
## 5. Session and handoff log
> Agent instruction: add a new section at the top of this log when a task or milestone is completed. Describe what was done, which files/crates changed, any pitfalls solved, and what the next agent should take on next.

### 2026-10-29 — Release v1.1.0 tag pushed
- Bumped workspace/package versions, release notes, README, Windows package README and OpenAPI schema to 1.1.0. The release workflow now publishes the GitHub Release after Linux/Windows artifacts build; fallback tag references are v1.1.0.
- `cargo run -p mbl-server -- openapi` and `cargo xtask openapi-check` pass. Full batch 25 verification is recorded below. Release commit `edafb80` and tag `v1.1.0` are pushed; remote `main` and tag were verified.
- The GitHub release page returned 404 in the available unauthenticated browser session, so Actions completion and attached artifacts could not be verified here. Next: confirm the workflow completed and release assets are present; M11 remains on hold.

### 2026-10-29 — Git-kontrollpunkt efter batch 25
- Användaren har godkänt checkpoint och publicering på GitHub. Batch 25 sparas ovanpå `a4efebb` med Signed-off-by; tidigare lokala checkpoints för Chromecast och batch 1–24 ingår i den samlade ändringen mot GitHub.
- Full verifiering från batch 25 återanvänds: 411/411 Rust-tester, 155 webbtester, workspace-Clippy, SQLx, cargo deny offline, binärbyggen, TypeScript/Vite, bundle-budget, fmt/diff, OpenAPI, tokens och MCP-paritet 149/149 passerar. Endast handoff- och checkpointstatus ändras inför committen.
- GitHub har en separat dokumentationsuppdatering (`ca035b7`); den ska bevaras vid publiceringen. Release/version/tagg/M11 ändras inte. Fysisk tuner/browser/Cast/WAN/Windows-acceptans och restlistans återstående funktioner kvarstår.

### 2026-10-29 — Batch 25: explicit DVR-återställning och beständig filstädning
- Ny GET/POST `/api/v1/livetv/recordings/{id}/recovery`, `RecordingRecoveryPlan` och MCP `recover_recording` med dry_run/confirm. Ägare/admin kan förhandskontrollera och återställa sparade Failed-filer efter omstart även när kanalen/sändningen är borta. Nuvarande ägare/bibliotek/rating och eventuell kvarvarande guide kontrolleras före och efter probing; delade tittare nekas.
- Återställning kopierar en storleksbegränsad prefixbild till en ägd tempfile och publicerar den utan överskrivning som `<id>.recovered.ts`. Spelaren kan inte påverkas av en gammal capture-skribent som fortsätter skriva `<id>.ts`. Bytebudget/reserv kontrolleras före kopiering; 60 s kopieringstimeout, 15 s ffprobe, RAII-städning vid avbrott. Återställt material blir alltid Partial med befintlig ägare/regel/retention. Originalet behålls till uttrycklig radering eller vald retention; kopian behöver extra utrymme. Ingen automatisk återställning av Failed-filer.
- Delete validerar hanterade filer och sparar raderingstombstone/katalogradering innan unlink. Misslyckad unlink försöks igen vid start och underhåll. Raderade ID:n städas i nyckelsidor om 200 med roterande cursor. Tombstone får prunas först efter lyckad filstädning och minst ett dygn efter både program- och capture-slut. Endast ID:ts raw/snapshot och egna scratchfiler omfattas; andra filer, symlänkar och kataloger raderas inte rekursivt. Ingen ny migration behövs.
- Webben skiljer Recover saved media från Retry av en sändning, även efter kanal-/programslut. Förhandskontroll, bekräftelse, dubblettskydd, avbrott vid avmontering och timeout finns. Saknad/tom/oläsbar fil ger ingen ny spelbar post. MCP är nu 75 adminverktyg (30 read, 35 read+library); snapshot och tester uppdaterade.
- Full verifiering på ARM64/Linux: cargo nextest --workspace passerar 411/411 tester, 0 skipped (två testtrådar och resurssnåla profilflaggor). Hela webbsviten passerar 155 tester; TypeScript/Vite och initial JS-budget 42,24 KB gzip passerar. De fem nya Rust-regressionerna täcker fryst/spelbar kopia, samtidiga REST/MCP-försök, saknad/tom/trasig fil/låg disk, beständig raderingsstädning och profiländring under probe. Tre nya webbfall täcker bekräftelse, saknad fil och sena svar/avbrott. Hela arbetsytans Clippy med alla targets och -D warnings, SQLx prepare --check mot en separat temporär databas med alla migrationer, cargo deny offline (advisories/bans/licenses/sources), debugbyggen av alla workspace-binärer, fmt/diff-check, OpenAPI-check, tokens-check och MCP-paritet 149/149 passerar (75 verktyg, 52 undantagna paths). Doctestkommandot passerar; projektet definierar 0 doctester. Biome har 0 fel och tidigare 5 varningar/4 infos. OpenAPI/TypeScript är regenererade. Fysisk och plattformsöverskridande releaseacceptans ingick inte.
- Bas är checkpoint `a4efebb`. `cargo clean` kördes enligt användaren före batchen (3,4 GiB borttaget). M11/release avvaktar; Git-checkpoint har nu godkänts av användaren. Helarkivskvot/prognoser, bredare transport-/undertextprofiler och fysisk tuner/browser/Cast/WAN/Windows-acceptans återstår.

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

### 2026-10-26 — Milestone M10 (Hardening → v1.0.0) & Release v1.0.0

- Completed (`M10.1`–`M10.8`):
  - M10.1: Formal STRIDE threat model in `docs/threat-model.md` and security hardening in `crates/mbl-service` for webhook validation, DoS caps, and path traversal prevention.
  - M10.2: Fuzzing suite in `fuzz/` for metadata, M3U/XMLTV, and HLS/range handling; fixed a panic in `parse_xmltv_date` for arbitrary multibyte UTF-8 strings.
  - M10.3: Benchmark report in `docs/benchmarks.md` covering scan throughput, TTFF, memory footprint, and zero-CPU virtual playhead metrics.
  - M10.4: Complete mdBook documentation site in `docs/book/` covering installation, hardware acceleration, Tailscale remote access, Live TV/DVR, MCP, AI companion, migration, and troubleshooting.
  - M10.5: `ImportService` for Plex and Jellyfin history/watch-state migration plus REST and MCP endpoints for import workflows.
  - M10.6: Full i18n coverage in the web client with English, Swedish, German, French, and Spanish translations and WCAG 2.2 AA accessibility.
  - M10.7: Separate release builds for `mbl-worker` and `mbl-node` with distribution packages and documentation updates.
  - M10.8: MCP evaluation scenarios and runner for AI/admin workflows; admin tool count reaches 74 with 100% OpenAPI parity.

- Verification:
  - `cargo fmt --all -- --check`: OK
  - `cargo clippy --workspace --all-targets -- -D warnings`: OK
  - `cargo nextest run --workspace`: 311/311 tests passed (0 failed, 0 skipped)
  - `cargo xtask mcp-parity`: OK (141/141 endpoints covered, 100.0%)
  - `cargo xtask openapi-check`: OK
  - `cargo xtask tokens --check`: OK
  - `just web-check`: Biome (0 errors), Vitest (22 tests), `tsc -b`, Vite build: OK

- Next step: begin **M11.1** in [`IMPLEMENTATION.md`](IMPLEMENTATION.md) (native Android mobile & tablet app with Compose and Media3/ExoPlayer).

### 2026-10-25 — Milestone M4 (Hardware Acceleration) & Release `v0.3.0`

- Completed: hardware acceleration stack in `crates/mbl-hwaccel`, FFmpeg capability probing, Hailo NPU co-tenant sharing, multi-backend fallback logic, and admin hardware diagnostics.
- Verification: Rust formatting, clippy, workspace tests, web checks, and OpenAPI validation all passed.

### 2026-10-25 — Milestone M3 (Modern UI v1) & Release `v0.2.0`

- Completed: design tokens, four themes, responsive UI, command palette, player detail view, i18n scaffolding, and bundle-size budget enforcement.

### 2026-10-24..25 — Milestones M0, M1 & M2 (`v0.1.0` MVP)

- Completed: base architecture and MVP media stack across `mbl-core`, `mbl-platform`, `mbl-db`, `mbl-auth`, `mbl-parser`, `mbl-probe`, `mbl-jobs`, `mbl-scanner`, `mbl-metadata`, `mbl-images`, `mbl-playback`, `mbl-transcode`, `mbl-stream`, `mbl-service`, `mbl-api`, `mbl-mcp`, `mbl-server`, `mbl-cli`, and `web/`.
