//! Time helpers. Timestamps are stored as UTC unix milliseconds (`INTEGER`).

use ::time::OffsetDateTime;

pub fn now_ms() -> i64 {
    to_ms(OffsetDateTime::now_utc())
}

pub fn to_ms(t: OffsetDateTime) -> i64 {
    // i128 nanos → ms fits in i64 for any date within ±292 million years.
    (t.unix_timestamp_nanos() / 1_000_000) as i64
}

pub fn from_ms(ms: i64) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ms() {
        let ms = 1_761_250_000_123;
        assert_eq!(to_ms(from_ms(ms).unwrap()), ms);
    }

    #[test]
    fn now_is_after_2025() {
        assert!(now_ms() > 1_735_689_600_000);
    }
}
