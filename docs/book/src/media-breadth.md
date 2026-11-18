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
- **Metadata Field Locking:** Protect manually corrected metadata fields (titles, years, studios) from being overwritten during future automated library rescans.
- **OpenSubtitles Integration:** Automatic subtitle searching and downloading matching file hashes and IMDb/TMDB identifiers.
- **DLNA / UPnP Media Server:** Native UPnP SSDP announcement and ContentDirectory browse tree with customized TV device profiles (LG webOS, Samsung, Sony Bravia).

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
