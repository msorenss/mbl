MBL Media Server for Linux (v1.1.0)
====================================

This archive contains mbl-server, mbl-worker and mbl-node with the web UI
embedded in the server binary. Match x86_64 or arm64 to your machine.
Built on Debian 12 (Bookworm); glibc 2.36 or newer and FFmpeg 6.1+ are required.

Quick start:
  ./mbl-server --data-dir ./data
Then open http://localhost:8420 to create the first administrator.

Keep the data directory when upgrading. Stop the old server before replacing
binaries. Debian packages are also available for systemd installation.

Worker and node pairing, configuration and systemd examples are documented in:
https://github.com/msorenss/mbl/tree/main/docs/book/src
