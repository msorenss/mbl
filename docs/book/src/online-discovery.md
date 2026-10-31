# WebDrama: internet discovery and source checks

This is the first real discovery flow after v1.1.0. Open **Live TV → Virtual &
Plugins**, enable **WebDrama**, then choose **Search the web**. Enter a title
such as `Singles Inferno`; search is not limited to drama or a predefined list
of streaming websites. You can also paste a known public source URL.

## What the results mean

By default the server searches [Mwmbl](https://developer.mwmbl.org/). It needs no API key,
but its index and anonymous quota limit coverage. Results can include reviews,
news and unrelated pages; a web result is not a promise of a streaming source.
The query is sent to the search provider only when you submit the form.
Repeated queries are cached for ten minutes; external requests are limited to
one per second. A provider failure is displayed rather than replaced by demo
results. Matching personal indexed sources remain available with a warning
even when the external provider fails. A different query or a known source URL may help with missing titles.

Choose **Check source** to fetch that page. Where the website publishes
Schema.org JSON-LD, MBL extracts its series, season and episode information.
Episode links can be checked separately. `mainEntity` objects and graph-ID
references take precedence over unrelated trailers or other episode objects.
Only the selected item's own `contentUrl` or associated video can supply its
structured preview source; child episodes cannot lend their video to the parent.

For pages without an exposed structured media URL, MBL also recognizes one
ordinary HTML `<video>` player with one unique, quoted `src` on the video or
its `<source>` children. Relative URLs and `&amp;` query separators are resolved
against the final page URL. Comments, scripts, styles, templates and noscript
content do not supply HTML players. Multiple players or different source URLs
remain unconfirmed. Series/season pages do not fall back to an embedded trailer.
Every extracted address still passes the public-address and actual-media checks.
HLS found through HTML is offered to the online player. Inspection alone does
not approve it; Play performs the bounded playlist and sample checks below.

Pages requiring JavaScript extraction,
a site-specific adapter, login or DRM may only offer **Open website**.

A check can report:

| Status | Meaning |
| --- | --- |
| Not checked | A search result, or a previous check older than five minutes. |
| Website found | The page loaded, without a directly verifiable media source. |
| HLS source — check in player | HLS was detected; Play validates a supported VOD playlist and sample before opening the relay. |
| Media sample verified | FFprobe recognized streams and FFmpeg decoded a short sample. |
| Access restricted or sign-in required | The website denied access (HTTP 401/403). |
| Unavailable / Playback not confirmed | The request failed or the limited sample could not confirm playback. |

**Preview** checks the source again, then opens the direct media in a browser
player. **Playing** appears only after the browser's actual playback event.
A server-decoded sample does not guarantee browser codec support, continued
source availability, or full-length playback. The check downloads at most
4 MiB per source/media request, so a valid file with metadata at its end can
remain unconfirmed. The check uses the server's resolved FFmpeg/FFprobe tools,
including configured paths and binaries installed beside MBL.

## Index, access and current limits

Inspection metadata is persisted for the inspecting user, including extracted
episodes. Another user's index cannot disclose your signed URLs. This is a
bounded cache, not a saved library: at most 512 source documents are kept across
the server and each user's search considers their latest 200. Deleting a user
removes their documents. The server does not automatically crawl episode links.

Public web content has no trusted parental ratings or local-library ACLs.
Managed profiles, profiles with an age ceiling and non-admin profiles restricted
to selected libraries cannot use internet discovery. The WebDrama enable switch
also gates REST and MCP access. Only public HTTP(S) addresses on standard ports
are fetched; private/reserved destinations, credentials in URLs and private
redirects are rejected. Page scripts are not executed. Inspection sample checks
use bounded local snapshots and do not follow playlists; the online player's
separate HLS preparation is described below.

Direct previews do not save viewing position. Use **Play with resume** for the
online player and its per-account history. Website sign-in, DRM, subtitle extraction, site-specific adapters,
automatic episode continuation, Cast, server transcoding and ML title matching
remain separate work. No restriction bypass is attempted.

REST endpoints are `GET /api/v1/online/search?q=...` and
`POST /api/v1/online/inspect` with a JSON `url` field. The equivalent MCP tools
are `search_online_sources` and `inspect_online_source`. Both return untrusted
external metadata as data. The API reference documents the response schemas.

## Search provider settings

Administrators can open **Search settings** inside the WebDrama dialog and choose
Mwmbl or SearXNG. Enter the public HTTPS **base URL** of a SearXNG instance you
operate or trust (without `/search`, query parameters or fragments). MBL uses
`/search?format=json&categories=general,videos` with a title plus `watch online`.
The instance must enable JSON search; many public instances disable it. See
[SearXNG's search API](https://docs.searxng.org/dev/search_api.html).
No instance is installed or selected automatically. Private/LAN URLs are not
supported by the current public-source fetch policy. Save then submit the title
again. Errors and rate limits are reported without silently switching providers.

Search cache keys include the endpoint. Title matching ignores punctuation and
case. Title matches, inspected series/episodes and verified samples are ranked
above weaker signals, while recap/review/news terms lower relevance. These are
heuristics, not evidence of streaming rights, source availability or playback.
Only media inspection can offer a preview; ranking alone leaves hits unchecked.

## Online player, HLS relay and resume

After checking a source, choose **Play with resume**. This opens the online
player; use its media controls to start playback. Direct media is checked again.
Supported unencrypted **VOD HLS** is fetched through a login-bound server relay:
master selection (one rendition, at most three playlist levels), a closed media
playlist and TS or fMP4 segments (optional single initialization map). The server
decodes a real first-segment sample before creating the player session. Browser
codec compatibility and the rest of the episode still depend on the source.

Each playlist/redirect/resource URL passes public-address and pinned-DNS checks.
The browser gets local opaque resource URLs rather than upstream segment links.
Cookies, credentials and referrer state are not forwarded upstream. Every relay
request checks the current login and profile, including after a segment fetch;
logout, profile restrictions, expiry, feature disable and explicit Close revoke
access. The client clears buffered media on failed heartbeats or fatal playback
errors. Heartbeats run every ten seconds, including while paused.

Limits: 512 KiB playlists, 2,048 resources, 8,192 playlist lines, 16 MiB per
complete segment/init response, two player sessions per account, 16 globally and
four simultaneous segment transfers. Preparation has a 45-second deadline after
source inspection. Sessions expire after four hours or 90 seconds of inactivity;
stale entries are pruned on access/start. No media archive or background download
is created. Encrypted/DRM streams, live or low-latency playlists, byte ranges,
external audio/subtitle renditions and unsupported tags are rejected explicitly.

Once actual playback begins, pause/heartbeat/close save the viewing position.
Loading metadata without playing does not create watch history. A completed item
starts at the beginning next time; subsequent close/heartbeat does not overwrite
completion. Reopening a source checks it again and restores the saved position.
Changed title/series/season/episode metadata resets resume instead of applying an
older episode's position. Failed sources keep their previous saved position.

**Recently played online** appears inside the WebDrama dialog and refreshes after
Close. It lists the latest 20 entries; up to 200 per account are retained with
metadata independently of the discovery cache, and removed with the account.
This online history is separate from local-library item IDs and Home hubs.
It is not yet unified with library Continue Watching or Up Next.

REST: POST `/api/v1/online/playback`, PATCH/DELETE
`/api/v1/online/playback/{id}`, authenticated manifest/resource GETs, and GET
`/api/v1/online/history`. MCP `get_online_history` reads the caller's history.
Interactive session/relay routes are documented MCP exclusions.
