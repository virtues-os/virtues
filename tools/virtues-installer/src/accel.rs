//! What accelerators this machine has, read from the kernel's own view of it.
//!
//! The installer runs search on the CPU by default (the bundled sidecars,
//! identical on every machine) and never installs GPU/NPU inference software
//! itself. What it does do is notice an accelerator and send the owner to the
//! guide for it, because search and the first index are much faster there.
//! Detection is therefore advisory: a miss costs a nudge, never an install.
//!
//! Everything is read from sysfs/procfs/devfs under a root path, so tests can
//! build a fake tree. No vendor tools are executed.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accelerator {
    /// What the owner sees, e.g. "NVIDIA GeForce RTX 4060".
    pub label: String,
    /// The family, which picks the guide section.
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Nvidia,
    Amd,
    Intel,
    /// Arm Mali, Qualcomm Adreno and other GPUs llama.cpp reaches via Vulkan.
    OtherGpu,
    /// NPUs: Intel, AMD Ryzen AI, Rockchip, Hailo, Qualcomm Hexagon.
    Npu,
}

impl Kind {
    /// The section of https://virtues.com/docs/setup/accelerators for it.
    pub fn guide_url(self) -> &'static str {
        match self {
            Kind::Nvidia => "https://virtues.com/docs/setup/accelerators#nvidia-gpus",
            Kind::Amd => "https://virtues.com/docs/setup/accelerators#amd-gpus",
            Kind::Intel => "https://virtues.com/docs/setup/accelerators#intel-gpus-and-npus",
            Kind::OtherGpu => "https://virtues.com/docs/setup/accelerators#other-gpus",
            Kind::Npu => "https://virtues.com/docs/setup/accelerators#npus",
        }
    }
}

/// Accelerators on this machine, most useful first.
pub fn detect() -> Vec<Accelerator> {
    detect_in(Path::new("/"))
}

fn detect_in(root: &Path) -> Vec<Accelerator> {
    let p = |rel: &str| root.join(rel.trim_start_matches('/'));
    let mut found: Vec<Accelerator> = Vec::new();
    let mut push = |a: Accelerator| {
        if !found.contains(&a) {
            found.push(a);
        }
    };

    // NVIDIA's proprietary driver lists each GPU with its model name.
    for info in glob_files(&p("/proc/driver/nvidia/gpus"), "information") {
        let model = std::fs::read_to_string(&info)
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("Model:").map(|m| m.trim().to_string()))
            })
            .unwrap_or_else(|| "NVIDIA GPU".to_string());
        let label = if model.to_lowercase().starts_with("nvidia") { model } else { format!("NVIDIA {model}") };
        push(Accelerator { label, kind: Kind::Nvidia });
    }
    // Jetson boards: CUDA through the Tegra driver, no /proc/driver/nvidia.
    if p("/etc/nv_tegra_release").exists() || compatible(root).contains("nvidia,tegra") {
        push(Accelerator { label: "NVIDIA Jetson".to_string(), kind: Kind::Nvidia });
    }

    // Every other GPU shows up as a DRM card with a kernel driver.
    for card in dir_entries(&p("/sys/class/drm")) {
        let name = card.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !name.starts_with("card") || name.contains('-') {
            continue; // connectors (card0-HDMI-A-1) repeat the card
        }
        let driver = std::fs::read_link(card.join("device/driver"))
            .ok()
            .and_then(|l| l.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();
        // Integrated graphics share the CPU's memory and rarely beat it at
        // embedding, so they don't earn a nudge: nearly every Intel or AMD mini
        // PC has one. An AMD card counts when it has its own memory (an APU's
        // is a small carve-out), an Intel one when it isn't the CPU's built-in
        // device, which always sits at PCI 00:02.0.
        let device = std::fs::canonicalize(card.join("device")).unwrap_or_else(|_| card.join("device"));
        let pci = device.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let (label, kind) = match driver.as_str() {
            "amdgpu" if vram_bytes(&card) >= 2 << 30 => ("AMD Radeon GPU", Kind::Amd),
            "i915" | "xe" if !pci.starts_with("0000:00:") => ("Intel Arc GPU", Kind::Intel),
            // An NVIDIA card on the open-source driver: no compute until the
            // proprietary driver goes in, which the guide's first step covers.
            "nouveau" => ("NVIDIA GPU", Kind::Nvidia),
            "panfrost" | "panthor" | "mali" | "mali_kbase" => ("Arm Mali GPU", Kind::OtherGpu),
            "msm" | "msm_drm" => ("Qualcomm Adreno GPU", Kind::OtherGpu),
            // nvidia is covered above. vc4/v3d (Raspberry Pi), virtual and
            // display-only drivers can't run models.
            _ => continue,
        };
        push(Accelerator { label: label.to_string(), kind });
    }

    // NPUs. Compute accelerators register under /sys/class/accel.
    for dev in dir_entries(&p("/sys/class/accel")) {
        let driver = std::fs::read_link(dev.join("device/driver"))
            .ok()
            .and_then(|l| l.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();
        let label = match driver.as_str() {
            "intel_vpu" => "Intel NPU",
            "amdxdna" => "AMD Ryzen AI NPU",
            "qaic" => "Qualcomm Cloud AI accelerator",
            _ => continue,
        };
        let kind = if driver == "intel_vpu" { Kind::Intel } else { Kind::Npu };
        push(Accelerator { label: label.to_string(), kind });
    }
    if p("/dev/rknpu").exists() || compatible(root).contains("rockchip,rk3588") {
        push(Accelerator { label: "Rockchip NPU".to_string(), kind: Kind::Npu });
    }
    if p("/dev/hailo0").exists() {
        push(Accelerator { label: "Hailo NPU".to_string(), kind: Kind::Npu });
    }
    if p("/dev/fastrpc-cdsp").exists() {
        push(Accelerator { label: "Qualcomm Hexagon NPU".to_string(), kind: Kind::Npu });
    }

    found.sort_by_key(|a| match a.kind {
        Kind::Nvidia => 0,
        Kind::Amd => 1,
        Kind::Intel => 2,
        Kind::Npu => 3,
        Kind::OtherGpu => 4,
    });
    found
}

/// Dedicated video memory an amdgpu card reports, or 0.
fn vram_bytes(card: &Path) -> u64 {
    std::fs::read_to_string(card.join("device/mem_info_vram_total"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn compatible(root: &Path) -> String {
    std::fs::read(root.join("proc/device-tree/compatible"))
        .map(|b| String::from_utf8_lossy(&b).replace('\0', " ").to_lowercase())
        .unwrap_or_default()
}

fn dir_entries(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// `<dir>/*/<file>` that exist.
fn glob_files(dir: &Path, file: &str) -> Vec<PathBuf> {
    dir_entries(dir).into_iter().map(|d| d.join(file)).filter(|f| f.is_file()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn card(root: &Path, n: &str, driver: &str, pci: &str) {
        let dev = root.join(format!("sys/devices/pci0000:00/{pci}"));
        fs::create_dir_all(&dev).unwrap();
        fs::create_dir_all(root.join(format!("sys/bus/pci/drivers/{driver}"))).unwrap();
        std::os::unix::fs::symlink(root.join(format!("sys/bus/pci/drivers/{driver}")), dev.join("driver")).unwrap();
        fs::create_dir_all(root.join(format!("sys/class/drm/{n}"))).unwrap();
        std::os::unix::fs::symlink(&dev, root.join(format!("sys/class/drm/{n}/device"))).unwrap();
    }

    fn vram(root: &Path, pci: &str, bytes: u64) {
        fs::write(root.join(format!("sys/devices/pci0000:00/{pci}/mem_info_vram_total")), bytes.to_string())
            .unwrap();
    }

    #[test]
    fn bare_machine_has_none() {
        let t = tree();
        assert!(detect_in(t.path()).is_empty());
    }

    #[test]
    fn nvidia_reads_the_model_name() {
        let t = tree();
        let g = t.path().join("proc/driver/nvidia/gpus/0000:01:00.0");
        fs::create_dir_all(&g).unwrap();
        fs::write(g.join("information"), "Model: \t\t NVIDIA GeForce RTX 4060\nIRQ: 1\n").unwrap();
        let a = detect_in(t.path());
        assert_eq!(a, vec![Accelerator { label: "NVIDIA GeForce RTX 4060".into(), kind: Kind::Nvidia }]);
    }

    #[test]
    fn drm_drivers_map_to_families_and_skip_display_only() {
        let t = tree();
        card(t.path(), "card0", "amdgpu", "0000:03:00.0");
        vram(t.path(), "0000:03:00.0", 8 << 30);
        card(t.path(), "card1", "vc4", "0000:04:00.0");
        card(t.path(), "card2", "xe", "0000:05:00.0");
        card(t.path(), "card3", "nouveau", "0000:06:00.0");
        fs::create_dir_all(t.path().join("sys/class/drm/card0-HDMI-A-1")).unwrap();
        let kinds: Vec<Kind> = detect_in(t.path()).into_iter().map(|a| a.kind).collect();
        assert_eq!(kinds, vec![Kind::Nvidia, Kind::Amd, Kind::Intel]);
    }

    /// A mini PC's built-in graphics: Intel's at 00:02.0, an AMD APU with a
    /// small memory carve-out. Neither gets the nudge.
    #[test]
    fn integrated_graphics_are_not_accelerators() {
        let t = tree();
        card(t.path(), "card0", "i915", "0000:00:02.0");
        card(t.path(), "card1", "amdgpu", "0000:c4:00.0");
        vram(t.path(), "0000:c4:00.0", 512 << 20);
        assert!(detect_in(t.path()).is_empty());
    }

    #[test]
    fn rockchip_npu_from_device_tree() {
        let t = tree();
        fs::create_dir_all(t.path().join("proc/device-tree")).unwrap();
        fs::write(t.path().join("proc/device-tree/compatible"), b"radxa,rock-5b\0rockchip,rk3588\0").unwrap();
        assert_eq!(detect_in(t.path())[0].kind, Kind::Npu);
    }

    #[test]
    fn every_kind_has_a_guide_anchor() {
        for k in [Kind::Nvidia, Kind::Amd, Kind::Intel, Kind::OtherGpu, Kind::Npu] {
            assert!(k.guide_url().starts_with("https://virtues.com/docs/setup/accelerators#"));
        }
    }
}
