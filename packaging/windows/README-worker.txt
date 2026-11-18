MBL Distributed Transcode Worker for Windows (v1.3.8)
======================================================

Welcome to mbl-worker! This binary turns any Windows machine (such as your GPU gaming PC)
into an on-demand distributed transcoding worker for your MBL media server.

Quick Start
-----------
1. FFmpeg:
   The package includes ffmpeg.exe next to mbl-worker.exe; it is used automatically.
   Use --ffmpeg <path> to choose another build.

2. Pair with your MBL server (elevated PowerShell, so the configuration can be written to
   %ProgramData%\MBL\worker.json):
   Create a pairing code in MBL under Administration -> Distributed Nodes -> Pair Transcode Worker.
       .\mbl-worker.exe pair --server http://<YOUR_MBL_SERVER>:8420 --code <CODE> --name "Gaming PC"
   Pairing tests the GPU and CPU encoders first; this can take a minute.

3. Run the worker:
       .\mbl-worker.exe run

   The worker takes video encodes from the server while it runs. If it stops, playback continues
   on the server from the next segment.

   Options:
       --max-jobs <n>                 simultaneous encodes (default 3 with a GPU encoder, else 1)
       --pause-while-gaming false     keep taking work while the NVIDIA GPU is busy
       --work-dir <dir>               temporary output (default %TEMP%\mbl-worker)
       --config <file>                another configuration file
