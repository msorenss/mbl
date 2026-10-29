MBL Distributed Transcode Worker for Windows (v1.1.0)
======================================================

Welcome to mbl-worker! This binary turns any Windows machine (such as your GPU gaming PC)
into an on-demand distributed transcoding worker for your MBL media server.

Quick Start
-----------
1. Install FFmpeg:
   Ensure FFmpeg is installed and accessible in your PATH, or place ffmpeg.exe and ffprobe.exe
   in the same folder as mbl-worker.exe.
   Recommended: `winget install Gyan.FFmpeg`

2. Pair with your MBL Server:
   Generate a 6-digit pairing code in the MBL Web UI (Admin -> Workers & Nodes -> Pair Worker).
   Then run:
       mbl-worker.exe pair --server http://<YOUR_MBL_SERVER_IP>:8420 --code <CODE>

3. Run the Worker:
   Start the worker daemon:
       mbl-worker.exe run

   Options:
       --pause-while-gaming : Automatically pause transcoding when high GPU load from games is detected.
