MBL Storage Node for Windows (v1.4.4)
======================================

mbl-node shares media folders on this PC with your MBL server. The server lists them as
ordinary libraries (named "<folder> (<node>)") and plays them; nothing is copied. The server
reads the files through this node, so this PC has to be on and reachable while you watch.

Requirements
------------
- MBL server 1.3.9 or later.
- ffmpeg.exe and ffprobe.exe next to mbl-node.exe (included in this package).
- A free TCP port the server can reach, 8430 below. Allow it in Windows Firewall:
      New-NetFirewallRule -DisplayName "MBL storage node" -Direction Inbound -Protocol TCP -LocalPort 8430 -Action Allow

1. Pair with the server (elevated PowerShell, so the configuration can be written to
   %ProgramData%\MBL\node.json). Create a code in MBL under
   Administration -> Distributed Nodes -> Pair Storage Node, then:
       .\mbl-node.exe pair --server http://<MBL-SERVER>:8420 --code N-123456 --base-url http://<THIS-PC-IP>:8430 --name "Gaming PC"

2. Share folders (one per library type):
       .\mbl-node.exe library add --name Movies --kind movie --path D:\Movies
       .\mbl-node.exe library add --name Series --kind show --path D:\TV
       .\mbl-node.exe library list
   Kinds: movie, show, music, photo, homevideo. Folders may not overlap.

3. Run the node:
       .\mbl-node.exe run
   It probes new files (the first run can take a while), syncs every 5 minutes
   (--sync-minutes) and serves files to the server on port 8430.

Notes
-----
- Files removed from a shared folder are marked missing in MBL at the next sync.
- `library remove --id <n>` stops sharing a folder; its items are marked missing at the next sync.
  Delete the library in MBL to remove them for good.
- Revoking the node in MBL removes its libraries.
- Seek previews (trickplay) are not generated for node media.
- If the node was paired with an older server, pair it again.
