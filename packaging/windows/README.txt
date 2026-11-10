MBL Media Server for Windows (v1.3.6)
=====================================

Welcome to MBL! MBL is a high-performance, lightweight personal media server.

Quick Start
-----------
1. Built-in FFmpeg:
   FFmpeg and FFprobe static binaries are bundled directly in the MBL installer
   and placed into the bin\ folder alongside mbl-server.exe. No separate FFmpeg
   installation is required for library scanning, audio probing, or video transcoding!
       winget install Gyan.FFmpeg
   - Option B: Download from https://ffmpeg.org/download.html and add ffmpeg.exe
     and ffprobe.exe to your system PATH.
   - Option C: Place ffmpeg.exe and ffprobe.exe in the same folder as mbl-server.exe
     or configure their paths in mbl.toml:
       [transcode]
       ffmpeg_path = "C:\\path\\to\\ffmpeg.exe"
       ffprobe_path = "C:\\path\\to\\ffprobe.exe"

   NVIDIA encoding: Run the Hardware self-test in MBL after updating the
   driver or changing FFmpeg. If FFmpeg reports an NVENC API version mismatch,
   use a driver that meets the minimum in the error or configure a compatible
   FFmpeg build using the paths above. MBL uses software encoding when no
   compatible hardware encoder passes the test.

2. Run MBL Server (Console or Windows Service):
   - Option A (Windows Background Service — Recommended):
     During the MSI installation wizard, keep "Windows Background Service (Auto-Start)"
     enabled and accept the Windows UAC prompt. If you manage the service from
     PowerShell instead, open it as administrator:
       .\mbl-server.exe service install
     To stop or remove the background service later:
       .\mbl-server.exe service stop
       .\mbl-server.exe service uninstall
   - Option B (Manual / Console Mode):
     Run mbl-server.exe from PowerShell, Command Prompt, or the Start Menu shortcut:
       .\mbl-server.exe
     To run without administrator access, extract the standalone ZIP into a folder
     you own and run it with a data directory under your user profile:
       .\mbl-server.exe --data-dir "$env:LOCALAPPDATA\MBL\data"
   - By default, MBL listens on http://localhost:8420
   - Open http://localhost:8420 in your browser to complete the setup wizard.

Configuration & Data Directories
--------------------------------
- Default data directory: %ProgramData%\MBL\data (contains database, caches, logs)
- Default configuration file: %ProgramData%\MBL\mbl.toml (optional)
- You can override configuration and data locations via CLI flags:
    .\mbl-server.exe --config C:\path\to\mbl.toml --data-dir C:\path\to\data --port 8420

Model Context Protocol (MCP)
----------------------------
MBL includes a built-in MCP server at http://localhost:8420/mcp for AI assistants.
To connect Claude Desktop, Cursor, or VS Code, generate an API token in the web UI under
Admin > AI Agents.

Distributed Worker and Library Node
-----------------------------------
- Pair a transcode worker with the server:
  .\mbl-worker.exe pair --server http://localhost:8420 --code W-123456
- Start the paired worker:
  .\mbl-worker.exe run --config worker.json
- Pair a library node (replace the URL with an address reachable by LAN clients):
  .\mbl-node.exe pair --server http://localhost:8420 --code N-123456 --base-url http://192.168.1.20:8420
- Start the paired node:
  .\mbl-node.exe run --config node.json
