# Changelog

All notable changes to this project are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Edit the menu: choose **Edit menu** in the user menu to hide entries and change their order within each group (drag an entry, or use the arrows). The layout is saved with your account, so every device shows the same menu; **Reset to default** brings back the standard menu. Home is always shown.
- **Open in Radarr/Sonarr/Lidarr** for administrators: on requests and on movie and show pages. It opens the title in the download service when the service has it, otherwise the service's start page, and appears only for services with an address and an API key. New `GET /api/v1/requests/arr-links`, `GET /api/v1/requests/{id}/arr-link` and `GET /api/v1/items/{id}/arr-link`.
- Guests at movie night: the host chooses **Invite guests** and shares a link; anyone can join with just a name, without an account, and swipe with the others. A guest's votes are bound to their browser, not to the name they type. Guests see only what the host's profile may see. **Turn off guest link** removes the guests and their votes.
- Plan a movie night: choose a time and who to invite. Invited people are members right away and get a notice in MBL (and a system notification when the browser allows it). Voting closes at that time and the most liked title wins; everyone gets the result. New webhook events `movienight.scheduled` and `movienight.decided`.
- Screenshots to the AI: in the player chat, choose how often a picture of the scene may go to the AI (off by default). The built-in companion sees the scene with your question, and the assistant chat can look at your screen when you ask what is on it; it is never a picture from later than where you are. Images reach the model as images for Claude and OpenAI-compatible providers. New MCP tool `get_companion_frame` (82 tools) and `GET /api/v1/companion/sessions/{id}/frame`.
- Find subtitles: with an OpenSubtitles API key under Server settings (an account allows more downloads per day), **Find subtitles** on movie and episode pages and in the player's subtitle menu searches by language and downloads the subtitle next to the video as `<name>.<language>.srt`. It becomes a track at once, and in the player it is selected right away.
- What's missing: a show page lists the aired episodes you lack compared with TMDB (gaps, later episodes and whole seasons, with titles; not specials or unaired episodes), and a TV or movie library page lists shows with missing episodes and released movies from collections you have part of. Each can be requested, which goes on to Sonarr/Radarr when connected. New `GET /api/v1/items/{id}/missing` and `GET /api/v1/libraries/{id}/missing`.
- Database maintenance under Server settings: at a chosen hour the server optimizes the database, vacuums when much of it is unused, keeps a number of automatic backups and removes old finished jobs. Titles whose files have been gone for a chosen number of days can be removed automatically or after a preview, together with empty seasons and shows; libraries whose folders cannot be read are left alone. New MCP tool `run_maintenance` (83 tools).
- Talk to the chats: **Talk** in the player chat and the assistant chat turns speech into text, on the server with Whisper on a Hailo-10H when a model is installed, otherwise with the browser's recognition. **Read replies aloud** speaks the answers, with the film turned down meanwhile. New `GET /api/v1/speech/status` and `POST /api/v1/speech/transcribe`.
- The companion knows what is on screen without a picture leaving the server: the live context has a scene analysis of the frame at your position, with objects detected by a Hailo NPU (and a sentence when a vision-language model is installed), kept apart from rough colour guesses. Verified on a Hailo-10H with YOLOv11s next to Frigate; not on a Hailo-8.
- With a vision-language model on a Hailo-10H (`Qwen2-VL-2B-Instruct.hef` in `/var/lib/mbl/models`) the scene analysis includes a sentence or two about who is in the picture and what is happening. The model now stays loaded between frames (6–11 s per frame instead of about 100 s).
- **AI models for the NPU** on the Hardware page: shows the NPU, the HailoRT version and the models MBL uses (Qwen2-VL for scene descriptions, Whisper for speech), downloads the build matching the installed HailoRT from Hailo's server with progress, checks it and shows which model is in use. New guide [Setting up a Hailo NPU](docs/book/src/hailo-setup.md).
- Raspberry Pi: transcoding segments now go to memory (`/run/mbl/transcode`) instead of the SD card when the machine has at least 4 GB of RAM. The rule existed but was never applied, so every transcode wrote several MB/s to the card. The packaged service unit gets `RuntimeDirectory=mbl`.
- Settings are kept in memory and database connections stay open, so a busy SD card (for example one Frigate records to) no longer holds requests up for seconds on a settings read or a reopened connection.
- Speech recognition no longer returns text Whisper invents over music or silence (fragments like "nd", stray numbers, a row of accents at the end).
- **Let MBL use the NPU** switch and live activity on the Hardware page: what MBL has loaded, how many frames, scene descriptions and transcriptions it did since start, and the last problem. MBL now connects to the board only when a model is needed, shares one connection per model across the server and lets go after 2 minutes (detection) or 15 minutes (scene description) without use. The scene description model loads when a companion joins, not mid-conversation, since loading it slows Frigate for about 40 seconds, so Frigate and other programs get the board's connections back. Before, every part of MBL that could use the NPU opened its own connection at startup and kept it.

### Changed
- **Play** and **Cast to TV** work on a show or season page: they start the episode after the last one you finished (continuing where you stopped if you started it), a started episode if you have not finished any, or the first episode. The button shows which one, for example "Resume S2E9", and playback continues with the next episode. New `GET /api/v1/items/{id}/next-episode`.

### Fixed
- Sound out of sync after resuming in the middle of a file (not from the start). When the video is copied it can only start at the keyframe before the saved position, while the sound started exactly at the position; players that line up the first frames then ran the sound ahead by up to a few seconds. The sound now starts at the same keyframe.
- The MCP tool `create_backup` saved backups in a folder relative to the server's working directory, which the installed service may not write to; backups now go next to the database in `backups/`.
- The chat button in the player was invisible on computers without an emoji font (common on Linux, including Raspberry Pi OS): it was a 💬 emoji. It now has its own icon and the label **Chat**, and so does the assistant button in the top bar. The subtitle button no longer shows an emoji either.

## [1.3.10] - 2026-11-20

### Added
- Built-in AI assistant. An administrator chooses a language model under **Server settings → AI assistant**: Claude (Anthropic), OpenAI, Google Gemini, OpenRouter, Groq, Mistral, DeepSeek, any OpenAI-compatible server, or a local Ollama, LM Studio or Hailo-Ollama. The settings fetch the provider's models and test the connection, including whether the model can use tools. The API key is never shown again after saving. The assistant uses MBL's own tools as the person asking, so library access, age limits and spoiler protection apply. What it may do is set per role: for administrators and for other users, from reading only up to everything an admin can do (other users can never give it admin, user or remote access, and it never gets more than the person's own rights). It asks in the chat before it changes anything. With playback allowed, "play …" starts the title in your browser.
- Agent programs as the assistant: instead of a model, **Claude Code** or another agent program can answer. MBL runs the command line (without a shell) with the question on standard input and gives it a temporary read-only token for MBL's MCP server, which is revoked after each answer.
- Assistant chat on every page (💬 in the top bar): ask what to watch next, or what else there is with an actor. Answers stream in, conversations are kept per user, and titles from the library that an answer names get **View** and **Play**. Type a question in the command palette (Ctrl+K) and choose **Ask the assistant** to open the chat with it.
- The watch companion answers by itself: with the assistant turned on, MBL joins the player chat the first time you write or press **Comment now**, answers with the scene so far as context (title, position, chapter and dialogue up to that point), never comments past your position, and adds unprompted comments at the frequency you choose.
- Player chat: write to the companions, choose **Comments on**, **Replies only** or **Comments off**, how often they comment on their own (low, medium, high), **Comment now**, and quick questions about the scene (who is on screen, what just happened, the story so far). The chat button sits in the player's control bar and the chat shows the newest message.
- For external agents: companions get the subtitle line on screen and the recent dialogue in the live context, read the chat with `read_companion_chat`, and are woken as soon as something happens with `wait_for_companion_events` (REST: `GET /api/v1/companion/sessions/{id}/wait`). New MCP tool `find_by_person` finds titles with an actor, director or writer. There are now 81 MCP tools.

### Changed
- The first download of the web app is about 40 % smaller (149 KB → 107 KB gzip): translations for the less common languages load when one of them is chosen.

### Fixed
- MCP tools now pass on conflicts with their reason (for example that comments are turned off) instead of "internal server error", and posting a companion comment before joining the session says to join first.
- The Windows worker build and a flaky worker test in CI.
- Remote control of a browser tab (and the Cast command channel) failed with 404 when the browser's session cookie belonged to another sign-in than the page, for example after signing in on both the HTTP and the HTTPS port: cookies are shared between ports, stored sign-ins are not. The page now sends its own token when it opens the command channel.

## [1.3.9] - 2026-11-19

### Added
- Transcode workers now take real work: `mbl-worker run` proves its encoders with an FFmpeg self-test, long-polls the server and encodes HLS sessions with its own FFmpeg, uploading output as it is produced. Sessions on a worker do not use a local encoder slot. If the worker fails, is revoked or stops responding for 15 seconds, the server continues locally from the next segment.

- Storage nodes now share real folders: `mbl-node library add` shares a folder, and `mbl-node run` lists it, probes new files with its own FFprobe and serves them through URLs the server signs. Each folder becomes an ordinary MBL library; direct play is relayed by the server, transcoding reads the node file, and LAN clients can get a signed direct URL from the handover endpoint. Works on Windows and Linux.

- Remote access diagnostics check the real path from the internet: public address, the router's WAN address over UPnP, DNS of the public host names, the forwarded HTTPS port (and port 80 for Let's Encrypt), the certificate and Tailscale. The admin page shows each check with an explanation instead of raw JSON, and the tailnet address of the server.
- `mbl-cli` is included in the Debian package, the Linux tarballs, the Windows ZIP and the MSI, so the stdio configurations generated for Claude Desktop and other AI agents work after installation.

- Quick Connect approval page (**Sign in a device**, `/connect`, also `/connect?code=123456`): enter the 6-digit code a TV or app shows, see which device it is, and approve it. New `POST /api/v1/auth/quick-connect/lookup` shows the waiting device before approval.
- Profile switching: managed profiles are entered with their PIN (or directly without one) through `POST /api/v1/auth/switch-profile`. The previous session on the device ends, so returning to a full account needs its password. The profile list now shows which profiles have a PIN.
- Intro and credits markers are found automatically: after a scan, an idle background job analyses new episodes (chapters, else audio fingerprints shared within the season) and movies (chapters, else black frames with silence). Each file is analysed once; the job yields to playback.
- Webhooks are delivered: `playback.started`, `playback.stopped`, `media.added`, `library.scan_completed` and `library.unavailable`, signed with `X-MBL-Signature` when a secret is set, with one retry. Administration → Server settings has a webhook section with a Test button (`POST /api/v1/webhooks/{id}/test`).
- Radarr and Sonarr: the Requests page has a section where an administrator enters address and API key, tests the connection and picks root folder and quality profile. Approving a request then adds the movie to Radarr (looked up by TMDB ID) or the show to Sonarr (looked up through `tmdb:` so Sonarr gets its TVDB ID) and starts a search. A title already there is not added twice. If the service refuses, the request stays pending and the reason is shown. Requests become *available* once the title appears in a library. Requests can also be declined. New routes `GET/PUT /api/v1/requests/arr/{service}` and `POST /api/v1/requests/arr/{service}/test`; the API key is never returned.
- Requests approved while their download service was off can be sent later with *Send to Radarr/Sonarr/Lidarr*; the page says when nothing was sent. Diagnostic logs name the service and the error code when a connection test, save or add fails.
- DLNA media server: smart TVs on the home network find MBL over SSDP and browse the libraries an admin chooses (movies, show → season → episode, artist → album → track) with an optional maximum content rating. Files are sent as they are with byte-range seeking; Samsung gets its Matroska MIME type. Only local-network peers are answered, and DLNA is now off by default. Settings: *DLNA for TVs* in server settings (`dlnaLibraryIds`, `dlnaMaxRating`). No transcoding, cover art or subtitles for DLNA yet.
- Lidarr and music requests: requests can be for an artist or an album. The request form searches music through Lidarr (which looks up MusicBrainz), and approving adds the artist with all its albums, or only the chosen album, to Lidarr and starts a search. Lidarr uses API v1 and also needs a metadata profile, chosen after Test connection. Music requests become available when the artist (or the album under that artist) appears in a music library. New route `GET /api/v1/requests/music/search`; requests carry `musicbrainzId` and `artist`; the MCP tool `create_media_request` accepts artists and albums.
- Metadata editing for administrators: **Edit metadata** on an item page changes title, sort title, original title, year, content rating, studio, genres, tagline and summary. Changed fields are locked, so refreshing metadata keeps them; locks can be cleared one by one. Movies and shows can be matched to another TMDB entry (search, then *Use this*), and a poster or backdrop can be chosen from TMDB's images. New routes `GET/PATCH /api/v1/items/{id}/metadata`, `POST /api/v1/items/{id}/match` and `GET/PUT /api/v1/items/{id}/artwork`. Locks also cover genres and chosen artwork during automatic matching.
- Lyrics in the player: music tracks with a `.lrc` file or embedded lyrics show them under the title; synced lyrics follow playback and keep the current line in view.
- Translations cover every visible text in all 22 languages, including messages with names or numbers and the server's remote-access explanations. A test fails when a new English text has no translation.
- Remote control (**Remote control**, `/remote`): lists the devices playing for your account (browsers, Chromecast and TV apps) and controls them with play, pause, skip back and forward, stop and volume. A browser playing MBL now registers itself, so a phone can control the computer. **Play on device** on an item page starts the title on one of those devices.
- Watch companions in the player: reactions that an AI companion posts during playback appear on screen for a few seconds. The live context for companions now includes the current chapter. New `GET /api/v1/companion/sessions/{id}/reactions`.
- Music queue: **Play all** and **Shuffle** on album and artist pages, **Add to queue** on tracks, and playing a track continues through the album. The audio view shows the track position, Previous and Next, and what comes next.
- Movie night (`/matchmaker`): choose a movie or TV library, send the link, and everyone swipes Like or Pass on their own account. When everyone likes the same title, it is shown as the match. The session shows how many have joined (`memberCount`).
- Scene search (`/discover`): describe a scene and start playback right there. Administrators can index a whole library in the background (`POST /api/v1/libraries/{id}/index-scenes`, pauses during playback) and test the AI speed.
- **Previously on…** on show and season pages: a recap built only from episodes you have watched.
- **Check for missing episodes** on show pages, and a storage check for administrators on the Requests page that shows how much space deleting library files would really free when Radarr or Sonarr hardlinked them.
- Live TV channels can be edited by administrators: name, number, guide channel ID, favourite, hidden, or deleted.

### Changed
- Worker pairing reports the encoders that passed the self-test instead of a fixed NVENC list. The worker configuration defaults to `%ProgramData%\MBL\worker.json` on Windows.

- Storage nodes paired with earlier versions only reported placeholder libraries; pair them again. The node configuration defaults to `%ProgramData%\MBL\node.json` on Windows and the Windows node package now includes FFmpeg.

### Fixed
- Text in other languages was sometimes half translated ("Could not Belastning library"), because missing phrases were translated word by word. Word-by-word translation is now only used when every word is known; otherwise the English text is shown until a translation exists.
- Profile switching with a PIN also works for ordinary (non-managed) accounts that an administrator has given a PIN. Before, such a profile went to the sign-in page and never asked for the PIN. Administrator accounts still need their password.
- The MCP `create_webhook` tool suggested the event name `library.scan_finished`, which does not exist.
- The `mbl` Debian package no longer contains `mbl-worker` and `mbl-node`. They are in their own packages, and installing one of them next to `mbl` failed because both packages owned the same files.
- OAuth metadata for MCP clients contained relative URLs, which clients reject. Issuer and endpoints are now absolute, built from `server.public_url` or the request address.
- `/mcp` answers requests without valid credentials with `401` and a `WWW-Authenticate` header pointing to the protected-resource metadata, so MCP clients can start OAuth sign-in. Before, a request without credentials got `403 invalid_origin`.
- The earlier reachability test treated the server's Tailscale address as the router's WAN address and always reported CGNAT when Tailscale was running.
- MCP connections use the same network classification as the web API, including the home network reached through the public host name.
- The MCP tool `get_online_history` reports why it failed (for example that WebTV is turned off) instead of a generic error.
- The OpenAPI schema described player command fields in snake_case (`position_ms`) although the server expects camelCase (`positionMs`).
- `GET /api/v1/ai/search-scenes` was documented with its `q` and `limit` parameters in the path instead of the query.
- The storage analysis reported either 0 bytes or everything as freed. It now counts only files that have no other hardlink.
- An empty guide channel ID saved an empty string instead of unlinking the channel from the guide; empty channel names and numbers are refused.

### Security
- Quick Connect hands out the session token once and only after the code is claimed, so a used code never creates a second session. Device labels from the unauthenticated request are shortened and cleaned, and at most 50 codes can wait at once.
- Profile PIN checks (`/users/{id}/pin/verify` and profile switching) are limited to 5 attempts, then 10 per hour per profile.
- Webhook secrets are no longer returned by the API after creation.
- The node handover endpoint now requires sign-in, checks library access and uses the server's network classification instead of a client-supplied `X-Forwarded-For` header.
- Signed worker source URLs use a random key per server start instead of a fixed built-in key, which let anyone forge a token and read media files without signing in.
- The session-wide `PUT /api/v1/stream/hls/{sessionId}/segments/{filename}` upload is removed. Workers can only upload `init.mp4`, fragments and the playlist of a job assigned to them.

### Validation and limitations
- Checked on a Raspberry Pi 5: workspace tests, Clippy, web tests, OpenAPI and MCP parity. Radarr and Sonarr were tried against real services; profile switching, translations and remote diagnostics in the browser.
- Not yet tried on real hardware: the DLNA server on Samsung and LG TVs, Lidarr, worker offload on a Windows NVIDIA machine, and storage nodes on Windows.
- DLNA sends files as they are (no transcoding, cover art or subtitles). Metadata editing covers movies, shows and their fields; music metadata comes from tags.
- Workers and storage nodes paired with an earlier version must be paired again.
- Verified with the full Rust and web test suites on the Raspberry Pi 5. Remote control, movie night, scene search and the music queue are tested against the API, not on physical TVs, phones or Chromecasts.
- A browser tab that is not playing cannot be started remotely, and a browser may block sound until someone clicks in it. Companion reactions are kept in memory only. Scene search without a Hailo NPU uses the slower CPU fallback.

## [1.3.8] - 2026-11-17

### Added
- Built-in Let's Encrypt certificates (ACME HTTP-01) with automatic renewal, automatic renewal of the local-CA server certificate, and a hardened HTTPS listener that keeps serving through settings errors and reports why it could not start.
- The MBL Cast receiver is the default Cast application and is hosted from the project's GitHub Pages site. The Google standard receiver remains selectable per browser under Server Settings.
- Subtitle appearance (size, font, colour, background) in the player menu, applied in the browser and on Cast receivers, including live changes while casting.
- Quality, audio track and subtitle can be chosen on the item page before playback starts, and "Cast to TV" starts directly on the receiver without a local transcode first.
- Mini player: the back button keeps playback running while you browse MBL.
- The sign-in page tells an account holder when two-factor authentication is enabled or the code was rejected, and explains when an administrator must enable it. Nothing is revealed before the password is correct.

### Changed
- Clients on the home network that open MBL through its public hostname are classified as LAN. The server resolves its own public address from the certificate hostname and public URL. MCP connections keep their existing classification.
- Session cookies carry `Secure` on HTTPS connections; the database and backups are owner-only on Unix.

### Fixed
- Remote playback on Raspberry Pi 5 no longer fails with a validation error: the Pi decode backend only offers H.264 encoding.
- A Cast reload after a subtitle or quality change no longer pauses when the old stream goes idle, and a second press on play during Cast start-up no longer pauses the receiver.
- The Remote Access page no longer crashes on an error response and explains the LAN/tailnet restriction.
- Let's Encrypt failures show the certificate authority's reported cause.

### Validation and limitations
- Let's Encrypt issuance and the custom receiver are confirmed on a Raspberry Pi 5 with a Nest Hub. Subtitle styling on receivers, the home-network classification behind a real router and the sign-in notices have automated coverage only.
- The default receiver needs the Cast application to be published and its receiver URL to point at the GitHub Pages site; until then only registered test devices can start it.
- Subtitle font choices on Cast are limited to the receiver's generic families.

## [1.3.7] - 2026-11-14

### Added
- Server-owned Cast device profiles with a conservative 720p30 fallback for unknown receivers. Normal playback does not require a manual device-model choice.
- Optional Custom Web Receiver with capability probing and bounded diagnostics. A registered Cast application ID and receiver hosting are required to use this mode; the standard receiver remains available.

### Fixed
- Cast encoding normalizes rotation and non-square pixels and keeps output inside the receiver's resolution and frame-rate limits. HLS master metadata comes from the actual encoded initialization segment.
- Cast handoff retries once after releasing a local encoder on an explicit capacity error, preserving playback position when recovery is needed.
- Receiver loading has bounded timeouts, and local playback is silenced during handoff.
- Updated rustix to 1.1.6 for the dependency security advisory.

### Changed
- Device-specific Cast profiles can offer two bounded HLS quality levels when a recent matching hardware self-test demonstrates encoding headroom. Pi/ARM retains one level.
- Playback statistics report the server's actual quality-level count. Each encoded level consumes an encoder slot, and failed adaptive startup retries with one level.

### Validation and limitations
- Bedroom display (Google Hub) has physically confirmed picture/audio with Auto using the safe single-level profile, plus subtitle visibility and perceived audio/video sync.
- Two-level adaptive playback has automated coverage for geometry, segment alignment, resume and encoder admission. Physical ABR/WAN acceptance and sustained-load testing remain outstanding.
- The optional Custom Web Receiver has not yet been physically accepted; registration is still pending.

## [1.3.6] - 2026-11-10

### Fixed
- Chromecast remux playback uses the segment durations produced by FFmpeg and keeps published segments available for the life of the session.
- The Cast seek slider prepares playback at the selected position when the seek is committed.

## [1.3.5] - 2026-11-09

### Added
- Eligible H.264 transcodes offer two synchronized HLS quality levels for web browsers and Chromecast. The browser can also cap quality based on player size and dropped frames.
- The quality limit carries over to Google Cast and stays available during casting. The server logs how many levels the playlist offers and which level each segment belongs to.

### Fixed
- Updated bundled SQLite to 3.54.1 to address CVE-2026-119375.
- Chromecast confirms it loaded the requested media before MBL releases local playback. Adaptive Cast prepares the initial fragment and the resume fragment at both quality levels.
- Player controls wrap on narrow screens. Subtitle and speed menus and the diagnostics panel fit within the mobile viewport.

## [1.3.4] - 2026-11-08

### Added
- The hardware page warns when the NVIDIA driver is too old for FFmpeg's NVENC API and shows the installed and required driver versions when available.
- Cast diagnostics include subtitle selection, paused progress reports, and receiver playback errors. Failed HLS rendition requests are logged on the server.

### Fixed
- Windows now throttles FFmpeg when it runs ahead of the HLS client, reducing cache usage during playback and while paused.
- Switching Cast subtitles or audio tracks reloads replacement HLS streams at the position the server prepared. If the receiver unloads the media after an error, MBL keeps the last playback position.
- Hardware self-tests replace old results so checks for backends that are no longer tested disappear from the report.

## [1.3.3] - 2026-11-08

### Changed
- Libraries shows a row of recently added media for each library, with folder paths tucked into administrator details.
- Up Next advances past the furthest completed episode and leaves an in-progress episode in Continue Watching.

### Fixed
- Browse, search and playback suggestions skip series, seasons and episodes without an available video file. Episodes can share a file.
- Windows MSI waits for the service to stop before replacing files and reports service management failures. The CLI waits for service transitions and distinguishes access denied from an already removed service.

## [1.3.2] - 2026-11-08

### Added
- Admin diagnostics combines recent MBL server events, browser API and playback events, and Chromecast activity. Filter by source, level or text and copy matching events as JSON.
- Library pages show whether a scan is queued, running, complete or failed, along with its job number, progress and any reported error. The library refreshes when the scan completes.
- Administrators can restart MBL from Server Settings and see when it comes back online.

### Changed
- MBL now stands for Media Box Lounge. Package names, service names and integration identifiers keep their existing values.

### Fixed
- A scan cannot report success or update the last scan time when every library folder is unavailable.
- Playback reports an actionable connection error when the startup request fails, and series pages without playable files skip the playback preview.
- Windows hardware reports use the host's RAM and detected NVIDIA adapter. Encoder self-tests verify that FFmpeg actually processes frames and keep the first useful failure details.
- Playback startup logs omit signed stream URLs.

## [1.3.1] - 2026-11-08

### Added
- Bundled static Gyan.FFmpeg essentials (`ffmpeg.exe` and `ffprobe.exe`) directly in the Windows MSI installer and standalone Windows ZIP packages, eliminating manual FFmpeg installation for library scanning, audio probing, and video transcoding.
- Automated GitHub release update checker:
  - New backend endpoint `GET /api/v1/system/update` querying GitHub releases with a 15-minute in-memory cache and recommended download asset matching the running OS and architecture.
  - Interactive "Software Updates" status card in *Admin → Settings* displaying current version, update notifications, direct platform download button, and a manual "Check for Updates" button.
- Windows platform profile and memory detection:
  - Host platform detection identifies NVIDIA GPUs (e.g. GeForce RTX 4060) via driver tools.
  - System health report reads total physical RAM on Windows using `GlobalMemoryStatusEx`.

### Fixed
- Library scanner error on fresh Windows installations caused by missing FFmpeg binaries in PATH.

## [1.3.0] - 2026-11-08

### Added
- Installable web app for phones and tablets, with a responsive layout and app-shell caching.
- Interface translations for 23 languages, including Tagalog, Cebuano and Waray.

### Changed
- The web interface has updated navigation, consistent menus and controls, and reduced-motion-aware view transitions.
- The web player prefetches HLS fragments and can feed them to the decoder during download. Its back buffer retains up to 30 seconds for short backward seeks.
- Direct Play and HLS file responses use 128 KiB read buffers. HLS responses stream from disk without loading the entire file into memory first.
- HLS sessions track published fragments per rendition in memory, reducing disk work while waiting for segments during playback and Cast preparation.
- The web player reports browser codec support for HEVC, AV1, AC-3 and E-AC-3. Playback and its preview use separate capabilities for direct files and HLS.
- HLS remux sessions for local playback prepare the fragment at the resume position, avoiding a second FFmpeg launch for the first request.

### Verified
- Physical Chromecast playback on Bedroom TV with three synthetic clips covering Direct Play, HLS with AAC, and HLS with E-AC-3 converted to AAC stereo.

## [1.2.3] - 2026-11-04

### Added
- Native Hailo-8 / Hailo-8L / Hailo-10H NPU runtime (`HailoNativeRuntime`) with cooperative `VDevice` sharing (Frigate-safe low priority), COCO NMS parsing, and real FFmpeg keyframe extraction for scene indexing.
- Direct HailoRT GenAI support: `VLM` scene descriptions, on-device `LLM` recap fallback and `Speech2Text` (Whisper) transcription, plus discovery of Hailo Model Zoo GenAI v5.5.0 models.
- Live Hailo telemetry (firmware, temperature, NNC utilization, RAM, voltage), NPU self-test, and Frigate co-tenant detection fix.
- Interactive `.hef` model picker in *Admin → Hardware* with hot-reload of NPU sessions; `GET /api/v1/system/browse?ext=` file filtering and `hailo_*_hef_path` system settings.
- Android client server prerequisites (M11.1 S1–S7):
  - mDNS/DNS-SD announcement as `_mbl._tcp.local.` (disable with `server.mdns = false`).
  - Stable `serverId` in `GET /api/v1/system/info` and a public `GET /api/v1/system/endpoints` listing LAN, tailnet and public URLs plus the HTTPS CA fingerprint.
  - Built-in `android-exoplayer` client profile.
  - Offline downloads: `POST /api/v1/items/{id}/downloads` (`original`, `universal_1080p`, `universal_720p`), `GET /api/v1/downloads`, `GET`/`DELETE /api/v1/downloads/{id}` and a resumable `GET /api/v1/downloads/{id}/file`.
  - Offline progress sync with last-write-wins timestamps: `POST /api/v1/users/me/progress/sync`.
  - Synced lyrics from `.lrc` sidecars or embedded tags: `GET /api/v1/items/{id}/lyrics`.
  - Account self-deletion (`DELETE /api/v1/users/me`, password required) with a *Delete account* section on the account page.

## [1.2.2] - 2026-11-02

### Added
- Interactive Windows MSI installer wizard (`WixUIExtension`) with installation directory selection (`InstallDirDlg` / **Change...** button) and customizable feature tree (`CustomizeDlg`) allowing optional toggling of the Windows Background Service (`ServiceFeature`), Distributed Transcode Worker, Federated Storage Node, and MBL TV Client.
- Native Win32 Service Control Manager (SCM) dispatcher (`StartServiceCtrlDispatcherW` / `SetServiceStatus`) in `mbl-server` (`--service` and `mbl-server service install|uninstall|start|stop|run`), plus a Start Menu shortcut for console mode.
- WebTV streaming provider and payment tier classification (`paid`, `freemium`, `free`, `info`) with visual badges and safe external site links (`🌐 Open`) across all search results.
- Dual-tab WebTV source view (*Verified Sources* vs *All Search Hits*) with structured `[WebTV]`/`[OnlinePlayer]` browser logging and `mbl::online` server tracing.
- Google Cast integration for WebTV online playback (`OnlinePlayer.tsx`) with URL token propagation (`?token=`) across HLS manifests and segments.
- Build and git version metadata (`git_commit`, `build_time`) exposed via `GET /api/v1/system/info`, startup logs, sidebar footer, and Server Settings.

### Fixed
- Fixed Windows MSI installer hang at *"Starting services"* (Error 1053 / 1920) by replacing blocking `<ServiceInstall>`/`<ServiceControl Wait="yes">` with native Win32 SCM status reporting and non-blocking custom service actions.

## [1.2.1] - 2026-11-01

### Added
- Active MQTT background bridge connecting dynamically to configured brokers with automatic Home Assistant MQTT Discovery for server status, active playback sessions, and media player entities.
- Full two-way Home Assistant control: dispatching `PLAY`, `PAUSE`, `STOP`, `SEEK <seconds>`, and `VOLUME <0-100>` commands directly to registered players and active playback sessions.
- Continuous player telemetry and rich JSON attributes (media title, season/episode, timecode position, duration, volume) published to `<prefix>/player/<id>/state` and `<prefix>/player/<id>/attributes`.
- Dynamic MQTT broker connection lifecycle driven by SQLite settings changes without requiring server restarts.
- Built-in HTTPS listener and TLS certificate manager supporting three certificate modes: MBL Local CA (with SANs for LAN IPs and hostnames), Tailscale HTTPS (`*.ts.net`), and ACME / Let's Encrypt / custom PEM.
- Comprehensive documentation for media library folder permissions across Linux POSIX ACLs (`setfacl`) and Windows NTFS (`icacls`), plus a dedicated guide for HTTPS and TLS certificates (`docs/book/src/tls-certificates.md`).

### Fixed
- Fixed HTTP/2 pseudo-header `:authority` handling in session CSRF validation, resolving 403 Forbidden errors during browser playback over HTTPS.
- Fixed Google Cast receiver stalls on HTTPS setups by rewriting private LAN media URLs to local HTTP (`8420`), accommodating Chromecast's fixed root certificate store.

## [1.2.0] - 2026-10-31

### Added
- Standardized branding to **Media Lounge Box (MBL)** across web titles, navigation layout, and DLNA device definitions.
- Home Assistant MQTT automation settings in Server Settings (`mqttEnabled`, `mqttBrokerUrl`, `mqttUsername`, `mqttPassword`, `mqttTopicPrefix`) with media player entity auto-discovery, telemetry, and remote control commands.
- DLNA UPnP configuration in Server Settings (`dlnaEnabled`, `dlnaFriendlyName`) for SSDP broadcasting and smart TV ContentDirectory streaming.
- Ready-to-copy configuration snippets for **Google Antigravity**, **Hermes Agent** (co-watching companion), and **OpenClaw** in the MCP Token Wizard (`AiAgents.tsx`).
- TMDB search endpoint (`GET /api/v1/system/tmdb/search`) and interactive TMDB search with poster previews in Media Requests (`Requests.tsx`), making manual TMDB ID entry optional.
- Direct MP4 remux delivery for Google Cast with H.264 video copy, stereo AAC 192 kbps audio, and upfront faststart indexing, enabling stutter-free Chromecast playback on high-bitrate media.
- Real-time subtitle selection and toggling on Google Cast during playback via `media.editTracksInfo` without requiring session disconnect.
- Server Settings page (`/admin/settings`) in web GUI with TMDB API key management, live API connection test, key removal, and setup notification banners in Libraries and Setup.
- Dynamic TMDB API key synchronization from SQLite settings repository across background metadata worker jobs without server restart.
- End-to-end request logging across streaming routes and playback lifecycle, capturing client IP, user agent (identifying Chromecast CrKey vs browsers), range requests, byte counts, and response latency.
- Local Cast diagnostic timeline in player statistics with receiver/media states, negotiation status, SDK errors, masked JSON copy and bounded per-attempt memory retention.
- Online player with login-bound unencrypted VOD HLS relay, bounded validated playlists/segments and real first-segment decoding before playback. Unsupported encrypted/live/ranged/external-track playlists fail explicitly.
- Per-account online viewing history and resume, recent-item controls, completion handling, profile/logout/expiry checks and media cleanup. Preview remains available separately. Added MCP `get_online_history` (78 admin, 33 read, 38 read+library tools).
- WebDrama search settings allow an administrator to select a public HTTPS SearXNG JSON endpoint. Search ranking uses title and viewing signals; matching personal indexed sources remain available with a warning during provider failures.
- Real WebDrama internet title search via Mwmbl, with manual public-source checks, per-user persisted series/season/episode metadata from JSON-LD and checked episode links. Search coverage depends on the external index; hits are not assumed to be streaming pages.
- Bounded FFprobe/FFmpeg media checks and browser previews for exposed direct media, with honest unchecked/restricted/external-player/error states. Preview rechecks the source and reports Playing only after the browser starts; unified library history and site-specific extraction remain pending.
- Authenticated online search/inspection REST endpoints and equivalent MCP tools. MCP now exposes 78 admin tools (33 read, 38 read+library); restricted profiles and private/reserved source destinations are denied.

### Fixed
- Revoked API tokens no longer persist as active in the AI agent administration table (`WHERE revoked_at IS NULL`), displaying a distinct "Revoked" badge and removing active revocation controls.
- Google Cast playback stall and freeze resolved by providing explicit `Content-Length` and `Accept-Ranges: bytes` headers on segments and playlists, avoiding chunked transfer buffering failures on Fuchsia decoders.
- RFC 8216 target duration violation fixed for copied video in HLS manifests by using `#EXT-X-TARGETDURATION: 10`, matching variable keyframe GOP durations.
- Subtitle track bounds (max 8) on Chromecast CAF receiver to prevent memory exhaustion and player crashes when casting media with numerous embedded subtitle streams.
- Google Cast handoff releases the local media source and blocks local restart while connecting; native Remote Playback retains its media element. Cast diagnostics now include local pause/readiness/position to distinguish sender audio from receiver playback.
- HLS audio encoding honors the negotiated device channel limit instead of retaining up to six channels for stereo-only receivers, including when audio synchronization is enabled.
- Series and season navigation opens their contents instead of attempting to play container objects; detail/home watch state refreshes after player stop is saved.
- Cast playback requests send whole millisecond start positions and explicitly describe MBL HLS audio/video segments as fragmented MP4, avoiding invalid JSON start positions and the receiver's MPEG-TS default. Physical receiver acceptance remains in progress.
- WebDrama source inspection selects the page's main episode/movie, including JSON-LD graph references, and no longer borrows media from related episodes. Native HTML video/source links are recognized conservatively and must pass real media checks; ambiguous players and HLS are not approved as direct playback.
- Library folder selection now shows server errors and access guidance, supports manually entered paths with server validation, and prevents stale or timed-out requests from replacing newer input. Selecting an entire filesystem warns about system files and overlapping libraries; save failures show the actual REST error.
- Library dialogs support initial focus, Escape, focus restoration and scrolling on smaller screens.
- Built-in demo plugin catalogues explicitly show unavailable playback instead of claiming to play a sample entry. Catalogue loading has visible errors, retry, cancellation and protection against stale feed responses.
- API documentation is linked from settings and the command palette. Mobile settings include agent, hardware and node administration; the palette includes nodes and media requests.

## [1.1.0] - 2026-10-29

### Added
- Account security page for TOTP setup/verification/disable and viewing/revoking login sessions; the login form accepts two-factor codes.
- Remote-access administration for server status, policy, WAN bandwidth limits and existing diagnostics.
- Basic collection/playlist web management: create, browse, rename, delete and add/remove permitted media.
- MCP now exposes 75 administrator tools (30 read and 35 read+library), including confirmed/dry-run DVR recovery.
- Explicit recovery of saved failed/interrupted DVR media as a playable partial snapshot, with REST/MCP previews, profile checks and web controls.
- Radio DVR recordings use the normal player with an audio view, track metadata, history and profile restrictions. DVB subtitles and multiple audio languages are preserved in broadcast recordings.
- Read-only canonical DVR previews shared by REST and MCP, plus a configurable free-space reserve and per-recording size cap.
- Playable partial DVR recordings with clear library labels, error details, retry/filter controls, and retention of all saved recordings.
- Profile-aware Live TV and guide filtering, programme-bounded playback for age-limited profiles, and clear channel availability and programme-end controls.
- Profile-aware DVR scheduling, recurring series matching after guide imports, season/episode controls, and owner-scoped keep-latest/delete-after-watched retention with active-playback protection.
- Completed DVR recordings open in the normal player with resume/history and real probed media metadata. A managed TV recordings library applies existing library and rating restrictions; older valid files are indexed at startup.
- Audio track selection during playback, with source track metadata and selected audio/burned-subtitle indices in playback diagnostics.
- Subtitle choices that renegotiate image/burned tracks and preserve Off, audio, quality and pause state across stream changes and Google Cast.
- Real video seek previews on the player timeline, generated as idle background work after video scans and on cache misses. Short clips and partial intervals are supported; failed decoding produces no synthetic images.
- Audio sync changes preserve the current stream and saved setting if server negotiation fails; Cast retains its active receiver when negotiation is rejected.
- Owner-scoped playback diagnostics in REST and MCP, with HLS backend/process/throttling/restart status in the player statistics panel.
- Photo library timeline with date pagination, authenticated image viewer and slideshow.
- Optional automatic next-episode playback on Google Cast, triggered only by completion of the current media load.
- Up Next episode selection across season boundaries, with manual start and optional automatic continuation for the current local playback chain.
- Player quality limits and signed server audio synchronization (−10 to +10 seconds), preserved across quality changes and Google Cast negotiation.
- API token network selection and visibility of LAN/Tailscale/WAN restrictions in the agent administration page.
- Administrator invitation management and public invitation registration page.
- Administrator profile permissions dialog for library access, managed profiles, age restrictions and PIN management.
- Media requests page with authenticated request submission, status and administrator approval controls.
- Account history import from Plex/Jellyfin JSON, with validation, import results and unmatched titles.
- **Milestone M12: Chromecast & Remote Playback Support:**
  - Google Cast Web Sender SDK integration (`web/src/utils/cast.ts`) with automatic receiver handshake (`CC1AD845` Default Media Receiver).
  - W3C Remote Playback API fallback for compatible browsers and remote displays.
  - Video player Cast control button `[⎚]` in `web/src/components/Player.tsx` with dedicated active Cast overlay and remote play/pause/seek/volume synchronization.
  - Integration with server `PlayerRegistry` (`POST /api/v1/players` and `DELETE /api/v1/players/{id}`) reporting live Chromecast telemetry and presence.
  - REST endpoint `DELETE /api/v1/players/{id}` for immediate unregistration on disconnect.
  - mdBook guide documenting Chromecast and Remote Playback behavior and limitations in `docs/book/src/chromecast.md`.
  - Comprehensive integration test suite in `crates/mbl-server/tests/m12.rs`.

### Fixed
- Protected library/rating content is filtered from collections and public playlists, including counts, item IDs and additions.
- Setting up TOTP cannot reset an already enabled second factor. Profile selection requires real account authentication instead of a local placeholder PIN.
- Library pagination loads beyond the first 200 items; search and command-palette results reject stale responses.
- Item details reload correctly on navigation, selected media/audio/subtitle versions reach playback, and an explicit part ID must belong to its requested item.
- Start from beginning sends an explicit zero position; late session responses and media changes cannot retain an abandoned player.
- Watch-state, Live TV mutations and worker administration report HTTP errors instead of false success.
- EPG windows/positions use consistent millisecond timestamps; DVR overlap checks include all channels and requested padding on the same tuner.
- WAN bandwidth allocation respects budgets below 1.5 Mbps, treats zero as unlimited and avoids overflow.
- Subtitle burn-in handles Windows drive paths, apostrophes and filter syntax characters without losing filenames during FFmpeg parsing.
- Windows Live TV HLS keeps its initialization fragment beside the playlist instead of writing it outside the session directory.
- Windows CI uses the real FFmpeg executables directly, preserving encoder cancellation and eliminating leaked test processes.
- DVR deletions retain their cleanup intent across crashes and file locks; tombstones are pruned only after known recording files have been removed.
- Copied AAC from transport streams is converted from ADTS to MP4 headers, fixing intermittent HLS startup/muxing failures without re-encoding audio.
- HLS master playlists advertise the actual video codec for copied streams and audio-only codecs/bitrate for radio. Copied HEVC fragments use the matching hvc1 sample entry.
- Retrying failed/partial DVR preserves its owner and surviving series retention policy, rechecks current profile permissions, and reviews inherited deletion settings before scheduling.
- Early DVR EOF no longer reports a full recording solely from FFmpeg success. Capture stops at its scheduled end, valid source-error output is preserved, and permissions are rechecked after probing.
- Live media links stop working after logout, login expiry or profile changes, including reused session IDs. Non-admin channel/tuner responses hide raw source URLs, and terminal live errors flush buffered playback.
- DVR jobs recheck current profile and guide permissions; catalogue PG filters no longer expose unknown rating labels that the playback policy rejects.
- Cancelled DVR airings no longer reappear automatically; explicit retry uses a new file ID. Conflicts are visible, duplicate requests respect tuner capacity, and completed recordings survive source removal. Shared viewers cannot delete another profile's recordings or rules.
- DVR duration now reflects media timestamps rather than recording job wall time. Scheduling preserves guide ratings/descriptions, completed-recording deletion removes its saved file and catalogue entry, and partial/invalid/outside-root recordings are excluded from publication.
- Live TV now uses a server-owned HLS relay with deinterlacing and browser-compatible H.264/AAC, sharing encoder capacity with VOD and tuner capacity with DVR.
- Live channel changes wait for cleanup; cancellation, source errors, expired leases, disk pressure and shutdown release owned media processes and temporary files.
- DVR reports conflicts with active live reservations and removes cancelled partial files. Unimplemented virtual-channel playback is disabled in the interface.
- Local HLS errors use limited retries with backoff and a recovery deadline. Failed master manifests reload correctly; permanent HTTP and decoding errors offer a manual restart.
- Failed streams release their server session before retrying, preserving position, tracks, quality, audio sync and pause state. Initial negotiation and native media failures also offer Retry.
- Stale player callbacks and retry timers cannot affect replacement media or Cast. Manifest recovery respects pauses and catches synchronous startup errors.
- Periodic HLS cleanup, ahead throttling and quota enforcement now run throughout the server lifecycle. Shutdown closes admission and waits for media processes and temporary directories to be removed.
- Paused and buffering players renew their session leases; abandoned sessions release playback metadata and WAN allocations without overwriting newer watch progress.
- Expired local streams offer a restart at the same position and settings. Heartbeats have a bounded timeout and are cancelled on close; unplayed local starts remain buffering until playback actually begins.
- Playback changes wait for a readable direct source or complete initial HLS media before replacing the current stream. Failed, timed-out or cancelled starts release candidate processes and capacity.
- Stream changes use the latest playback position, pending starts are aborted when the player closes, and unconfirmed startup candidates do not create watch history.
- Stopped HLS sessions cannot restart through late requests; partial failed output and stale future segments no longer masquerade as complete replacement media.
- ASS/WebVTT target selection, embedded subtitle ordinal mapping and validated external subtitle file burn-in. Invalid track indices are rejected in REST and MCP.
- HLS playback of videos without audio, correct audio codec declarations and real playlist duration for short clips.
- Optional title lookup no longer blocks Cast loading; native Remote Playback cannot accidentally close the current session to load unsupported new media.
- Trickplay preview numbering, source/cache invalidation, age restrictions and library-root checks; preview images use authenticated private responses.
- Idle analysis pauses preserve retry attempts, thermal checks read the host sensors by default, and Cast overlays no longer cover player controls.
- Cast statistics no longer display stale local video metrics; local buffer measurements respect gaps and resolution changes.
- Photo timeline counts honor library and age restrictions; original photos are served with image MIME types.
- Episode changes wait for in-flight session cleanup; player shortcuts no longer override focused form controls.
- Failed playback renegotiation no longer stops the user's previous HLS sessions to free capacity. Concurrent starts cannot exceed transcode admission limits.
- Missing HLS/FFmpeg support returns an error instead of a nonfunctional stream URL.
- Stored night mode is applied when local playback starts.
- French and Spanish language preferences can now be saved.
- Token revocation and clipboard failures are reported; snippet failures retain an already created token.
- Invitation redemption now creates the account, grants permissions and consumes the single-use link in one transaction. Invitation codes are no longer logged.
- Explicit null now clears an existing profile age restriction; omitted permissions remain unchanged.
- Cast progress/stop now use receiver position; canceled connection restores local playback.
- Receiver state uses documented SDK events, preserves zero volume and routes seek/mute/volume controls remotely.
- Cast disconnect unregisters after pending heartbeats; closing during connection cannot leave a late active session.
- Media routes provide CORS/preflight for receiver requests while preserving existing stream access checks.
- Chromecast documentation now records remaining receiver-profile, track-selection and command-transport limitations; physical device acceptance is pending.
- **Plugin Toggle & Re-enable Bug:**
  - Fixed frontend bug in `web/src/pages/LiveTv.tsx` where plugin toggle previously passed hardcoded `true`, permanently disabling plugins once toggled off.
  - Added `enabled: bool` field to `PluginManifest` and hooked `list_plugins()` to query the SQLite `plugin_configs` table so manifests always reflect real database status.
  - Added status badge (`Active` vs `Disabled`) and dynamic button label (`Enable` / `Disable`) with conditional feed browsing.
- **Player Registry Serialization:**
  - Added `#[serde(default)]` to `PlayerInfo.last_seen_at` in `crates/mbl-service/src/players.rs`, preventing 422 Unprocessable Entity errors during player heartbeats and registration.

### Verification and packages
- Regression baseline: 388 Windows Rust tests and 412 Linux Rust tests pass without skips; Windows reports no leaked test processes. The web suite passes 155 tests.
- Packaging requires successful CI for the exact release commit. Workspace lint, SQLx, OpenAPI, tokens and API/MCP checks are part of the verified workflow/tooling.
- Linux x86_64 and ARM64: Debian packages and standalone tarballs for the server, worker and node; GNU/Linux binaries are built on Debian 12 and checked for glibc 2.36 compatibility.
- Windows x86_64: 64-bit MSI, server/TV/worker/node ZIP, and separate worker/node ZIPs. The web client is embedded in the server.
- All 16 packages have SHA-256 checksums. Upload size/digests are checked before the complete release is published.

### Known limitations
- Physical Chromecast/tuner/WAN acceptance remains outstanding; automated integration tests do not certify every receiver or broadcast format.
- New M11 mobile/TV/VR client work remains on hold; existing mbl-tv packaging is retained. Virtual-channel playback is disabled until its full media transition path is implemented.
- Broader DVR transport/subtitle profiles and archive-wide storage quotas remain follow-up work.
- Hailo scene-analysis status is documented as CPU heuristics; production NPU inference is not claimed by this release.

## [1.0.0] - 2026-10-26

### Added
- Milestone M10: Hardening → v1.0.0:
  - Formal STRIDE Threat Model documentation (`docs/threat-model.md`) defining trust boundaries, asset classifications, and mitigation strategies across 4 trust zones.
  - Security hardening: Webhook URL protocol validation enforcing strict `http://` and `https://` schemas; DoS upload limits (50 MB) on M3U and XMLTV payloads; path traversal rejection in worker streaming source endpoints.
  - Property-based fuzzing and libfuzzer campaign (`fuzz/`): targets for media parser, NFO parser, streaming XMLTV, M3U playlists, shell escaper, and HLS master/media playlists.
  - Comprehensive performance benchmark report (`docs/benchmarks.md`) validating SLA metrics against PLAN §21 (scanner throughput > 500 files/s, direct play TTFF < 250 ms, zero-CPU playhead math < 0.02 ms, server idle memory 42.4 MB on Pi 5).
  - Complete mdBook documentation site (`docs/book/`) covering architecture, installation (Linux, Docker, Raspberry Pi 5, Windows), hardware acceleration, remote access with Tailscale, distributed nodes & workers, Live TV & DVR, media breadth, MCP integration, AI companions, migrations, backups, and troubleshooting.
  - Plex and Jellyfin watch history and playback state migration services (`ImportService`), REST endpoints (`POST /api/v1/import/plex`, `POST /api/v1/import/jellyfin`), and MCP tools (`import_plex_history`, `import_jellyfin_history`).
  - Comprehensive 5-language internationalization: full translations across English (`en`), Swedish (`sv`), German (`de`), French (`fr`), and Spanish (`es`) with 100% key parity and vitest test coverage.
  - Standalone release packaging for `mbl-worker` and `mbl-node` in `.github/workflows/release.yml` (.deb, .tar.gz for x86_64 and arm64, standalone Windows zip archives with readmes and systemd units).
  - MCP agent evaluation scenario suite in `evals/mcp/*.yaml` with automated scenario runner in `crates/mbl-mcp/tests/evals.rs`.
  - Expanded MCP admin toolset to 74 tools with 100.0% parity against OpenAPI 3.1 specification.

## [0.8.0] - 2026-10-26

### Added
- M9 Live TV, Channels and Online Streaming Plugins:
  - HDHomeRun tuner discovery on local network via UDP broadcast (`255.255.255.255:65001`), device inspection, lineup synchronization, and channel mapping.
  - M3U and M3U8 playlist parser (`#EXTM3U`, `#EXTINF`) extracting `tvg-id`, `tvg-logo`, `tvg-chno`, callsigns, and stream URLs.
  - Streaming XMLTV parser (`parse_xmltv_streaming`) with chunked event reading via `quick-xml`, parsing programme schedules, titles, descriptions, genres, ratings, and `xmltv_ns` season/episode numbering.
  - Live playback engine (`GET /api/v1/livetv/stream/{id}`) with tuner concurrency tracking and conflict management.
  - DVR recording scheduler (`/api/v1/livetv/recordings`, `/api/v1/livetv/rules`) supporting one-off recordings, series rules with new-only and season filters, pre/post-padding, tuner conflict checks, and retention keep policies.
  - Two-axis EPG TV Guide grid API (`/api/v1/livetv/epg`) with now/next program tracking and guide search (`/api/v1/livetv/epg/search`).
  - Online media streaming plugin provider framework (`mbl-plugins` / `PluginService`) with built-in feeds for YouTube, Spotify, and Web/K-Drama, item browsing, and stream resolution mapped to `ItemSummary`.
  - Virtual 24/7 Linear TV channels (`/api/v1/livetv/virtual-channels`) with mathematical zero-CPU live playhead calculation (`/api/v1/livetv/virtual-channels/{id}/playhead`) and rolling synthetic EPG generation.
  - SolidJS Live TV UI (`/livetv`) featuring 4 tabs: Interactive 2-axis EPG TV Guide with program detail modal & DVR schedule buttons; Tuners & Sources management with HDHomeRun auto-discovery and M3U/XMLTV import modals; DVR Recordings & Series Rules management; and Virtual 24/7 Channels with live playhead progress & online Streaming Plugins browser.
  - 5 new MCP tools (`search_guide`, `list_recordings`, `schedule_recording`, `create_series_rule`, `cancel_recording`), expanding the admin toolset to 72 tools with 100% OpenAPI coverage parity.

## [0.7.0] - 2026-10-26

### Added
- M8 Distributed nodes (Transcode workers & Remote library nodes):
  - Worker protocol specification (`docs/worker-protocol.md`) and typed data structures (`WorkerJobSpec`, `WorkerCapabilities`, `WorkerHeartbeat`, `WorkerControl`) in `mbl-core`.
  - Distributed transcode worker CLI binary `mbl-worker` with pairing code registration, hardware capability probing via `mbl-hwaccel`, periodic heartbeat loop, and pause-while-gaming logic.
  - Signed source media range chunk streaming (`/api/v1/stream/worker/source/{partId}`) with HMAC-SHA256 source tokens (`WorkerSourceTokenClaims`) guarding against directory traversal.
  - Encoded segment upload endpoint (`PUT /api/v1/stream/hls/{sessionId}/segments/{filename}`) storing chunks directly in the central server's session directory for zero-latency client serving.
  - Scheduler integration with Wake-on-LAN (UDP magic packet on port 9) to awaken sleeping GPU workers, idle worker selection, and seamless local fallback on worker loss.
  - Batch offload support for optimized versions, trickplay scrubbing sheets, and intro analysis with worker time windows and dry-run estimation.
  - Windows worker tray application support with pause-while-gaming GPU load threshold detection.
  - Admin Web UI for distributed nodes (`/admin/workers`) featuring pairing code modals for workers and storage nodes, active session and capability views, and Wake-on-LAN and revocation controls.
  - 5 new MCP tools (`list_workers`, `wake_worker`, `pair_worker`, `revoke_worker`, `list_remote_nodes`) expanding the admin toolset to 67 tools with 100% OpenAPI coverage parity.
  - Standalone Library Node daemon `mbl-node` for federated storage (NAS, remote mini-PCs, or secondary servers) with one-time pairing and periodic delta catalog synchronization (`POST /api/v1/nodes/{id}/sync-catalog`).
  - Remote Node Streaming & Smart Handover (`GET /api/v1/nodes/{id}/stream/handover/{itemId}`) detecting client IP to grant direct local play handover URLs on LAN subnets while maintaining relay fallbacks for WAN connections.

### Changed
- Package the standalone `mbl-worker` and `mbl-node` binaries in Debian and Windows MSI/ZIP releases.

## [0.6.0] - 2026-10-26

### Added
- M7 Media breadth:
  - Music scanning (`mbl-scanner`) with `lofty` metadata extraction (artist, album, track, disk, lyrics, ReplayGain, MusicBrainz IDs) and rate-limited MusicBrainz + Cover Art Archive client (`mbl-metadata::musicbrainz`).
  - EBU R128 audio loudness analysis (`mbl-analysis::loudness`) and progressive audio transcode streaming endpoint (`/api/v1/stream/audio/{partId}` in Opus, AAC, or MP3 with direct stdout piping).
  - OpenSubsonic API compatibility layer (`/rest/*`) supporting standard Subsonic mobile and desktop apps (Symfonium, Feishin, DSub, etc.).
  - Photos timeline and EXIF extraction (`mbl-scanner`, `/api/v1/libraries/{id}/timeline`) grouping photos by year and month.
  - Collections and Playlists (`/api/v1/collections`, `/api/v1/playlists`) with automatic and manual item curation.
  - Item metadata field locking (`item_metadata_locks`) preventing automatic scrapers from overwriting manually verified fields.
  - OpenSubtitles REST client (`mbl-metadata::opensubtitles`) for automatic and on-demand subtitle retrieval.
  - DLNA / UPnP Media Server (`mbl-remote::dlna`) with root device description XML, ContentDirectory DIDL-Lite browse trees, and Smart TV renderer profiles (Samsung, LG WebOS, Sony Bravia).
  - Player Registry and Remote Control (`mbl-service::players`, `/api/v1/players`) tracking web, TV, DLNA, and Cast players.
  - Webhook dispatch system (`/api/v1/webhooks`) with HMAC-SHA256 signature verification for cinema automation and notification bots.
  - Home Assistant MQTT Integration (`mbl-remote::mqtt`) with MQTT Discovery payloads for media players and diagnostic sensors, plus playback lifecycle event triggers.
  - AI Watch Companion protocol (`mbl://sessions/{id}/live-context`, `/api/v1/companion/sessions/{id}/*`) enabling AI agents (e.g. Hermes) to co-watch movies with users, receive live VTT dialogue cues, send commentary reactions, and explain scenes.
  - Subtitle Auto-Sync (`mbl-analysis::subsync`) with voice energy envelope detection and dynamic programming alignment for drifting subtitles.
  - Scene X-Ray on Pause, Spoiler-Free "Previously On..." AI recaps bounded strictly by watch history, Binge Sleep Guard, and shareable clip/GIF creation logic.
  - Movie Night Matchmaker ("Swipe to Match") real-time session engine (`/api/v1/matchmaker/sessions`).
  - Native Media Requests (`/api/v1/requests`) with Radarr/Sonarr integration, missing episodes inspector, and hardlink-aware storage analysis (`st_nlink`).
  - 20 new MCP tools (expanding total tool count to 62, 100% OpenAPI coverage parity), 5 new prompt templates (`diagnose_playback`, `library_health_check`, `optimize_for_remote`, `weekly_report`, `curate_collection`), and `resources/subscribe` live streaming.
  - Windows WiX MSI packaging script (`packaging/windows/build-msi.ps1`) and GitHub Actions release workflow integration producing `mbl-setup-x64.msi`.

## [0.5.0] - 2026-10-26

### Added
- M6 Users, sharing, remote access, and Windows:
  - Per-user library access control lists (`user_library_access`) enforced via `ItemFilter` across all items, search, and playback routes, treating unpermitted libraries as 404 Not Found.
  - Managed user profiles with parental PIN protection (Argon2id) and age content-rating ceilings (G, PG, PG-13, R, NC-17, TV-MA).
  - Share invites (`invites`, `POST /api/v1/invites`, `GET /api/v1/invites/inspect/{code}`, `POST /api/v1/auth/register-invite`) with expiration, pre-assigned library access, and content rating ceilings.
  - Time-based One-Time Password (TOTP) 2FA (`totp-rs`, RFC 6238) via `/api/v1/auth/totp/setup`, `/verify`, `/disable`, strictly required for administrators connecting over WAN.
  - Network classification (`mbl-remote::network`) distinguishing LAN subnets, Tailscale CGNAT ranges (`100.64.0.0/10`), Loopback, and WAN, with trusted proxy forward header resolution (`X-Forwarded-For`).
  - Remote bandwidth management (`mbl-remote::bandwidth`) with per-client throughput feedback and fair-share allocation under the server's WAN upload limit.
  - Tailscale integration (`mbl-remote::tailscale`) with LocalAPI client for node status, MagicDNS names, cert retrieval, and loopback-verified identity authentication (`POST /api/v1/auth/tailscale`) with spoofing rejection.
  - CGNAT reachability diagnostics (`mbl-remote::reachability`) recommending Tailscale vs. Port-Forwarding/TLS.
  - Admin endpoints guarded by default against WAN connections (`admin_lan_only` policy).
  - Windows packaging: WiX MSI installer (`packaging/windows/mbl.wxs`) with Server, Worker, and TV Client components, Windows service and tray icon support, bundled FFmpeg, Windows Firewall rule, and `winget` manifest (`packaging/windows/mbl.yaml`).
  - Docker images: multi-arch Dockerfile (`packaging/docker/Dockerfile`), VAAPI and NVIDIA hardware-accelerated variants, and `docker-compose.tailscale.yml` with Tailscale sidecar.
  - MCP mutation framework: `MutationOpts { dry_run, confirm }` on all state-modifying tools, requiring explicit confirmation or MCP elicitation for destructive operations.
  - 15 new MCP tools across `Users`, `Remote`, and `Admin` scopes: `list_users`, `create_user`, `update_user_permissions`, `create_invite`, `revoke_sessions`, `delete_user`, `get_remote_access_status`, `run_reachability_test`, `get_bandwidth_usage`, `update_remote_policy`, `get_settings`, `update_settings` (with MCP security immutability), `create_backup`, `list_backups`, `restart_server`, and prompt `setup_remote_access` (expanding admin scope to 42 tools).
  - OAuth 2.1 authorization code flow with PKCE (RFC 7636, S256) and Dynamic Client Registration for MCP clients (`/.well-known/oauth-authorization-server`, `/oauth/register`, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`).

## [0.4.0] - 2026-10-26

### Added
- M5 Pi 5 excellence + TV client:
  - Thermal and power monitoring (`mbl-platform::thermal`) reading SoC temperature and Raspberry Pi throttling/under-voltage bitmasks, automatically pausing idle jobs above 75 °C and exposing status via `/api/v1/system/health`, MCP `get_health`, and the web Hardware dashboard.
  - Platform profile defaults (`PlatformProfile::defaults`) with tuned concurrency, transcode limits, and cache sizes.
  - Automatic `tmpfs` transcode directory selection (`/run/mbl/transcode` capped at 25% of RAM) on Linux hosts with >= 4 GiB of RAM.
  - Pre-transcoded optimized versions (`mbl-analysis::optimize`, `POST /api/v1/items/{id}/optimize`, `create_optimized_versions` MCP tool) with night-window job execution; `PlaybackService` automatically prefers optimized versions for DirectPlay when the primary version requires transcoding.
  - Trickplay scrub thumbnails (`mbl-analysis::trickplay`): keyframe-only extraction into 320px 10x10 JPEG sprite sheets (`sprite_000.jpg`) with manifest index (`/api/v1/stream/trickplay/{partId}/index.json`).
  - Audio intro detection (`mbl-analysis::intro`): Chromaprint sub-fingerprinting across episodes in a season, chapter name preference, and credits detection (`mbl-analysis::credits`) using silence and black frame heuristics, merged directly into `PlaybackResponse.markers`.
  - `mbl-tv` 10-foot TV client (`crates/mbl-tv`): Slint UI using `mbl-ui-kit` tokens, `linuxkms` for DRM/KMS on Raspberry Pi and `winit` on desktop, embedded `libmpv` with `hwdec=auto-safe`, 2D spatial focus navigation, on-screen keyboard, HDMI-CEC (`libcec`), gamepad (`gilrs`), refresh-rate matching (23.976/24/50/59.94/60 Hz), HDR passthrough, audio passthrough (IEC958), and 6-digit Quick-Connect login flow (`/api/v1/auth/quick-connect`, `/authorize`, `/poll`).
  - Raspberry Pi OS appliance image build stage (`packaging/pi-gen/stage-mbl/`) with TCP BBR + `fq`, volatile journald, log2ram, and optional auto-boot into `mbl-tv`, plus Debian APT repository generation scripts (`packaging/apt/`).
  - Hailo-8 / Hailo-10H NPU multimodal media intelligence (`mbl-hailo`): 64-dimensional L2-normalized scene embeddings, natural-language semantic scene search (`GET /api/v1/ai/search-scenes`, MCP `search_scenes`), visual scene/face tagging, hardware benchmarking (`GET /api/v1/ai/benchmark`), and spoiler-free recaps (`GET /api/v1/ai/recap/{id}`, MCP `get_spoiler_free_recap`) powered by `hailo-ollama` with deterministic fallback.

## [0.3.0] - 2026-10-25

### Added
- M4 Hardware acceleration (`mbl-hwaccel`): platform fingerprinting across Linux (DRM/V4L2/Hailo/Jetson/Raspberry Pi) and Windows (DXGI), FFmpeg capability probing (`-hwaccels`, `-encoders`, `-decoders`, `-filters`), and functional self-testing cached in `hw_capabilities` (`0003_hw_capabilities.sql`).
- Hardware transcode backends in `mbl-transcode` for VAAPI, Intel QSV, NVIDIA NVENC, Raspberry Pi (`V4l2m2m` for Pi 4 and `PiDrmDecode` for Pi 5), and Windows (`D3d11Amf`), plus ordered fallback chains with 5-second early-exit failure detection and 1-hour bad-backend tracking.
- HDR10/HLG-to-SDR BT.709 tone mapping (`tonemap_vaapi`, `vpp_qsv`, `tonemap_cuda`, and software `zscale` + `tonemap=hable`), GPU multi-rendition ABR ladders (`split=N`), and remote HEVC/AV1 output preference on constrained networks.
- Hardware capacity model (`GET /api/v1/system/hw`, `POST /api/v1/system/hw/retest`, `GET /api/v1/system/transcode-capacity`), Admin Hardware & NPU UI (`web/src/pages/admin/Hardware.tsx`), and MCP hardware tools (`get_hw_report`, `get_transcode_capacity`, `run_hw_selftest`) plus `mbl://hw/report` resource.
- Hailo-8 / Hailo-10H NPU detection and cooperative multi-process `VDevice` sharing (`hailo.mode`, `hailo.vdevice_group_id`) for co-tenant setups (such as Frigate, `hailo-ollama`, or GStreamer pipelines), plus `.github/workflows/ffmpeg.yml` custom FFmpeg build pipeline.

### Fixed
- Retry temporary Windows file-sharing and locking failures when deleting expired transcode sessions after FFmpeg exits.
- Include the server executable in Linux Debian packages and verify its presence during release builds.

## [0.2.0] - 2026-10-25

### Added
- M3 Modern UI v1: generated design tokens, four themes with user accents, reusable UI components, responsive desktop/mobile/TV navigation, and an updated home experience.
- Refreshed media details and player with playback prediction, quality and subtitle controls, night mode, playback statistics, and Up Next.
- Command palette, profile picker, English translations with Swedish and German scaffolds, and initial bundle-size budget enforcement.

### Fixed
- Normalize HLS output paths on Windows and make filesystem-path tests platform-independent.

### Changed
- Install FFmpeg in Linux, ARM64, and Windows CI test jobs.

## [0.1.0] - 2026-10-25

### Added
- Cargo workspace, tooling (`just`, CI) and crate skeletons.
- `mbl-server` boots with `/health` and `/api/v1/system/info`, layered configuration, and logging to stdout and a file.
- SQLite database with the initial schema, separate read and write pools, and a backup before migrations.
- `cargo xtask gen-media` synthetic test library generator.
- Built-in device capability profiles (`chrome`, `firefox`, `safari`, `edge`, `generic-hls`) with dynamic client profile merging (`mbl-playback`).
- Pure playback decision engine (`decide`) choosing direct play, direct stream (remux), audio-only transcode or video transcode with human-readable reasons.
- Direct-play streaming endpoint (`GET/HEAD /api/v1/stream/direct/{partId}`) with HTTP `Range` support (`206`/`416`), 6-hour HMAC `?token=` authentication, and library-root path traversal protection.
- Typed FFmpeg pipeline model and pure argument builder (`mbl-transcode`) supporting remux copy, AAC/AC3/Opus audio transcoding, software video encoding (`libx264`/`libx265`/`libsvtav1`), deinterlacing (`yadif`), text/bitmap subtitle burn-in, filter-graph escaping, and fMP4 HLS segment output.
- HLS transcode/remux session manager (`mbl-transcode`, `mbl-stream`) with upfront master/media playlist generation, `/api/v1/stream/hls/{session}/...` endpoints, seek-on-demand FFmpeg restarts, `SIGSTOP`/`SIGCONT` ahead-throttling, 60-second inactivity cleanup, disk quota enforcement, and session concurrency caps.
- External sidecar subtitle detection during library scan (`Movie.en.srt`, `.en.forced.srt`, `.eng.sdh.srt`) and subtitle streaming endpoint (`GET /api/v1/stream/subtitles/{partId}/{streamIndex}.{vtt,ass}`) with FFmpeg WebVTT conversion, raw ASS delivery, and disk caching.
- Playback session negotiation and watch state API (`POST /api/v1/playback`, `/api/v1/playback/{session}/progress`, `/api/v1/playback/{session}/stop`, and cascading `POST/DELETE /api/v1/items/{id}/watched`), resume offset persistence, `watch_history` recording, and automatic pausing of idle background jobs during active playback.
- Model Context Protocol (`mbl-mcp`) server bootstrap mounted at `/mcp` (Streamable HTTP) with `Origin` and WAN validation (`mcp.enabled`, `mcp.allow_wan`, `mcp.allowed_origins`), `mbl-cli mcp --server <url>` stdio proxy, and `just mcp-inspect`.
- Scoped API tokens (`POST/GET/DELETE /api/v1/auth/tokens`; scopes `read`, `playback`, `library`, `users`, `remote`, `livetv`, `admin`; optional expiry and allowed networks), per-caller MCP tool filtering, per-token rate limits, and an `mcp_audit` log with secret redaction and 90-day retention.
- Model Context Protocol tools v0 (21 tools across `system`, `libraries`, `media`, `watch`, `playback`, and `jobs`), static and templated resources (`mbl://server/info`, `mbl://server/health`, `mbl://sessions/active`, `mbl://items/{id}`, `mbl://jobs/{id}`), untrusted metadata hygiene (control/bidi character stripping, structuredContent isolation), and cursor pagination.
- MCP administration REST endpoints (`GET /api/v1/mcp/audit` with token/tool/outcome filters, `GET/PATCH /api/v1/mcp/status` with global kill-switch toggle, `POST /api/v1/mcp/snippets` generating VS Code/Claude Desktop/Cursor/stdio configuration, `GET /api/v1/mcp/presets` for the Token Wizard, and `GET /api/v1/auth/tokens?all=true` for admin token auditing).
- Web client bootstrap (`web/` workspace using Vite, SolidJS, `@solidjs/router`, `@tanstack/solid-query`, `@tanstack/solid-virtual`, `openapi-fetch`, `hls.js`, `@kobalte/core`, Biome, Vitest, typed API client generated via `pnpm gen:api`, auth guard, layout shell, dark theme, and `web-check` in `just check`).
- AI agent administration page (`web/src/pages/admin/AiAgents.tsx`) with Token Wizard, scope presets, one-time secret modal, copy-paste config snippets (VS Code, Claude Desktop, Cursor, stdio), active token revocation table, MCP audit log table with tool/outcome/token filters, and global MCP kill-switch toggle.
- Web MVP screens (`web/`): 4-step setup wizard, full-featured media player with dynamic `DeviceProfile` detection, direct play and HLS (`hls.js`), VTT subtitle switcher and progress reporting; home dashboard with horizontal hub rows (`Continue Watching`, `Up Next`, `Recently Added`); virtualized library poster grid with sort and unwatched filter; detail view with backdrop hero and episodes list; debounced instant search; and user administration.
- Packaging v0.1: Embedded production SPA client via `rust-embed` with SPA routing fallback; Linux Debian package configuration (`cargo-deb`) for amd64 and aarch64 (Raspberry Pi 5) with hardened systemd service unit, `postinst`/`prerm` lifecycle scripts and user/group setup; Windows distribution package structure with `README.txt` and FFmpeg setup instructions; and GitHub Actions release automation (`.github/workflows/release.yml`) building multi-platform release artifacts, checksums (`SHA256SUMS`), and draft GitHub releases on tags `v*`.

