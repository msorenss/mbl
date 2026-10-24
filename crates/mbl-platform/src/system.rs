use serde::Serialize;

/// Hardware platform class used to pick defaults. Real detection arrives in M4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlatformProfile {
    Pi5,
    Pi4,
    GenericArm,
    X86Intel,
    X86Nvidia,
    X86Amd,
    Generic,
}

pub fn detect_profile() -> PlatformProfile {
    PlatformProfile::Generic
}

/// Total physical memory in bytes, when it can be determined cheaply.
pub fn total_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
        parse_meminfo_total(&meminfo)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo_total(meminfo: &str) -> Option<u64> {
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
        assert_eq!(
            serde_json::to_string(&PlatformProfile::X86Nvidia).unwrap(),
            "\"x86Nvidia\""
        );
    }
}
