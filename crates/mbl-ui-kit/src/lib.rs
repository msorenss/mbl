//! MBL UI Kit — shared design tokens, theme definitions, and Slint UI theme source (M3.1–M3.2).

use serde::{Deserialize, Serialize};

/// Raw generated Slint theme specification (`ui/theme.slint`).
pub const SLINT_THEME_SOURCE: &str = include_str!("../ui/theme.slint");

/// Supported visual themes (PLAN §10.2, M3.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    #[default]
    Dark,
    Oled,
    Light,
    HighContrast,
}

impl ThemeMode {
    pub const ALL: &[Self] = &[Self::Dark, Self::Oled, Self::Light, Self::HighContrast];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Oled => "oled",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
        }
    }
}

/// Supported user accent color presets (PLAN §10.2, M3.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AccentColor {
    #[default]
    Amber,
    Indigo,
    Emerald,
    Rose,
    Cyan,
    Violet,
}

impl AccentColor {
    pub const ALL: &[Self] =
        &[Self::Amber, Self::Indigo, Self::Emerald, Self::Rose, Self::Cyan, Self::Violet];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Amber => "amber",
            Self::Indigo => "indigo",
            Self::Emerald => "emerald",
            Self::Rose => "rose",
            Self::Cyan => "cyan",
            Self::Violet => "violet",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slint_theme_contains_all_palettes_and_accents() {
        assert!(SLINT_THEME_SOURCE.contains("export struct ThemePalette"));
        assert!(SLINT_THEME_SOURCE.contains("export global Theme"));
        for theme in ThemeMode::ALL {
            let prop = format!("palette-{}:", theme.as_str());
            assert!(SLINT_THEME_SOURCE.contains(&prop), "missing Slint palette property `{prop}`");
        }
        for accent in AccentColor::ALL {
            let prop = format!("accent-{}:", accent.as_str());
            assert!(SLINT_THEME_SOURCE.contains(&prop), "missing Slint accent property `{prop}`");
        }
    }

    #[test]
    fn theme_and_accent_serde_roundtrip() {
        for &mode in ThemeMode::ALL {
            let json = serde_json::to_string(&mode).unwrap();
            let parsed: ThemeMode = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, mode);
        }
        for &accent in AccentColor::ALL {
            let json = serde_json::to_string(&accent).unwrap();
            let parsed: AccentColor = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, accent);
        }
    }
}
