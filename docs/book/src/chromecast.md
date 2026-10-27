# Chromecast & Remote Playback (M12)

MBL provides first-class support for casting media directly from web and companion clients to Google Cast devices (Chromecast, Android TV, Google TV) and standard remote displays.

---

## 1. Google Cast Web Sender Integration

The MBL web client integrates the **Google Cast Web Sender SDK** (`cast_sender.js`) to provide seamless playback on Google Cast hardware:

- **Receiver Applications:** Uses the standard Google Default Media Receiver (`CC1AD845`) as well as custom MBL receivers.
- **Direct Stream Handover:** Direct streams and HLS fMP4 manifests (`application/x-mpegurl`) are routed with media metadata, including cover art posters, titles, episode info, and duration.
- **Controls & Remote State:** Volume, pause, play, seek, and track selection are bidirectionally synchronized between the web controller and the remote Chromecast screen.

---

## 2. W3C Remote Playback Fallback

For browsers and operating systems where the Google Cast framework is unavailable (such as Safari on macOS/iOS or systems with strict sandbox extensions), MBL gracefully falls back to the **W3C Remote Playback API** (`HTMLMediaElement.prototype.remote.prompt()`). This provides native support for Apple AirPlay and Miracast wireless displays.

---

## 3. Player Registry & Companion Sync

When a Cast session is established, it registers directly with the MBL server's central `PlayerRegistry` (`POST /api/v1/players`):

```json
{
  "id": "cast-1730058100",
  "name": "Living Room Chromecast Ultra",
  "deviceType": "chromecast",
  "state": "playing",
  "nowPlayingItemId": 42,
  "positionMs": 45000,
  "durationMs": 1800000,
  "volume": 80
}
```

This integration enables:
- **Companion Control:** Other authenticated devices on your network (mobile phones, tablets, or the web command palette) can view and control the active Chromecast session.
- **Smart Home Discovery:** Home Assistant MQTT discovery and webhooks automatically reflect the Chromecast's current state, pause state, and media title.
- **Immediate Teardown:** When disconnected, clients issue `DELETE /api/v1/players/{id}` to immediately release the player slot in the registry.

---

## 4. Troubleshooting & Network Requirements

- **LAN Discovery:** Google Cast requires multicast DNS (mDNS) on port `5353/UDP`. Ensure client devices and Chromecast hardware reside on the same subnet, or configure mDNS reflection across VLANs.
- **Direct IP Access:** The MBL server's `public_url` must be reachable by the Chromecast device over the local network (or Tailnet) to fetch media segments and image assets.
