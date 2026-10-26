use std::path::Path;

use serde::{Deserialize, Serialize};

/// Hardware platform class used to pick defaults and hardware transcode pipelines (M4.1).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum PlatformProfile {
    Pi5,
    Pi4,
    GenericArm,
    X86Intel,
    X86Nvidia,
    X86Amd,
    #[default]
    Generic,
}

impl PlatformProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pi5 => "pi5",
            Self::Pi4 => "pi4",
            Self::GenericArm => "genericArm",
            Self::X86Intel => "x86Intel",
            Self::X86Nvidia => "x86Nvidia",
            Self::X86Amd => "x86Amd",
            Self::Generic => "generic",
        }
    }

    /// True for Raspberry Pi and generic ARM SBC profiles where software tone-mapping and multi-rendition ABR ladders are disabled.
    pub const fn is_pi_or_arm(self) -> bool {
        matches!(self, Self::Pi4 | Self::Pi5 | Self::GenericArm)
    }

    /// Default concurrency, cache sizes, segment lengths and transcode limits for this platform profile (M5.2).
    pub const fn defaults(self) -> PlatformDefaults {
        match self {
            Self::Pi4 => PlatformDefaults {
                probe_concurrency: 2,
                segment_secs: 3,
                ahead_segments: 6,
                max_transcode_sessions: 1,
                image_cache_mb: 512,
                metadata_cache_mb: 128,
            },
            Self::Pi5 => PlatformDefaults {
                probe_concurrency: 2,
                segment_secs: 3,
                ahead_segments: 6,
                max_transcode_sessions: 1,
                image_cache_mb: 1024,
                metadata_cache_mb: 256,
            },
            Self::GenericArm => PlatformDefaults {
                probe_concurrency: 2,
                segment_secs: 4,
                ahead_segments: 8,
                max_transcode_sessions: 1,
                image_cache_mb: 512,
                metadata_cache_mb: 128,
            },
            Self::X86Intel | Self::X86Nvidia | Self::X86Amd => PlatformDefaults {
                probe_concurrency: 8,
                segment_secs: 6,
                ahead_segments: 10,
                max_transcode_sessions: 4,
                image_cache_mb: 2048,
                metadata_cache_mb: 512,
            },
            Self::Generic => PlatformDefaults {
                probe_concurrency: 4,
                segment_secs: 6,
                ahead_segments: 10,
                max_transcode_sessions: 2,
                image_cache_mb: 1024,
                metadata_cache_mb: 256,
            },
        }
    }
}

/// Default tuning parameters per [`PlatformProfile`] (M5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlatformDefaults {
    pub probe_concurrency: u32,
    pub segment_secs: u32,
    pub ahead_segments: u32,
    pub max_transcode_sessions: u32,
    pub image_cache_mb: u64,
    pub metadata_cache_mb: u64,
}

/// Detect the [`PlatformProfile`] of the current host.
pub fn detect_profile() -> PlatformProfile {
    detect_profile_from_root(Path::new("/"), std::env::consts::ARCH)
}

/// Detect [`PlatformProfile`] from a root directory (e.g. `/` or a test fixture sysfs tree) and CPU architecture string.
pub fn detect_profile_from_root(root: &Path, arch: &str) -> PlatformProfile {
    // 1. Check Raspberry Pi device-tree model
    for rel in ["proc/device-tree/model", "sys/firmware/devicetree/base/model"] {
        if let Ok(raw) = std::fs::read_to_string(root.join(rel)) {
            let model = raw.trim_end_matches('\0').trim();
            if model.contains("Raspberry Pi 5") {
                return PlatformProfile::Pi5;
            }
            if model.contains("Raspberry Pi 4") || model.contains("Compute Module 4") {
                return PlatformProfile::Pi4;
            }
        }
    }

    // 2. Check DRM / PCI GPU vendors and NVIDIA devices
    if root.join("dev/nvidiactl").exists() || root.join("dev/nvidia0").exists() {
        return PlatformProfile::X86Nvidia;
    }

    let mut has_nvidia = false;
    let mut has_intel = false;
    let mut has_amd = false;

    if let Ok(entries) = std::fs::read_dir(root.join("sys/class/drm")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with("card") && !name_str.starts_with("renderD") {
                continue;
            }
            if let Ok(vendor) = std::fs::read_to_string(entry.path().join("device/vendor")) {
                match vendor.trim().to_ascii_lowercase().as_str() {
                    "0x10de" => has_nvidia = true,
                    "0x8086" => has_intel = true,
                    "0x1002" | "0x1022" => has_amd = true,
                    _ => {}
                }
            }
        }
    }

    if has_nvidia {
        return PlatformProfile::X86Nvidia;
    }
    if has_intel {
        return PlatformProfile::X86Intel;
    }
    if has_amd {
        return PlatformProfile::X86Amd;
    }

    // 3. Fallback to CPU vendor / architecture
    if matches!(arch, "aarch64" | "arm") {
        return PlatformProfile::GenericArm;
    }

    if matches!(arch, "x86_64" | "x86")
        && let Ok(cpuinfo) = std::fs::read_to_string(root.join("proc/cpuinfo"))
    {
        if cpuinfo.contains("GenuineIntel") {
            return PlatformProfile::X86Intel;
        }
        if cpuinfo.contains("AuthenticAMD") {
            return PlatformProfile::X86Amd;
        }
    }

    PlatformProfile::Generic
}

/// Total physical memory in bytes, when it can be determined cheaply.
pub fn total_memory_bytes() -> Option<u64> {
    total_memory_bytes_from_root(Path::new("/"))
}

/// Read total physical memory from `<root>/proc/meminfo`.
pub fn total_memory_bytes_from_root(root: &Path) -> Option<u64> {
    let meminfo = std::fs::read_to_string(root.join("proc/meminfo")).ok()?;
    parse_meminfo_total(&meminfo)
}

pub fn parse_meminfo_total(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo() {
        let sample = "MemTotal:       16384000 kB\nMemFree:          100000 kB\n";
        assert_eq!(parse_meminfo_total(sample), Some(16_384_000 * 1024));
        assert_eq!(parse_meminfo_total("garbage"), None);
    }

    #[test]
    fn profile_serializes_camel_case() {
        assert_eq!(serde_json::to_string(&PlatformProfile::X86Nvidia).unwrap(), "\"x86Nvidia\"");
    }
}
