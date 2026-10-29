# Migrating from Plex & Jellyfin

MBL provides dedicated migration services (`ImportService`) and REST / MCP endpoints to import watch histories, play counts, and resume points from existing Plex or Jellyfin installations.

## Migrating from Plex

Export your Plex watch history or extract view records via Plex API tokens, then post the payload to:

```bash
POST /api/v1/import/plex
```

Payload structure:

```json
{
  "items": [
    {
      "title": "Inception",
      "year": 2010,
      "tmdbId": 27205,
      "imdbId": "tt1375666",
      "viewCount": 3,
      "viewOffsetMs": 4500000,
      "lastViewedAt": 1700000000000
    }
  ]
}
```

- MBL matches items primarily against external metadata identifiers (`tmdb_id`, `imdb_id`, `tvdb_id`).
- If identifiers are missing, a fuzzy title and release year fallback is executed.
- Watch states and history logs (`method = 'import'`, `client = 'plex'`) are updated automatically.

## Migrating from Jellyfin

Jellyfin user data can be exported using the Jellyfin REST API (`/Users/{userId}/Items`):

```bash
POST /api/v1/import/jellyfin
```

Payload structure:

```json
{
  "items": [
    {
      "name": "Interstellar",
      "productionYear": 2014,
      "played": true,
      "playCount": 2,
      "playbackPositionTicks": 36000000000,
      "lastPlayedDate": "2026-10-25T14:30:00Z",
      "providerIds": {
        "Imdb": "tt0816692",
        "Tmdb": "157336"
      }
    }
  ]
}
```

MBL automatically converts Jellyfin playback position ticks (10,000 ticks = 1 ms) to millisecond offsets and upserts the user's watch state.

## Import from the web account page

Open **Account & security → Import watch history**. Choose Plex or Jellyfin,
then select a JSON file smaller than 1 MB containing an `items` array in the
format shown on the page. The importer matches your existing library and updates
the signed-in account's watch state. The result lists matched and unmatched titles.
This uploads an exported JSON file; it does not connect to a Plex/Jellyfin server.
If the server rejects the import, the selected file remains available for retry.
