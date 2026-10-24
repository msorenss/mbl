use mbl_core::{Error, Result};

pub(crate) fn username(raw: &str) -> Result<String> {
    let name = raw.trim();
    let valid_chars = name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '@'));
    if name.is_empty() || name.chars().count() > 64 || !valid_chars {
        return Err(Error::Validation(
            "username must be 1-64 characters: letters, digits, '.', '_', '-', '@'".into(),
        ));
    }
    Ok(name.to_owned())
}

pub(crate) fn password(raw: &str) -> Result<()> {
    let len = raw.chars().count();
    if !(8..=256).contains(&len) {
        return Err(Error::Validation(
            "password must be 8-256 characters".into(),
        ));
    }
    Ok(())
}

pub(crate) fn display_name(raw: Option<&str>, fallback: &str) -> Result<String> {
    let name = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback);
    if name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(Error::Validation(
            "display name must be at most 100 printable characters".into(),
        ));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames() {
        assert_eq!(username("  marcus ").unwrap(), "marcus");
        assert!(username("anna.b-c_d@home").is_ok());
        assert!(username("").is_err());
        assert!(username("has space").is_err());
        assert!(username(&"x".repeat(65)).is_err());
    }

    #[test]
    fn passwords() {
        assert!(password("12345678").is_ok());
        assert!(password("1234567").is_err());
        assert!(password(&"x".repeat(257)).is_err());
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name(None, "marcus").unwrap(), "marcus");
        assert_eq!(display_name(Some("  "), "marcus").unwrap(), "marcus");
        assert_eq!(display_name(Some(" Marcus S "), "x").unwrap(), "Marcus S");
        assert!(display_name(Some("bad\u{7}"), "x").is_err());
    }
}
