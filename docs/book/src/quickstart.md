# Quick Start Guide

Once MBL is installed and running, open your modern web browser to:

```
http://localhost:8420
```

## Step 1: Initial Administrator Account Setup

On first launch, you are greeted by the Setup Wizard:
1. Enter your preferred administrator **Username** and secure **Password**.
2. Optionally configure server display name and regional language settings.
3. Submit to initialize the cryptographic master key and create your initial admin token.

## Step 2: Creating Your First Library

1. Navigate to **Libraries** (`/libraries`) in the sidebar or press `Ctrl+K` and type `New Library`.
2. Enter a library name, e.g. `Movies` or `TV Shows`.
3. Select the library kind (`movie`, `show`, `music`, `photos`).
4. Select or enter the filesystem directory containing your media.
5. Click **Create & Scan**.

MBL immediately launches background filesystem indexing, metadata matching, and artwork fetching.

## Step 3: Client Playback

You can stream directly in your browser, or install:
- **`mbl-tv`:** For Android TV, Apple TV, LG webOS, or Raspberry Pi HDMI 10-foot interfaces.
- **OpenSubsonic Clients:** Connect Symfonium, DSub, or Feishin to `http://<server-ip>:8420/rest/` using your user credentials.
- **DLNA / UPnP Players:** Stream directly to smart TVs and receivers via UPnP ContentDirectory.
