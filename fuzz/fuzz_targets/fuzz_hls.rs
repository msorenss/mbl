#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = mbl_stream::hls::parse_segment_filename(s);
        let _ = mbl_stream::direct::parse_range_header(s, 10_000_000);
    }
});
