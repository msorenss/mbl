#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = mbl_service::livetv::parse_xmltv_streaming(data);
});
