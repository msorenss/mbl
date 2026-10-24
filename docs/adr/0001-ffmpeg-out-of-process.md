# 0001. Run FFmpeg out of process

- Status: Accepted
- Date: 2026-10-23

## Context
MBL needs FFmpeg for probing, transcoding, thumbnails and analysis on very different platforms: Raspberry Pi builds with V4L2/DRM patches, Windows builds with QSV/NVENC/AMF, and distro builds. We could link libav* in-process (`ffmpeg-next`) or run the `ffmpeg`/`ffprobe` binaries as child processes.

## Decision
Run `ffmpeg` and `ffprobe` as supervised child processes. Arguments are built from a typed `Pipeline` model and passed as an argv vector, never through a shell. In-process bindings may be considered later, but only for thumbnails.

## Consequences
- A crash or hang in a decoder cannot take down the server. Processes can be killed, paused (throttling) and run with timeouts.
- FFmpeg's license (GPL builds with x264/x265) stays separate from MBL's binaries.
- Users can swap FFmpeg builds without recompiling. The self-test (M4.3) validates whatever binary is configured.
- Process spawn overhead (~tens of ms) is irrelevant compared to transcode duration.
- We must maintain an argument builder plus snapshot tests, and a robust filter-graph escaper.
