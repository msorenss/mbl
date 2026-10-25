# Hardware Acceleration Self-Test Assets (`assets/hwtest/`)

`mbl-hwaccel::run_selftest` executes 2-second synthetic `lavfi` `testsrc2` decode and 1080p encode probes against each candidate hardware backend (`vaapi`, `qsv`, `nvenc`, `v4l2m2m`, `piDrmDecode`, `d3d11Amf`, and `software`), recording `ok` and measured `fps` in the `hw_capabilities` SQLite table.

When custom bitstream conformance samples (`h264_1080p.mp4`, `hevc_10bit_hdr10.mp4`, `av1_1080p.mp4`) are placed in this directory on the hardware lab runner, the conformance suite additionally verifies hardware surface decode and tone-mapping pipelines against those files.
