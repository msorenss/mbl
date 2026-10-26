//! Thermal and power-supply monitoring (M5.1).
//!
//! Reads CPU temperature from `<root>/sys/class/thermal/thermal_zone0/temp` and Raspberry Pi
//! throttling / under-voltage flags from sysfs (`get_throttled` bitmask and `rpi_volt` hwmon).

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Temperature threshold (°C) above which background idle jobs are paused (PLAN §11, M5.1).
pub const PAUSE_IDLE_TEMP_C: f32 = 75.0;

/// Snapshot of host thermal and power status.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThermalStatus {
    /// Current SoC / CPU temperature in degrees Celsius, if readable.
    pub temperature_c: Option<f32>,
    /// Raw Raspberry Pi `get_throttled` bitmask, if available.
    pub raw_throttled: Option<u32>,
    /// Currently experiencing under-voltage (`bit 0` or `rpi_volt/in0_lcrit_alarm`).
    pub under_voltage: bool,
    /// ARM frequency is currently capped (`bit 1`).
    pub frequency_capped: bool,
    /// Currently thermally throttled (`bit 2` or `bit 3`).
    pub currently_throttled: bool,
    /// Under-voltage has occurred since last boot (`bit 16`).
    pub under_voltage_occurred: bool,
    /// Thermal throttling has occurred since last boot (`bit 18` or `bit 19`).
    pub throttling_occurred: bool,
    /// Whether background idle jobs (`Priority::Idle`) are paused due to high temperature (≥ 75 °C) or active throttling.
    pub idle_jobs_paused: bool,
    /// Human-readable dashboard warnings for admins.
    pub warnings: Vec<String>,
}

impl ThermalStatus {
    /// True if idle jobs should be paused (`temperature_c >= 75.0` or active thermal throttling).
    pub fn should_pause_idle_jobs(&self) -> bool {
        self.idle_jobs_paused
    }
}

/// Read thermal and power status from the host filesystem (`/`).
pub fn read_thermal_status() -> ThermalStatus {
    read_thermal_status_from_root(Path::new("/"))
}

/// Read thermal and power status from a given root directory (e.g. `/` or a test fixture sysfs tree).
pub fn read_thermal_status_from_root(root: &Path) -> ThermalStatus {
    let temperature_c = read_temperature_c(root);
    let mut raw_throttled = read_throttled_mask(root);

    if read_rpi_volt_alarm(root) {
        raw_throttled = Some(raw_throttled.unwrap_or(0) | 0x1);
    }

    let mask = raw_throttled.unwrap_or(0);
    let under_voltage = (mask & 0x1) != 0;
    let frequency_capped = (mask & 0x2) != 0;
    let currently_throttled = (mask & 0x4) != 0 || (mask & 0x8) != 0;
    let under_voltage_occurred = under_voltage || (mask & 0x1_0000) != 0;
    let throttling_occurred =
        currently_throttled || (mask & 0x4_0000) != 0 || (mask & 0x8_0000) != 0;

    let over_temp_threshold = temperature_c.is_some_and(|t| t >= PAUSE_IDLE_TEMP_C);
    let idle_jobs_paused = over_temp_threshold || currently_throttled;

    let mut warnings = Vec::new();
    if under_voltage {
        warnings.push(
            "Power supply under-voltage detected; use an official 5V/5A USB-C PD power supply to avoid instability."
                .to_owned(),
        );
    } else if under_voltage_occurred {
        warnings.push(
            "Under-voltage has occurred since boot; check the power supply and USB-C cable."
                .to_owned(),
        );
    }

    if let Some(temp) = temperature_c
        && temp >= PAUSE_IDLE_TEMP_C
    {
        warnings.push(format!(
            "High SoC temperature ({temp:.1} °C >= {PAUSE_IDLE_TEMP_C:.0} °C): background idle jobs are paused."
        ));
    } else if currently_throttled {
        warnings.push(
            "SoC is currently thermally throttled: background idle jobs are paused.".to_owned(),
        );
    } else if throttling_occurred {
        warnings.push(
            "Thermal throttling has occurred since boot; consider improving cooling or airflow."
                .to_owned(),
        );
    }

    ThermalStatus {
        temperature_c,
        raw_throttled,
        under_voltage,
        frequency_capped,
        currently_throttled,
        under_voltage_occurred,
        throttling_occurred,
        idle_jobs_paused,
        warnings,
    }
}

fn read_temperature_c(root: &Path) -> Option<f32> {
    let candidates = ["sys/class/thermal/thermal_zone0/temp", "sys/class/hwmon/hwmon0/temp1_input"];
    for rel in candidates {
        if let Ok(raw) = std::fs::read_to_string(root.join(rel))
            && let Some(temp) = parse_temperature(&raw)
        {
            return Some(temp);
        }
    }
    None
}

pub fn parse_temperature(raw: &str) -> Option<f32> {
    let val: f32 = raw.trim().parse().ok()?;
    if !val.is_finite() || val <= 0.0 {
        return None;
    }
    // Linux thermal_zone uses millidegrees Celsius (e.g. 66100 -> 66.1 °C).
    if val >= 1000.0 { Some((val / 100.0).round() / 10.0) } else { Some(val) }
}

fn read_throttled_mask(root: &Path) -> Option<u32> {
    let candidates = [
        "sys/devices/platform/soc/soc:firmware/get_throttled",
        "sys/devices/platform/axi/1000120000.pcie/1f000d8000.firmware/get_throttled",
        "sys/class/thermal/thermal_zone0/throttled",
    ];
    for rel in candidates {
        if let Ok(raw) = std::fs::read_to_string(root.join(rel))
            && let Some(mask) = parse_throttled_mask(&raw)
        {
            return Some(mask);
        }
    }
    None
}

pub fn parse_throttled_mask(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    let trimmed = trimmed.strip_prefix("throttled=").unwrap_or(trimmed).trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        trimmed.parse::<u32>().ok().or_else(|| u32::from_str_radix(trimmed, 16).ok())
    }
}

fn read_rpi_volt_alarm(root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(root.join("sys/class/hwmon")) else {
        return false;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
            continue;
        };
        if name.trim() == "rpi_volt"
            && let Ok(alarm) = std::fs::read_to_string(dir.join("in0_lcrit_alarm"))
        {
            return alarm.trim() == "1";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_temperature_does_not_pause_idle_jobs() {
        let dir = tempfile::tempdir().unwrap();
        let tz = dir.path().join("sys/class/thermal/thermal_zone0");
        std::fs::create_dir_all(&tz).unwrap();
        std::fs::write(tz.join("temp"), "54200\n").unwrap();

        let status = read_thermal_status_from_root(dir.path());
        assert_eq!(status.temperature_c, Some(54.2));
        assert!(!status.idle_jobs_paused);
        assert!(!status.under_voltage);
        assert!(status.warnings.is_empty());
    }

    #[test]
    fn high_temperature_above_75c_pauses_idle_jobs_and_warns() {
        let dir = tempfile::tempdir().unwrap();
        let tz = dir.path().join("sys/class/thermal/thermal_zone0");
        std::fs::create_dir_all(&tz).unwrap();
        std::fs::write(tz.join("temp"), "78400\n").unwrap();

        let status = read_thermal_status_from_root(dir.path());
        assert_eq!(status.temperature_c, Some(78.4));
        assert!(status.should_pause_idle_jobs());
        assert_eq!(status.warnings.len(), 1);
        assert!(status.warnings[0].contains("78.4 °C"));
    }

    #[test]
    fn parses_throttled_flags_and_rpi_volt_alarm_from_fixture_sysfs() {
        let dir = tempfile::tempdir().unwrap();
        let tz = dir.path().join("sys/class/thermal/thermal_zone0");
        let fw = dir.path().join("sys/devices/platform/axi/1000120000.pcie/1f000d8000.firmware");
        std::fs::create_dir_all(&tz).unwrap();
        std::fs::create_dir_all(&fw).unwrap();
        std::fs::write(tz.join("temp"), "68000\n").unwrap();
        // 0x50005 = under-voltage (0x1) + currently throttled (0x4) + historical under-voltage (0x10000) + historical throttled (0x40000)
        std::fs::write(fw.join("get_throttled"), "0x50005\n").unwrap();

        let status = read_thermal_status_from_root(dir.path());
        assert_eq!(status.raw_throttled, Some(0x50005));
        assert!(status.under_voltage);
        assert!(status.currently_throttled);
        assert!(status.under_voltage_occurred);
        assert!(status.throttling_occurred);
        assert!(status.should_pause_idle_jobs());
        assert_eq!(status.warnings.len(), 2);

        // Also test rpi_volt hwmon alarm when get_throttled is absent
        std::fs::remove_file(fw.join("get_throttled")).unwrap();
        let hwmon = dir.path().join("sys/class/hwmon/hwmon3");
        std::fs::create_dir_all(&hwmon).unwrap();
        std::fs::write(hwmon.join("name"), "rpi_volt\n").unwrap();
        std::fs::write(hwmon.join("in0_lcrit_alarm"), "1\n").unwrap();

        let status2 = read_thermal_status_from_root(dir.path());
        assert!(status2.under_voltage);
        assert!(!status2.currently_throttled);
    }
}
