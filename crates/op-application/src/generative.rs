//! What this PC can run for generative AI: the graphics card and its memory, system memory and
//! free disk space, and a verdict for each local model (see `comfy_packs`).

use std::path::Path;

pub struct Gpu {
    pub name: String,
    /// Video memory in MB, when the driver tells (NVIDIA cards).
    pub vram_mb: Option<u64>,
}

/// One line of `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`.
pub fn parse_gpu(line: &str) -> Option<Gpu> {
    let (name, mem) = line.trim().rsplit_once(',')?;
    let name = name.trim();
    (!name.is_empty()).then(|| Gpu { name: name.to_string(), vram_mb: mem.trim().parse().ok() })
}

/// The first NVIDIA graphics card, asked of the driver's own tool; None without one.
pub fn detect_gpu() -> Option<Gpu> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    String::from_utf8_lossy(&out.stdout).lines().find_map(parse_gpu)
}

/// What the speech helper says about the graphics card: (CUDA usable, number of cards). None
/// without the helper. Starts Python (about a second): call it off the interface thread.
pub fn helper_cuda() -> Option<(bool, usize)> {
    let (python, script) = crate::sentences::helper_files()?;
    let mut cmd = std::process::Command::new(python);
    cmd.arg(script).arg("--check");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok().filter(|o| o.status.success())?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    Some((json["cuda"].as_bool().unwrap_or(false), json["devices"].as_u64().unwrap_or(0) as usize))
}

pub fn memory_gb() -> f64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    sys.total_memory() as f64 / (1u64 << 30) as f64
}

/// Free space, in GB, of the disk that holds `path`.
pub fn free_disk_gb(path: &Path) -> Option<f64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space() as f64 / 1e9)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// The card has the memory the model wants.
    Good,
    /// Less than wanted: it may be slow or run out of memory.
    Tight,
    /// Far too little memory.
    No,
    /// The card's memory is not known.
    Unknown,
}

pub fn verdict(need_gb: u32, gpu: Option<&Gpu>) -> Verdict {
    let Some(mb) = gpu.and_then(|g| g.vram_mb) else {
        return Verdict::Unknown;
    };
    let have = mb as f64 / 1024.0;
    let need = f64::from(need_gb);
    if have >= need * 0.99 {
        Verdict::Good
    } else if have >= need * 0.6 {
        Verdict::Tight
    } else {
        Verdict::No
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_driver_line_is_read() {
        let g = parse_gpu("NVIDIA GeForce RTX 3070, 8192\n").unwrap();
        assert_eq!(g.name, "NVIDIA GeForce RTX 3070");
        assert_eq!(g.vram_mb, Some(8192));
        assert!(parse_gpu("").is_none());
        assert_eq!(parse_gpu("Some Card, [N/A]").unwrap().vram_mb, None);
    }

    #[test]
    fn a_verdict_follows_the_memory() {
        let card = |mb| Gpu { name: "x".into(), vram_mb: Some(mb) };
        assert_eq!(verdict(8, Some(&card(8192))), Verdict::Good);
        assert_eq!(verdict(12, Some(&card(8192))), Verdict::Tight);
        assert_eq!(verdict(24, Some(&card(8192))), Verdict::No);
        assert_eq!(verdict(8, None), Verdict::Unknown);
        assert_eq!(verdict(8, Some(&Gpu { name: "x".into(), vram_mb: None })), Verdict::Unknown);
    }

    #[test]
    fn the_disk_of_a_path_is_found() {
        let here = std::env::current_dir().unwrap();
        assert!(free_disk_gb(&here).is_some_and(|g| g > 0.0));
    }
}
