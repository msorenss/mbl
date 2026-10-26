#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = mbl_parser::parse_movie(s);
        let _ = mbl_parser::parse_episode(s);
        let _ = mbl_parser::parse_music(s);
        let _ = mbl_parser::classify_extra(s);
        let _ = mbl_parser::is_sample(s);
    }
});
