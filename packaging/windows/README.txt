MBL Media Server for Windows (v0.7.0)
=====================================

Welcome to MBL! MBL is a high-performance, lightweight personal media server.

Quick Start
-----------
1. Install FFmpeg:
   MBL requires FFmpeg for media probing and on-the-fly transcoding.
   - Option A (Recommended): Install via Windows Package Manager:
       winget install Gyan.FFmpeg
   - Option B: Download from https://ffmpeg.org/download.html and add ffmpeg.exe
     and ffprobe.exe to your system PATH.
   - Option C: Place ffmpeg.exe and ffprobe.exe in the same folder as mbl-server.exe
     or configure their paths in mbl.toml:
       [transcode]
       ffmpeg_path = "C:\\path\\to\\ffmpeg.exe"
       ffprobe_path = "C:\\path\\to\\ffprobe.exe"

2. Run MBL Server:
   - Run mbl-server.exe from PowerShell or Command Prompt:
       .\mbl-server.exe
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
