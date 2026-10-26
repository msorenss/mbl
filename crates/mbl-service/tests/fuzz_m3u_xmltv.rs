//! Fuzzing and robustness test suite for M3U and XMLTV parsers (M10.2).

use mbl_service::livetv::{parse_m3u_entries, parse_xmltv_date, parse_xmltv_streaming};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn parse_m3u_never_panics_on_arbitrary_string(input in "\\PC{0,1000}") {
        let _ = parse_m3u_entries(&input);
    }

    #[test]
    fn parse_m3u_never_panics_on_corrupted_extinf(
        duration in any::<i64>(),
        attrs in "\\PC{0,100}",
        name in "\\PC{0,100}",
        url in "\\PC{0,200}",
    ) {
        let m3u = format!("#EXTM3U\n#EXTINF:{duration} {attrs},{name}\n{url}\n");
        let items = parse_m3u_entries(&m3u);
        assert!(items.len() <= 1);
    }

    #[test]
    fn parse_xmltv_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1000)) {
        let _ = parse_xmltv_streaming(&bytes);
    }

    #[test]
    fn parse_xmltv_never_panics_on_xml_like_structures(
        channel in "[a-zA-Z0-9_.-]{0,30}",
        title in "\\PC{0,100}",
        start in "\\PC{0,30}",
        stop in "\\PC{0,30}",
    ) {
        let xml = format!(
            "<tv><programme channel=\"{channel}\" start=\"{start}\" stop=\"{stop}\"><title>{title}</title></programme></tv>"
        );
        let _ = parse_xmltv_streaming(xml.as_bytes());
    }

    #[test]
    fn parse_xmltv_date_never_panics(date_str in "\\PC{0,50}") {
        let _ = parse_xmltv_date(&date_str);
    }
}
