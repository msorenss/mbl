//! Fuzzing and robustness test suite for NFO parser (M10.2).

use mbl_metadata::parse_nfo;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn parse_nfo_never_panics_on_arbitrary_string(input in "\\PC{0,1000}") {
        let _ = parse_nfo(&input);
    }

    #[test]
    fn parse_nfo_never_panics_on_xml_like_payloads(
        tag in "[a-zA-Z0-9_]{1,15}",
        val in "\\PC{0,200}",
        corrupted in proptest::bool::ANY,
    ) {
        let xml = if corrupted {
            format!("<{tag}>{val}</bad_close>")
        } else {
            format!("<{tag}>{val}</{tag}>")
        };
        let _ = parse_nfo(&xml);
    }

    #[test]
    fn parse_nfo_handles_extreme_numbers(
        year in any::<i64>(),
        rating in any::<f64>(),
    ) {
        let xml = format!(
            "<movie><title>Test</title><year>{year}</year><rating>{rating}</rating></movie>"
        );
        let res = parse_nfo(&xml);
        if let Some(meta) = res {
            assert_eq!(meta.title.as_deref(), Some("Test"));
        }
    }
}
