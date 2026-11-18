# Music, Photos & Media Breadth

MBL is a complete personal media platform spanning video, music, and photography.

## Music & Hi-Res Audio

- **Tag Extraction:** Integrated with `lofty` for deep inspection of FLAC, ALAC, MP3, AAC, OGG, and Opus files.
- **MusicBrainz Integration:** Rate-limited Cover Art Archive integration fetches verified album art, release groups, and artist bios.
- **EBU R128 Loudness Normalization:** Analyzes integrated LUFS, loudness range (LRA), and True Peak values to provide gapless, uniform-volume playback without clipping.
- **Progressive Audio Transcode:** Streams on-the-fly transcoded audio to Opus, AAC, or MP3 with direct HTTP stdout streaming.
- **OpenSubsonic API (`/rest/*`):** Full compatibility with third-party audio apps like Symfonium, DSub, and Feishin.

## Photos & Timeline

- **EXIF Extraction:** Reads camera make, lens model, aperture, shutter speed, ISO, and GPS coordinates using `kamadak-exif`.
- **Chronological Timeline:** Automatically clusters and indexes media items by year and month (`/api/v1/libraries/{id}/timeline`).

## Collections & Playlists

- **Manual & Smart Collections:** Create rule-based dynamic collections (e.g., "Horror Movies from the 80s") or curated static collections.
- **Metadata editing and locks:** administrators open **Edit metadata** on an item page to change title, sort title, original title, year, content rating, studio, genres, tagline and summary. Changed fields are locked so metadata refreshes keep them; a lock can be cleared to let the next refresh replace the value. Movies and shows can be matched to another TMDB entry, and a poster or backdrop can be chosen from TMDB's images (needs a TMDB API key).
- **Lyrics:** music tracks with a sidecar `.lrc` file or embedded lyrics show them in the player; synced lyrics follow playback.
- **OpenSubtitles Integration:** Automatic subtitle searching and downloading matching file hashes and IMDb/TMDB identifiers.
- **DLNA / UPnP Media Server:** smart TVs on the home network can browse and play chosen libraries (see below).

## DLNA for TVs

**Administration → Server settings → DLNA for TVs** lets smart TVs (Samsung, LG and other DLNA players) on the home network find the server and play from it without an app.

- DLNA has no sign-in, so everyone on the network sees the same content. Choose which libraries are shared and, if needed, a maximum content rating. Nothing is shared until at least one library is chosen. DLNA is off by default.
- Only devices on the local network are answered (by their real address; forwarding headers are ignored). Tailscale and internet clients are refused.
- TVs find the server through SSDP on UDP port 1900 and fetch descriptions and media over the server's HTTP port (8420 by default). Turning DLNA on or off takes effect within a few seconds, without a restart.
- Folders follow the library: movies directly, TV shows as show → season → episode, music as artist → album → track.
- Files are sent as they are, with seeking. There is no transcoding for DLNA yet: if a TV cannot play a format (for example some MKV files or audio codecs), play it with Cast or an MBL app instead.
- Not yet: cover art, subtitles in the TV's own player, search, and change notifications (the TV sees new titles when it browses again).

## Webhooks

**Administration → Server settings → Webhooks** sends a JSON `POST` to an address when something happens. Use **Test** to check that the receiver answers.

| Event | When |
|---|---|
| `playback.started` | A client confirmed playback (first progress report) |
| `playback.stopped` | A confirmed session ended; `completed` is true from 90 % watched |
| `media.added` | A scan added new items (`count`) |
| `library.scan_completed` | A library scan finished (`added`, `missing`) |
| `library.unavailable` | A library folder could not be read |

The header `X-MBL-Event` carries the event name. With a secret, `X-MBL-Signature: sha256=<hex>` is the HMAC-SHA256 of the raw body with the secret as key. A failed delivery is retried once after 10 seconds. Example body:

```json
{
  "event": "playback.stopped",
  "timestamp": 1763400000000,
  "sessionId": "…",
  "user": { "id": 2, "username": "anna" },
  "item": { "id": 412, "title": "Pilot", "kind": "episode", "year": 2024, "show": "Dark Matter", "season": 1, "episode": 1 },
  "positionMs": 3120000,
  "durationMs": 3300000,
  "completed": true
}
```

For Home Assistant, create an automation with a *Webhook* trigger and use its URL (`http://homeassistant.local:8123/api/webhook/<id>`).
