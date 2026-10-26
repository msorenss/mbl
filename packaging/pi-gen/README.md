# MBL OS Image (`pi-gen` Stage, M5.11)

This directory contains the `stage-mbl` stage for [RPi-Distro/pi-gen](https://github.com/RPi-Distro/pi-gen) to build a ready-to-flash 64-bit Raspberry Pi 4 / Pi 5 **MBL OS** appliance image.

## Included Features
- **Zero-keyboard first boot:** advertises `http://mbl.local:8420` via `avahi-daemon` mDNS and opens straight into the MBL 4-step Setup Wizard.
- **Pre-installed packages:** `mbl-server`, `mbl-tv`, `ffmpeg`, `libmpv2`, `libcec6`, `udisks2`, and the signed MBL APT repository (`/etc/apt/sources.list.d/mbl.list`).
- **Appliance Mode (optional):** enable `systemctl enable --now mbl-tv.service` to boot directly into the 10-foot `mbl-tv` client on KMS/DRM (`linuxkms`).
- **SD-Card Longevity & Streaming Tuning:**
  - `99-mbl-bbr.conf`: enables TCP BBR congestion control and `fq` packet scheduler.
  - `journald-mbl.conf`: configures `Storage=volatile` (64 MB RAM cap).
  - `log2ram` support for `/var/log`.
  - Automatic `/run/mbl/transcode` tmpfs (capped at 25% of RAM on 4 GB / 8 GB Pi boards).
