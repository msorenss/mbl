//! MBL 10-foot TV Client (`mbl-tv`, M5.8–M5.10):
//! - Display backend selection (`LinuxKms` on headless Raspberry Pi, `WinitDesktop` on Windows/Linux desktop)
//! - `libmpv` render/playback configuration with `hwdec=auto-safe`, HDR passthrough, and ALSA `IEC958` audio passthrough
//! - mDNS (`_mbl._tcp.local.`) server discovery & Quick-Connect 6-digit code login flow
//! - 10-foot TV screens (`ProfilePicker`, `Home`, `Library`, `Detail`, `Search` with on-screen keyboard, `PlayerOsd`, `Settings`)
//!   consuming [`mbl_ui_kit::Theme`] tokens with 2D spatial focus navigation
//! - HDMI-CEC, gamepad, and display refresh-rate matching (`M5.10`)

use std::time::Duration;

use mbl_platform::PlatformProfile;
use mbl_ui_kit::{AccentColor, ThemeMode};
use serde::{Deserialize, Serialize};

/// Rendering window backend selected for `mbl-tv` (M5.8, ADR 0005).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayBackend {
    /// Direct KMS/DRM double-plane rendering (`linuxkms`) on Raspberry Pi 4/5 without X11/Wayland.
    LinuxKms,
    /// Desktop window (`winit`) on Windows and Linux desktop environments.
    WinitDesktop,
}

/// Choose the rendering backend for the current platform and environment (M5.8).
pub fn select_display_backend(
    profile: PlatformProfile,
    has_desktop_display: bool,
) -> DisplayBackend {
    if matches!(profile, PlatformProfile::Pi4 | PlatformProfile::Pi5) && !has_desktop_display {
        DisplayBackend::LinuxKms
    } else {
        DisplayBackend::WinitDesktop
    }
}

/// `libmpv` configuration for 4K HEVC HDR playback (M5.8, M5.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MpvConfig {
    pub hwdec: String,
    pub vo: String,
    pub gpu_context: String,
    pub hdr_passthrough: bool,
    pub target_trc: String,
    pub audio_device: Option<String>,
    pub audio_spdif: Option<String>,
}

impl MpvConfig {
    pub fn for_platform(
        profile: PlatformProfile,
        backend: DisplayBackend,
        hdr_passthrough: bool,
        audio_passthrough: &AudioPassthroughConfig,
    ) -> Self {
        let (vo, gpu_context) = match backend {
            DisplayBackend::LinuxKms => ("gpu-next".to_owned(), "drm".to_owned()),
            DisplayBackend::WinitDesktop => ("gpu-next".to_owned(), "auto".to_owned()),
        };
        let hwdec = match profile {
            PlatformProfile::Pi5 | PlatformProfile::Pi4 => "v4l2request-copy,drm-copy,auto-safe",
            _ => "auto-safe",
        }
        .to_owned();
        let target_trc = if hdr_passthrough { "pq".to_owned() } else { "bt.1886".to_owned() };
        let (audio_device, audio_spdif) = if audio_passthrough.enabled {
            (
                Some(audio_passthrough.alsa_iec958_device.clone()),
                Some(audio_passthrough.codecs.join(",")),
            )
        } else {
            (None, None)
        };

        Self { hwdec, vo, gpu_context, hdr_passthrough, target_trc, audio_device, audio_spdif }
    }

    /// Key-value options passed to `mpv_set_option_string`.
    pub fn to_mpv_options(&self) -> Vec<(String, String)> {
        let mut opts = vec![
            ("hwdec".into(), self.hwdec.clone()),
            ("vo".into(), self.vo.clone()),
            ("gpu-context".into(), self.gpu_context.clone()),
            ("target-trc".into(), self.target_trc.clone()),
            (
                "hdr-compute-peak".into(),
                if self.hdr_passthrough { "no".into() } else { "yes".into() },
            ),
        ];
        if let Some(dev) = &self.audio_device {
            opts.push(("audio-device".into(), dev.clone()));
        }
        if let Some(spdif) = &self.audio_spdif {
            opts.push(("audio-spdif".into(), spdif.clone()));
        }
        opts
    }
}

/// ALSA IEC958 / WASAPI bitstream audio passthrough settings (M5.10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioPassthroughConfig {
    pub enabled: bool,
    pub alsa_iec958_device: String,
    pub codecs: Vec<String>,
}

impl Default for AudioPassthroughConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            alsa_iec958_device: "alsa/iec958:CARD=vc4hdmi0,DEV=0".into(),
            codecs: vec![
                "ac3".into(),
                "eac3".into(),
                "dts".into(),
                "dts-hd".into(),
                "truehd".into(),
            ],
        }
    }
}

/// Match the TV HDMI refresh rate (Hz) to the source video frame rate (M5.10).
///
/// Prefers exact 1:1 cadence (`23.976` -> `23.976` or `24.0`), then exact integer multiples (`25.0` -> `50.0`, `29.97` -> `59.94`).
pub fn select_display_refresh_rate(video_fps: f32, available_hz: &[f32]) -> Option<f32> {
    if available_hz.is_empty() || video_fps <= 0.0 {
        return None;
    }
    // 1. Exact match: closest within 0.03 Hz
    let closest_exact = available_hz
        .iter()
        .filter(|&&hz| (hz - video_fps).abs() <= 0.03)
        .min_by(|&&a, &&b| (a - video_fps).abs().total_cmp(&(b - video_fps).abs()));
    if let Some(&exact) = closest_exact {
        return Some(exact);
    }
    // 2. 2x multiple: closest within 0.06 Hz (e.g. 25fps -> 50Hz, 29.97fps -> 59.94Hz, 24fps -> 48Hz)
    let double = video_fps * 2.0;
    let closest_mult2 = available_hz
        .iter()
        .filter(|&&hz| (hz - double).abs() <= 0.06)
        .min_by(|&&a, &&b| (a - double).abs().total_cmp(&(b - double).abs()));
    if let Some(&mult2) = closest_mult2 {
        return Some(mult2);
    }
    // 3. Cinema 23.976 <-> 24.0 Hz fallback
    if (video_fps - 23.976).abs() < 0.05
        && let Some(&hz24) = available_hz.iter().find(|&&hz| (hz - 24.0).abs() <= 0.05)
    {
        return Some(hz24);
    }
    // 4. Fallback to highest available standard mode (e.g. 60Hz)
    available_hz.iter().copied().max_by(|a, b| a.total_cmp(b))
}

/// Remote / D-pad / CEC / Gamepad navigation actions (M5.9, M5.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TvNavAction {
    Up,
    Down,
    Left,
    Right,
    Select,
    Back,
    PlayPause,
    FastForward,
    Rewind,
    Home,
}

/// HDMI-CEC user control key codes (CEC Specification Table 27, M5.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CecKeyCode {
    Select = 0x00,
    Up = 0x01,
    Down = 0x02,
    Left = 0x03,
    Right = 0x04,
    RootMenu = 0x09,
    Exit = 0x0D,
    Play = 0x44,
    Pause = 0x46,
    Rewind = 0x48,
    FastForward = 0x49,
}

impl CecKeyCode {
    pub const fn to_nav_action(self) -> TvNavAction {
        match self {
            Self::Up => TvNavAction::Up,
            Self::Down => TvNavAction::Down,
            Self::Left => TvNavAction::Left,
            Self::Right => TvNavAction::Right,
            Self::Select => TvNavAction::Select,
            Self::Exit => TvNavAction::Back,
            Self::RootMenu => TvNavAction::Home,
            Self::Play | Self::Pause => TvNavAction::PlayPause,
            Self::FastForward => TvNavAction::FastForward,
            Self::Rewind => TvNavAction::Rewind,
        }
    }
}

/// Gamepad button events (`gilrs`-compatible mapping, M5.10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GamepadButton {
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    South,
    East,
    Start,
    LeftTrigger,
    RightTrigger,
}

impl GamepadButton {
    pub const fn to_nav_action(self) -> TvNavAction {
        match self {
            Self::DPadUp => TvNavAction::Up,
            Self::DPadDown => TvNavAction::Down,
            Self::DPadLeft => TvNavAction::Left,
            Self::DPadRight => TvNavAction::Right,
            Self::South => TvNavAction::Select,
            Self::East => TvNavAction::Back,
            Self::Start => TvNavAction::PlayPause,
            Self::LeftTrigger => TvNavAction::Rewind,
            Self::RightTrigger => TvNavAction::FastForward,
        }
    }
}

/// 10-foot screens in `mbl-tv` (M5.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TvScreen {
    ProfilePicker,
    Home,
    Library,
    Detail,
    Search,
    PlayerOsd,
    Settings,
}

/// On-screen keyboard (6 columns × 6 rows) for 10-foot Search screen (M5.9).
pub const OSK_KEYS: [[char; 6]; 6] = [
    ['A', 'B', 'C', 'D', 'E', 'F'],
    ['G', 'H', 'I', 'J', 'K', 'L'],
    ['M', 'N', 'O', 'P', 'Q', 'R'],
    ['S', 'T', 'U', 'V', 'W', 'X'],
    ['Y', 'Z', '0', '1', '2', '3'],
    ['4', '5', '6', '7', '8', ' '],
];

/// State machine for the 10-foot TV UI with 2D spatial focus navigation (M5.9).
#[derive(Debug, Clone)]
pub struct TvAppState {
    pub screen: TvScreen,
    pub screen_stack: Vec<TvScreen>,
    pub theme: ThemeMode,
    pub accent: AccentColor,
    pub focus_row: usize,
    pub focus_col: usize,
    pub hub_rows: usize,
    pub posters_per_row: usize,
    pub search_query: String,
    pub player_paused: bool,
    pub player_position_ms: u64,
    pub player_osd_visible: bool,
}

impl Default for TvAppState {
    fn default() -> Self {
        Self {
            screen: TvScreen::Home,
            screen_stack: vec![TvScreen::ProfilePicker],
            theme: ThemeMode::Dark,
            accent: AccentColor::Amber,
            focus_row: 0,
            focus_col: 0,
            hub_rows: 5,
            posters_per_row: 20,
            search_query: String::new(),
            player_paused: false,
            player_position_ms: 0,
            player_osd_visible: true,
        }
    }
}

impl TvAppState {
    pub fn set_theme_by_name(&mut self, name: &str) {
        self.theme = match name {
            "oled-black" | "oled" => ThemeMode::Oled,
            "light" => ThemeMode::Light,
            "high-contrast" => ThemeMode::HighContrast,
            _ => ThemeMode::Dark,
        };
    }

    pub fn navigate_to(&mut self, next: TvScreen) {
        self.screen_stack.push(self.screen);
        self.screen = next;
        self.focus_row = 0;
        self.focus_col = 0;
    }

    /// Process a remote/CEC/gamepad navigation action using 2D spatial focus bounds.
    pub fn handle_action(&mut self, action: TvNavAction) {
        let (max_rows, max_cols) = match self.screen {
            TvScreen::ProfilePicker => (1, 4),
            TvScreen::Home => (self.hub_rows.max(1), self.posters_per_row.max(1)),
            TvScreen::Library => (10, 6),
            TvScreen::Detail => (3, 4),
            TvScreen::Search => (6, 6),
            TvScreen::PlayerOsd => (2, 5),
            TvScreen::Settings => (4, 2),
        };

        match action {
            TvNavAction::Up => {
                self.focus_row = self.focus_row.saturating_sub(1);
            }
            TvNavAction::Down => {
                if self.focus_row + 1 < max_rows {
                    self.focus_row += 1;
                }
            }
            TvNavAction::Left => {
                self.focus_col = self.focus_col.saturating_sub(1);
            }
            TvNavAction::Right => {
                if self.focus_col + 1 < max_cols {
                    self.focus_col += 1;
                }
            }
            TvNavAction::Select => match self.screen {
                TvScreen::ProfilePicker => self.navigate_to(TvScreen::Home),
                TvScreen::Home | TvScreen::Library => self.navigate_to(TvScreen::Detail),
                TvScreen::Detail => self.navigate_to(TvScreen::PlayerOsd),
                TvScreen::Search => {
                    let ch = OSK_KEYS[self.focus_row.min(5)][self.focus_col.min(5)];
                    self.search_query.push(ch);
                }
                TvScreen::PlayerOsd => {
                    self.player_paused = !self.player_paused;
                }
                TvScreen::Settings => {}
            },
            TvNavAction::Back => {
                if let Some(prev) = self.screen_stack.pop() {
                    self.screen = prev;
                    self.focus_row = 0;
                    self.focus_col = 0;
                }
            }
            TvNavAction::Home => {
                self.screen_stack.clear();
                self.screen = TvScreen::Home;
                self.focus_row = 0;
                self.focus_col = 0;
            }
            TvNavAction::PlayPause => {
                self.player_paused = !self.player_paused;
            }
            TvNavAction::FastForward => {
                self.player_position_ms = self.player_position_ms.saturating_add(10_000);
            }
            TvNavAction::Rewind => {
                self.player_position_ms = self.player_position_ms.saturating_sub(10_000);
            }
        }
    }
}

/// Discovered MBL server on the LAN (`_mbl._tcp.local.` mDNS or local HTTP probe, M5.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredServer {
    pub name: String,
    pub url: String,
    pub version: String,
}

/// Probe candidate MBL server URLs (`http://mbl.local:8420`, `http://127.0.0.1:8420`, or explicit URL).
pub async fn discover_server(candidates: &[String]) -> Option<DiscoveredServer> {
    let client = reqwest::Client::builder().timeout(Duration::from_millis(1200)).build().ok()?;
    for base in candidates {
        let trimmed = base.trim_end_matches('/');
        let info_url = format!("{trimmed}/api/v1/system/info");
        if let Ok(resp) = client.get(&info_url).send().await
            && resp.status().is_success()
            && let Ok(json) = resp.json::<serde_json::Value>().await
        {
            let name = json.get("name").and_then(|v| v.as_str()).unwrap_or("MBL").to_owned();
            let version =
                json.get("version").and_then(|v| v.as_str()).unwrap_or("unknown").to_owned();
            return Some(DiscoveredServer { name, url: trimmed.to_owned(), version });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_linuxkms_on_headless_pi_and_winit_on_desktop_m5_8() {
        assert_eq!(select_display_backend(PlatformProfile::Pi5, false), DisplayBackend::LinuxKms);
        assert_eq!(
            select_display_backend(PlatformProfile::Pi5, true),
            DisplayBackend::WinitDesktop
        );
        assert_eq!(
            select_display_backend(PlatformProfile::X86Intel, false),
            DisplayBackend::WinitDesktop
        );
    }

    #[test]
    fn mpv_config_enables_hwdec_hdr_and_iec958_passthrough_m5_8_m5_10() {
        let passthrough = AudioPassthroughConfig::default();
        let cfg = MpvConfig::for_platform(
            PlatformProfile::Pi5,
            DisplayBackend::LinuxKms,
            true,
            &passthrough,
        );
        assert!(cfg.hwdec.contains("auto-safe"));
        assert_eq!(cfg.gpu_context, "drm");
        assert_eq!(cfg.target_trc, "pq");
        assert_eq!(cfg.audio_spdif.as_deref(), Some("ac3,eac3,dts,dts-hd,truehd"));
    }

    #[test]
    fn spatial_navigation_and_on_screen_keyboard_work_across_tv_screens_m5_9() {
        let mut app = TvAppState::default();
        assert_eq!(app.screen, TvScreen::Home);
        assert_eq!(app.theme, ThemeMode::Dark);

        // Move right 3 posters, down 2 hub rows on Home (5x20)
        app.handle_action(CecKeyCode::Right.to_nav_action());
        app.handle_action(CecKeyCode::Right.to_nav_action());
        app.handle_action(CecKeyCode::Down.to_nav_action());
        assert_eq!((app.focus_row, app.focus_col), (1, 2));

        // Select opens Detail -> Select opens PlayerOsd
        app.handle_action(CecKeyCode::Select.to_nav_action());
        assert_eq!(app.screen, TvScreen::Detail);
        app.handle_action(GamepadButton::South.to_nav_action());
        assert_eq!(app.screen, TvScreen::PlayerOsd);

        // Fast-forward 10s via gamepad right trigger
        app.handle_action(GamepadButton::RightTrigger.to_nav_action());
        assert_eq!(app.player_position_ms, 10_000);

        // Navigate to Search and type "AI" via on-screen keyboard
        app.navigate_to(TvScreen::Search);
        app.handle_action(TvNavAction::Select); // (0,0) = 'A'
        app.handle_action(TvNavAction::Down); // row 1
        app.handle_action(TvNavAction::Right);
        app.handle_action(TvNavAction::Right); // (1,2) = 'I'
        app.handle_action(TvNavAction::Select);
        assert_eq!(app.search_query, "AI");
    }

    #[test]
    fn display_refresh_rate_matching_selects_cinema_and_pal_cadences_m5_10() {
        let modes = [23.976f32, 24.0, 50.0, 59.94, 60.0];
        assert_eq!(select_display_refresh_rate(23.976, &modes), Some(23.976));
        assert_eq!(select_display_refresh_rate(24.0, &modes), Some(24.0));
        assert_eq!(select_display_refresh_rate(25.0, &modes), Some(50.0));
        assert_eq!(select_display_refresh_rate(29.97, &modes), Some(59.94));
    }
}
