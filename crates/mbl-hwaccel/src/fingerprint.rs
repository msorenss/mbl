//! Hardware platform and accelerator fingerprinting (M4.1, M4.17).
//!
//! Inspects Linux `/proc/device-tree/model`, `/proc/cpuinfo`, `/proc/meminfo`, `/sys/class/drm`,
//! `/sys/class/video4linux` (`/dev/video*`), Hailo NPU sysfs/dev nodes (`/dev/h1x-*`,
//! `/dev/hailo*`, `/sys/class/hailo_chardev`, PCI vendor `0x1e60`), NVIDIA GPUs (`/dev/nvidia*`,
//! `nvidia-smi -q -x`), and Windows DXGI adapters.

use std::path::Path;
use std::process::Command;

use mbl_platform::PlatformProfile;
use mbl_platform::system::{detect_profile_from_root, total_memory_bytes_from_root};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

/// Detected Neural Processing Unit (NPU) accelerator profile (M4.1, M4.17).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum NpuProfile {
    #[default]
    None,
    /// Hailo-8L (13 TOPS, M.2 / Raspberry Pi AI Kit).
    Hailo8L,
    /// Hailo-8 (26 TOPS, Raspberry Pi AI HAT+ 26 TOPS / M.2).
    Hailo8,
    /// Hailo-10H (40 TOPS GenAI + Vision/Audio NPU, Raspberry Pi AI HAT+ 2 / M.2).
    Hailo10H,
}

impl NpuProfile {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Hailo8L => "hailo8l",
            Self::Hailo8 => "hailo8",
            Self::Hailo10H => "hailo10h",
        }
    }

    /// Peak INT8/INT4 TOPS rating for the detected NPU.
    pub const fn tops(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Hailo8L => 13,
            Self::Hailo8 => 26,
            Self::Hailo10H => 40,
        }
    }

    /// Whether the NPU supports on-device GenAI LLM/VLM execution (`Hailo-10H`).
    pub const fn supports_genai(self) -> bool {
        matches!(self, Self::Hailo10H)
    }

    /// Whether any Hailo NPU is present for Whisper ASR, upscaling, and scene indexing.
    pub const fn is_present(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Linux DRM/KMS device node (`/dev/dri/card*`, `/dev/dri/renderD*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DrmDevice {
    pub card: String,
    pub render_node: Option<String>,
    pub vendor_id: Option<String>,
    pub device_id: Option<String>,
    pub driver: Option<String>,
}

/// Linux V4L2 video device (`/dev/video*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct V4l2Device {
    pub path: String,
    pub name: String,
    pub driver: Option<String>,
    pub is_m2m: bool,
}

/// NVIDIA GPU discovered via `nvidia-smi -q -x` or `/proc/driver/nvidia/gpus`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaGpu {
    pub index: u32,
    pub name: String,
    pub uuid: Option<String>,
    pub driver_version: Option<String>,
    pub memory_total_mib: Option<u64>,
    pub max_enc_sessions: u32,
}

/// Windows DXGI display/compute adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DxgiAdapter {
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub dedicated_video_memory_bytes: u64,
}

/// Detected Hailo NPU device details (`/dev/h1x-*` or `/dev/hailo*`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HailoDeviceInfo {
    pub device_node: String,
    pub board_name: String,
    pub architecture: String,
    pub pci_address: Option<String>,
    pub driver_version: Option<String>,
    pub profile: NpuProfile,
}

/// Complete hardware fingerprint of the host machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HardwareFingerprint {
    pub profile: PlatformProfile,
    pub npu_profile: NpuProfile,
    pub arch: String,
    pub os: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub total_memory_bytes: Option<u64>,
    pub board_model: Option<String>,
    pub drm_devices: Vec<DrmDevice>,
    pub v4l2_devices: Vec<V4l2Device>,
    pub nvidia_gpus: Vec<NvidiaGpu>,
    pub dxgi_adapters: Vec<DxgiAdapter>,
    pub hailo_devices: Vec<HailoDeviceInfo>,
    /// Stable SHA-256 hex digest of the hardware + driver state so self-tests re-run on driver changes.
    pub driver_fingerprint: String,
}

/// Detect the current host's [`HardwareFingerprint`].
pub fn detect_fingerprint() -> HardwareFingerprint {
    let nvidia_xml = query_nvidia_smi_xml();
    detect_fingerprint_from_root(Path::new("/"), std::env::consts::ARCH, nvidia_xml.as_deref())
}

/// Detect [`HardwareFingerprint`] from a filesystem root (e.g. `/` or a synthetic test fixture).
pub fn detect_fingerprint_from_root(
    root: &Path,
    arch: &str,
    nvidia_xml: Option<&str>,
) -> HardwareFingerprint {
    let profile = detect_profile_from_root(root, arch);
    let total_memory_bytes = total_memory_bytes_from_root(root);
    let board_model = read_board_model(root);
    let (cpu_model, cpu_cores) = read_cpu_info(root);
    let drm_devices = detect_drm_devices(root);
    let v4l2_devices = detect_v4l2_devices(root);
    let nvidia_gpus = match nvidia_xml {
        Some(xml) => parse_nvidia_smi_xml(xml),
        None => detect_nvidia_from_proc(root),
    };
    let dxgi_adapters = detect_dxgi_adapters();
    let hailo_devices = detect_hailo_devices(root);
    let npu_profile = hailo_devices
        .iter()
        .map(|d| d.profile)
        .max_by_key(|p| p.tops())
        .unwrap_or(NpuProfile::None);

    let driver_fingerprint = compute_driver_fingerprint(
        profile,
        npu_profile,
        &drm_devices,
        &v4l2_devices,
        &nvidia_gpus,
        &hailo_devices,
    );

    HardwareFingerprint {
        profile,
        npu_profile,
        arch: arch.to_owned(),
        os: std::env::consts::OS.to_owned(),
        cpu_model,
        cpu_cores,
        total_memory_bytes,
        board_model,
        drm_devices,
        v4l2_devices,
        nvidia_gpus,
        dxgi_adapters,
        hailo_devices,
        driver_fingerprint,
    }
}

fn read_board_model(root: &Path) -> Option<String> {
    for rel in [
        "proc/device-tree/model",
        "sys/firmware/devicetree/base/model",
        "sys/devices/virtual/dmi/id/board_name",
    ] {
        if let Ok(raw) = std::fs::read_to_string(root.join(rel)) {
            let trimmed = raw.trim_end_matches('\0').trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}

fn read_cpu_info(root: &Path) -> (String, u32) {
    let mut model = String::new();
    let mut count = 0u32;
    if let Ok(cpuinfo) = std::fs::read_to_string(root.join("proc/cpuinfo")) {
        for line in cpuinfo.lines() {
            if let Some((k, v)) = line.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                if key == "processor" {
                    count = count.saturating_add(1);
                } else if (key == "model name" || key == "Model" || key == "Hardware")
                    && model.is_empty()
                    && !val.is_empty()
                {
                    model = val.to_owned();
                }
            }
        }
    }
    if model.is_empty() {
        model = std::env::consts::ARCH.to_owned();
    }
    if count == 0 {
        count = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
    }
    (model, count)
}

fn detect_drm_devices(root: &Path) -> Vec<DrmDevice> {
    let mut out = Vec::new();
    let drm_dir = root.join("sys/class/drm");
    let Ok(entries) = std::fs::read_dir(&drm_dir) else {
        return out;
    };

    let mut cards: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| {
            name.starts_with("card")
                && name.len() > 4
                && name[4..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    cards.sort();

    for card_name in cards {
        let card_sys = drm_dir.join(&card_name);
        let dev_sys = card_sys.join("device");
        let vendor_id = std::fs::read_to_string(dev_sys.join("vendor"))
            .ok()
            .map(|s| s.trim().to_ascii_lowercase());
        let device_id = std::fs::read_to_string(dev_sys.join("device"))
            .ok()
            .map(|s| s.trim().to_ascii_lowercase());
        let driver = std::fs::read_link(dev_sys.join("driver"))
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .or_else(|| {
                std::fs::read_to_string(dev_sys.join("uevent")).ok().and_then(|u| {
                    u.lines().find_map(|l| l.strip_prefix("DRIVER=").map(str::to_owned))
                })
            });

        // Find matching renderD* node if any
        let render_node = find_render_node(root, &dev_sys, &card_name);
        out.push(DrmDevice {
            card: format!("/dev/dri/{card_name}"),
            render_node,
            vendor_id,
            device_id,
            driver,
        });
    }
    out
}

fn find_render_node(root: &Path, dev_sys: &Path, card_name: &str) -> Option<String> {
    if let Ok(entries) = std::fs::read_dir(dev_sys.join("drm")) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("renderD") {
                return Some(format!("/dev/dri/{name}"));
            }
        }
    }
    if let Some(idx_str) = card_name.strip_prefix("card")
        && let Ok(idx) = idx_str.parse::<u32>()
    {
        let candidate = format!("renderD{}", 128 + idx);
        if root.join("dev/dri").join(&candidate).exists()
            || root.join("sys/class/drm").join(&candidate).exists()
        {
            return Some(format!("/dev/dri/{candidate}"));
        }
    }
    None
}

fn detect_v4l2_devices(root: &Path) -> Vec<V4l2Device> {
    let mut out = Vec::new();
    let v4l_dir = root.join("sys/class/video4linux");
    let Ok(entries) = std::fs::read_dir(&v4l_dir) else {
        return out;
    };

    let mut nodes: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with("video"))
        .collect();
    nodes.sort();

    for node in nodes {
        let sys_path = v4l_dir.join(&node);
        let name = std::fs::read_to_string(sys_path.join("name"))
            .ok()
            .map(|s| s.trim().to_owned())
            .unwrap_or_else(|| node.clone());
        let driver = std::fs::read_link(sys_path.join("device/driver"))
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .or_else(|| {
                std::fs::read_to_string(sys_path.join("device/uevent")).ok().and_then(|u| {
                    u.lines().find_map(|l| l.strip_prefix("DRIVER=").map(str::to_owned))
                })
            });
        let lower = name.to_ascii_lowercase();
        let is_m2m = lower.contains("bcm2835-codec")
            || lower.contains("v4l2m2m")
            || lower.contains("rpivid")
            || lower.contains("rpi-hevc-dec")
            || lower.contains("hevc-dec")
            || lower.contains("codec-encode")
            || lower.contains("codec-decode");

        out.push(V4l2Device { path: format!("/dev/{node}"), name, driver, is_m2m });
    }
    out
}

fn detect_hailo_devices(root: &Path) -> Vec<HailoDeviceInfo> {
    let mut out = Vec::new();

    // 1. Check Hailo-10H (`hailo1x_pci`) devices via `/sys/class/hailo1x` or `/dev/h1x-*`
    let mut seen_nodes = std::collections::BTreeSet::new();

    if let Ok(entries) = std::fs::read_dir(root.join("dev")) {
        let mut dev_names: Vec<String> =
            entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
        dev_names.sort();

        for name in dev_names {
            if name.starts_with("h1x-") {
                let node = format!("/dev/{name}");
                seen_nodes.insert(node.clone());
                let driver_version = read_hailo_module_version(root, "hailo1x_pci");
                let pci_address = find_hailo_pci_address(root, &name);
                out.push(HailoDeviceInfo {
                    device_node: node,
                    board_name: "Hailo-10H".to_owned(),
                    architecture: "HAILO10H".to_owned(),
                    pci_address,
                    driver_version,
                    profile: NpuProfile::Hailo10H,
                });
            } else if name.starts_with("hailo")
                && name[5..].chars().all(|c| c.is_ascii_digit())
                && !name[5..].is_empty()
            {
                let node = format!("/dev/{name}");
                seen_nodes.insert(node.clone());
                let (board_name, arch, profile) = classify_hailo8_sysfs(root, &name);
                let driver_version = read_hailo_module_version(root, "hailo_pci");
                let pci_address = find_hailo_pci_address(root, &name);
                out.push(HailoDeviceInfo {
                    device_node: node,
                    board_name,
                    architecture: arch,
                    pci_address,
                    driver_version,
                    profile,
                });
            }
        }
    }

    // 2. Also inspect `/sys/class/hailo_chardev` in case `/dev` wasn't populated in a synthetic root
    for class_dir in ["sys/class/hailo_chardev", "sys/class/hailo1x"] {
        if let Ok(entries) = std::fs::read_dir(root.join(class_dir)) {
            let mut names: Vec<String> =
                entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
            names.sort();
            for name in names {
                let node = format!("/dev/{name}");
                if seen_nodes.contains(&node) {
                    continue;
                }
                seen_nodes.insert(node.clone());
                if name.starts_with("h1x-") || class_dir.ends_with("hailo1x") {
                    out.push(HailoDeviceInfo {
                        device_node: node,
                        board_name: "Hailo-10H".to_owned(),
                        architecture: "HAILO10H".to_owned(),
                        pci_address: find_hailo_pci_address(root, &name),
                        driver_version: read_hailo_module_version(root, "hailo1x_pci"),
                        profile: NpuProfile::Hailo10H,
                    });
                } else {
                    let (board_name, arch, profile) = classify_hailo8_sysfs(root, &name);
                    out.push(HailoDeviceInfo {
                        device_node: node,
                        board_name,
                        architecture: arch,
                        pci_address: find_hailo_pci_address(root, &name),
                        driver_version: read_hailo_module_version(root, "hailo_pci"),
                        profile,
                    });
                }
            }
        }
    }

    // 3. Fallback PCI scan (`/sys/bus/pci/devices/*` with vendor `0x1e60`) if no chardev was found yet
    if out.is_empty()
        && let Ok(entries) = std::fs::read_dir(root.join("sys/bus/pci/devices"))
    {
        let mut addrs: Vec<String> =
            entries.flatten().filter_map(|e| e.file_name().into_string().ok()).collect();
        addrs.sort();
        for addr in addrs {
            let dev_dir = root.join("sys/bus/pci/devices").join(&addr);
            let Ok(vendor) = std::fs::read_to_string(dev_dir.join("vendor")) else {
                continue;
            };
            if !vendor.trim().eq_ignore_ascii_case("0x1e60") {
                continue;
            }
            let device_id = std::fs::read_to_string(dev_dir.join("device"))
                .ok()
                .map(|s| s.trim().to_ascii_lowercase())
                .unwrap_or_default();
            // 0x45c4 = Hailo-10H, 0x2864 = Hailo-8 / Hailo-8L
            let (node, board, arch, profile, drv) = if device_id == "0x45c4" {
                (
                    "/dev/h1x-0".to_owned(),
                    "Hailo-10H".to_owned(),
                    "HAILO10H".to_owned(),
                    NpuProfile::Hailo10H,
                    read_hailo_module_version(root, "hailo1x_pci"),
                )
            } else {
                (
                    "/dev/hailo0".to_owned(),
                    "Hailo-8".to_owned(),
                    "HAILO8".to_owned(),
                    NpuProfile::Hailo8,
                    read_hailo_module_version(root, "hailo_pci"),
                )
            };
            out.push(HailoDeviceInfo {
                device_node: node,
                board_name: board,
                architecture: arch,
                pci_address: Some(addr),
                driver_version: drv,
                profile,
            });
        }
    }

    out
}

fn classify_hailo8_sysfs(root: &Path, name: &str) -> (String, String, NpuProfile) {
    let sys_base = root.join("sys/class/hailo_chardev").join(name);
    let board = std::fs::read_to_string(sys_base.join("board_name"))
        .or_else(|_| std::fs::read_to_string(sys_base.join("device_architecture")))
        .ok()
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| "Hailo-8".to_owned());

    let upper = board.to_ascii_uppercase();
    if upper.contains("10H") {
        ("Hailo-10H".to_owned(), "HAILO10H".to_owned(), NpuProfile::Hailo10H)
    } else if upper.contains("8L") {
        ("Hailo-8L".to_owned(), "HAILO8L".to_owned(), NpuProfile::Hailo8L)
    } else {
        ("Hailo-8".to_owned(), "HAILO8".to_owned(), NpuProfile::Hailo8)
    }
}

fn read_hailo_module_version(root: &Path, module: &str) -> Option<String> {
    std::fs::read_to_string(root.join("sys/module").join(module).join("version"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn find_hailo_pci_address(root: &Path, dev_name: &str) -> Option<String> {
    for class_dir in ["sys/class/hailo_chardev", "sys/class/hailo1x"] {
        let p = root.join(class_dir).join(dev_name);
        if let Ok(text) = std::fs::read_to_string(p.join("device/uevent")) {
            for line in text.lines() {
                if let Some(slot) = line.strip_prefix("PCI_SLOT_NAME=") {
                    return Some(slot.trim().to_owned());
                }
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(root.join("sys/bus/pci/devices")) {
        for entry in entries.flatten() {
            if let Ok(v) = std::fs::read_to_string(entry.path().join("vendor"))
                && v.trim().eq_ignore_ascii_case("0x1e60")
            {
                return entry.file_name().into_string().ok();
            }
        }
    }
    None
}

fn query_nvidia_smi_xml() -> Option<String> {
    if !Path::new("/dev/nvidiactl").exists() && !cfg!(windows) {
        return None;
    }
    let output = Command::new("nvidia-smi").args(["-q", "-x"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn detect_nvidia_from_proc(root: &Path) -> Vec<NvidiaGpu> {
    let mut out = Vec::new();
    let gpus_dir = root.join("proc/driver/nvidia/gpus");
    let driver_version = std::fs::read_to_string(root.join("proc/driver/nvidia/version"))
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned));

    if let Ok(entries) = std::fs::read_dir(gpus_dir) {
        for (idx, entry) in entries.flatten().enumerate() {
            let info =
                std::fs::read_to_string(entry.path().join("information")).unwrap_or_default();
            let name = info
                .lines()
                .find_map(|l| l.strip_prefix("Model:").map(|v| v.trim().to_owned()))
                .unwrap_or_else(|| format!("NVIDIA GPU {idx}"));
            let uuid =
                info.lines().find_map(|l| l.strip_prefix("GPU UUID:").map(|v| v.trim().to_owned()));
            out.push(NvidiaGpu {
                index: idx as u32,
                name: name.clone(),
                uuid,
                driver_version: driver_version.clone(),
                memory_total_mib: None,
                max_enc_sessions: estimate_nvenc_sessions(&name),
            });
        }
    }
    out
}

/// Parse `nvidia-smi -q -x` XML output into a list of [`NvidiaGpu`] descriptors.
pub fn parse_nvidia_smi_xml(xml: &str) -> Vec<NvidiaGpu> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut gpus = Vec::new();
    let mut driver_version: Option<String> = None;
    let mut tag_stack: Vec<String> = Vec::new();

    let mut in_gpu = false;
    let mut current_name = String::new();
    let mut current_uuid: Option<String> = None;
    let mut current_mem_mib: Option<u64> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let tag = e.name().into_inner().to_owned();
                if tag == "gpu" {
                    in_gpu = true;
                    current_name.clear();
                    current_uuid = None;
                    current_mem_mib = None;
                }
                tag_stack.push(tag);
            }
            Ok(Event::End(e)) => {
                let tag = e.name().into_inner();
                if tag == "gpu" && in_gpu {
                    let index = gpus.len() as u32;
                    let name = if current_name.is_empty() {
                        format!("NVIDIA GPU {index}")
                    } else {
                        current_name.clone()
                    };
                    let max_enc_sessions = estimate_nvenc_sessions(&name);
                    gpus.push(NvidiaGpu {
                        index,
                        name,
                        uuid: current_uuid.take(),
                        driver_version: driver_version.clone(),
                        memory_total_mib: current_mem_mib.take(),
                        max_enc_sessions,
                    });
                    in_gpu = false;
                }
                tag_stack.pop();
            }
            Ok(Event::Text(e)) => {
                let text = e.into_inner().trim().to_owned();
                if text.is_empty() {
                    continue;
                }
                let current_tag = tag_stack.last().map(String::as_str).unwrap_or("");
                let parent_tag = tag_stack
                    .len()
                    .checked_sub(2)
                    .and_then(|i| tag_stack.get(i))
                    .map(String::as_str)
                    .unwrap_or("");

                if current_tag == "driver_version" && !in_gpu {
                    driver_version = Some(text);
                } else if in_gpu {
                    match (parent_tag, current_tag) {
                        ("gpu", "product_name") => current_name = text,
                        ("gpu", "uuid") => current_uuid = Some(text),
                        ("fb_memory_usage", "total") => {
                            if let Some(num_str) = text.split_whitespace().next()
                                && let Ok(mib) = num_str.parse::<u64>()
                            {
                                current_mem_mib = Some(mib);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    gpus
}

fn estimate_nvenc_sessions(name: &str) -> u32 {
    let upper = name.to_ascii_uppercase();
    // NVIDIA data-center / workstation GPUs (Tesla, Quadro, RTX A-series, L4, L40, A100) are unrestricted
    if upper.contains("TESLA")
        || upper.contains("QUADRO")
        || upper.contains("RTX A")
        || upper.contains("L40")
        || upper.contains("L4 ")
        || upper.contains("A100")
        || upper.contains("A10 ")
    {
        16
    } else {
        // GeForce consumer driver limit (8 concurrent sessions on modern drivers >= 550)
        8
    }
}

fn detect_dxgi_adapters() -> Vec<DxgiAdapter> {
    Vec::new()
}

fn compute_driver_fingerprint(
    profile: PlatformProfile,
    npu_profile: NpuProfile,
    drm: &[DrmDevice],
    v4l2: &[V4l2Device],
    nvidia: &[NvidiaGpu],
    hailo: &[HailoDeviceInfo],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(profile.as_str().as_bytes());
    hasher.update(b"|");
    hasher.update(npu_profile.as_str().as_bytes());
    for d in drm {
        hasher.update(b"|drm:");
        hasher.update(d.card.as_bytes());
        if let Some(v) = &d.vendor_id {
            hasher.update(v.as_bytes());
        }
        if let Some(dev) = &d.device_id {
            hasher.update(dev.as_bytes());
        }
        if let Some(drv) = &d.driver {
            hasher.update(drv.as_bytes());
        }
    }
    for v in v4l2 {
        if v.is_m2m {
            hasher.update(b"|v4l2:");
            hasher.update(v.name.as_bytes());
        }
    }
    for g in nvidia {
        hasher.update(b"|nv:");
        hasher.update(g.name.as_bytes());
        if let Some(drv) = &g.driver_version {
            hasher.update(drv.as_bytes());
        }
    }
    for h in hailo {
        hasher.update(b"|hailo:");
        hasher.update(h.device_node.as_bytes());
        hasher.update(h.architecture.as_bytes());
        if let Some(drv) = &h.driver_version {
            hasher.update(drv.as_bytes());
        }
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(16);
    for b in &digest[..8] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn write_file(root: &Path, rel: &str, content: &str) {
        let full = root.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }

    #[test]
    fn detects_pi5_with_hailo10h_fixture_sysfs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(root, "proc/device-tree/model", "Raspberry Pi 5 Model B Rev 1.1\0");
        write_file(
            root,
            "proc/cpuinfo",
            "processor\t: 0\nModel\t\t: Raspberry Pi 5 Model B Rev 1.1\nprocessor\t: 1\nprocessor\t: 2\nprocessor\t: 3\n",
        );
        write_file(root, "proc/meminfo", "MemTotal:        8249504 kB\n");
        write_file(root, "dev/h1x-0", "");
        write_file(root, "sys/module/hailo1x_pci/version", "5.4.0\n");
        write_file(root, "sys/bus/pci/devices/0001_01_00.0/vendor", "0x1e60\n");
        write_file(root, "sys/bus/pci/devices/0001_01_00.0/device", "0x45c4\n");
        write_file(root, "sys/class/video4linux/video19/name", "rpi-hevc-dec\n");

        let fp = detect_fingerprint_from_root(root, "aarch64", None);
        assert_eq!(fp.profile, PlatformProfile::Pi5);
        assert_eq!(fp.npu_profile, NpuProfile::Hailo10H);
        assert_eq!(fp.npu_profile.tops(), 40);
        assert!(fp.npu_profile.supports_genai());
        assert_eq!(fp.cpu_cores, 4);
        assert_eq!(fp.board_model.as_deref(), Some("Raspberry Pi 5 Model B Rev 1.1"));
        assert_eq!(fp.hailo_devices.len(), 1);
        assert_eq!(fp.hailo_devices[0].device_node, "/dev/h1x-0");
        assert_eq!(fp.hailo_devices[0].driver_version.as_deref(), Some("5.4.0"));
        assert_eq!(fp.v4l2_devices.len(), 1);
        assert!(fp.v4l2_devices[0].is_m2m);
    }

    #[test]
    fn detects_pi4_with_v4l2m2m_and_hailo8l() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(root, "proc/device-tree/model", "Raspberry Pi 4 Model B Rev 1.4\0");
        write_file(root, "sys/class/video4linux/video10/name", "bcm2835-codec-decode\n");
        write_file(root, "sys/class/video4linux/video11/name", "bcm2835-codec-encode\n");
        write_file(root, "dev/hailo0", "");
        write_file(root, "sys/class/hailo_chardev/hailo0/board_name", "Hailo-8L\n");
        write_file(root, "sys/module/hailo_pci/version", "4.20.0\n");

        let fp = detect_fingerprint_from_root(root, "aarch64", None);
        assert_eq!(fp.profile, PlatformProfile::Pi4);
        assert_eq!(fp.npu_profile, NpuProfile::Hailo8L);
        assert_eq!(fp.npu_profile.tops(), 13);
        assert!(!fp.npu_profile.supports_genai());
        assert_eq!(fp.v4l2_devices.iter().filter(|d| d.is_m2m).count(), 2);
    }

    #[test]
    fn detects_x86_intel_vaapi_render_node() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        write_file(root, "sys/class/drm/card0/device/vendor", "0x8086\n");
        write_file(root, "sys/class/drm/card0/device/device", "0x46d1\n");
        write_file(root, "sys/class/drm/card0/device/uevent", "DRIVER=i915\n");
        write_file(root, "dev/dri/renderD128", "");

        let fp = detect_fingerprint_from_root(root, "x86_64", None);
        assert_eq!(fp.profile, PlatformProfile::X86Intel);
        assert_eq!(fp.npu_profile, NpuProfile::None);
        assert_eq!(fp.drm_devices.len(), 1);
        assert_eq!(fp.drm_devices[0].render_node.as_deref(), Some("/dev/dri/renderD128"));
        assert_eq!(fp.drm_devices[0].driver.as_deref(), Some("i915"));
    }

    #[test]
    fn parses_nvidia_smi_xml_and_detects_x86_nvidia() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_file(root, "dev/nvidiactl", "");

        let sample_xml = r#"<?xml version="1.0" ?>
<nvidia_smi_log>
    <driver_version>550.120</driver_version>
    <gpu id="00000000:01:00.0">
        <product_name>NVIDIA GeForce RTX 4060</product_name>
        <uuid>GPU-12345678-abcd-ef00-1122-334455667788</uuid>
        <fb_memory_usage>
            <total>8188 MiB</total>
        </fb_memory_usage>
    </gpu>
</nvidia_smi_log>"#;

        let fp = detect_fingerprint_from_root(root, "x86_64", Some(sample_xml));
        assert_eq!(fp.profile, PlatformProfile::X86Nvidia);
        assert_eq!(fp.nvidia_gpus.len(), 1);
        assert_eq!(fp.nvidia_gpus[0].name, "NVIDIA GeForce RTX 4060");
        assert_eq!(fp.nvidia_gpus[0].driver_version.as_deref(), Some("550.120"));
        assert_eq!(fp.nvidia_gpus[0].memory_total_mib, Some(8188));
        assert_eq!(fp.nvidia_gpus[0].max_enc_sessions, 8);
    }
}
