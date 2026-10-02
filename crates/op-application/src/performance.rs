//! Performance profiles (Preferences > Performance) and the hardware check that recommends one.
//!
//! A profile is a set of choices that trade looks and preview quality for speed: interface
//! animations, how often the interface redraws, preview resolution, how much decoded video is
//! kept in memory, how far ahead playback decodes, and timeline thumbnails and waveforms. The
//! lowest profile is meant for old computers without a graphics card; the highest one spends
//! resources freely on a smooth, fully animated interface. Exports are never affected.

use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Profile {
    UltraPerformance,
    Performance,
    Balanced,
    Quality,
    Maximum,
}

impl Profile {
    pub const ALL: [Profile; 5] = [
        Profile::UltraPerformance,
        Profile::Performance,
        Profile::Balanced,
        Profile::Quality,
        Profile::Maximum,
    ];

    pub fn index(self) -> u8 {
        self as u8
    }

    pub fn from_index(i: u8) -> Profile {
        Self::ALL[(i as usize).min(4)]
    }

    pub fn label(self) -> &'static str {
        match self {
            Profile::UltraPerformance => "Ultra Performance",
            Profile::Performance => "Performance",
            Profile::Balanced => "Balanced",
            Profile::Quality => "Quality",
            Profile::Maximum => "Maximum Quality",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Profile::UltraPerformance => {
                "For very old computers or ones without a graphics card: no animations, quarter-resolution playback, no timeline thumbnails or waveforms and the fewest redraws."
            }
            Profile::Performance => {
                "For modest computers and laptops on battery: no animations, half-resolution playback and fewer redraws."
            }
            Profile::Balanced => {
                "Smooth and light: short animations, half-resolution playback and full resolution when paused."
            }
            Profile::Quality => {
                "For capable computers: full animations and full-resolution playback."
            }
            Profile::Maximum => {
                "Everything on: richer animations, redraws at the display's refresh rate, a large frame cache and long read-ahead."
            }
        }
    }

    pub fn settings(self) -> ProfileSettings {
        use Profile::*;
        match self {
            UltraPerformance => ProfileSettings {
                animation_time: 0.0,
                smooth_scroll: false,
                playback_fps_cap: Some(24),
                busy_repaint: Duration::from_millis(250),
                playback_resolution: 4,
                paused_resolution: 2,
                frame_cache_mb: 384,
                read_ahead: 6,
                frame_wait: Duration::ZERO,
                thumbnails: false,
                waveforms: false,
            },
            Performance => ProfileSettings {
                animation_time: 0.0,
                smooth_scroll: false,
                playback_fps_cap: Some(30),
                busy_repaint: Duration::from_millis(150),
                playback_resolution: 2,
                paused_resolution: 1,
                frame_cache_mb: 768,
                read_ahead: 8,
                frame_wait: Duration::from_millis(3),
                thumbnails: true,
                waveforms: true,
            },
            Balanced => ProfileSettings {
                animation_time: 0.06,
                smooth_scroll: true,
                playback_fps_cap: Some(60),
                busy_repaint: Duration::from_millis(80),
                playback_resolution: 2,
                paused_resolution: 1,
                frame_cache_mb: 1536,
                read_ahead: 12,
                frame_wait: Duration::from_millis(6),
                thumbnails: true,
                waveforms: true,
            },
            Quality => ProfileSettings {
                animation_time: 1.0 / 12.0,
                smooth_scroll: true,
                playback_fps_cap: Some(60),
                busy_repaint: Duration::from_millis(60),
                playback_resolution: 1,
                paused_resolution: 1,
                frame_cache_mb: 2048,
                read_ahead: 12,
                frame_wait: Duration::from_millis(8),
                thumbnails: true,
                waveforms: true,
            },
            Maximum => ProfileSettings {
                animation_time: 0.12,
                smooth_scroll: true,
                playback_fps_cap: None,
                busy_repaint: Duration::from_millis(33),
                playback_resolution: 1,
                paused_resolution: 1,
                frame_cache_mb: 4096,
                read_ahead: 24,
                frame_wait: Duration::from_millis(10),
                thumbnails: true,
                waveforms: true,
            },
        }
    }
}

/// What a profile sets.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ProfileSettings {
    /// Interface animation length in seconds (0 turns animations off).
    pub animation_time: f32,
    pub smooth_scroll: bool,
    /// Highest interface redraw rate during playback (None: every frame of the display).
    pub playback_fps_cap: Option<u32>,
    /// Redraw interval while background work runs.
    pub busy_repaint: Duration,
    /// Preview resolution divisors.
    pub playback_resolution: u32,
    pub paused_resolution: u32,
    /// Decoded frame cache (limited to a quarter of the memory).
    pub frame_cache_mb: usize,
    /// Frames decoded ahead while playing.
    pub read_ahead: i64,
    /// How long playback waits for a late frame before showing the nearest one.
    pub frame_wait: Duration,
    /// Timeline clip thumbnails and audio waveforms.
    pub thumbnails: bool,
    pub waveforms: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuKind {
    Discrete,
    Integrated,
    /// Drawn by the processor (no usable graphics card, or a basic display driver).
    Software,
    Unknown,
}

/// What the hardware check found.
#[derive(Clone, Debug, PartialEq)]
pub struct Hardware {
    pub cpu: String,
    pub cores: usize,
    pub threads: usize,
    pub memory_gb: f64,
    pub gpu: String,
    pub gpu_kind: GpuKind,
}

impl Hardware {
    /// Probes the processor and memory; the graphics card comes from the renderer.
    pub fn probe(gpu: &str, gpu_kind: GpuKind) -> Hardware {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        sys.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let cores = sysinfo::System::physical_core_count().unwrap_or(threads.div_ceil(2).max(1));
        let cpu = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|b| !b.is_empty())
            .unwrap_or_else(|| "Unknown processor".into());
        Hardware {
            cpu,
            cores,
            threads,
            memory_gb: sys.total_memory() as f64 / (1u64 << 30) as f64,
            gpu: gpu.to_string(),
            gpu_kind,
        }
    }

    /// The profile this computer handles comfortably.
    pub fn recommended(&self) -> Profile {
        let gpu = match self.gpu_kind {
            GpuKind::Discrete => 3,
            GpuKind::Integrated | GpuKind::Unknown => 1,
            GpuKind::Software => -10,
        };
        let cpu = match self.cores {
            0..=2 => -2,
            3..=5 => 0,
            6..=7 => 1,
            8..=11 => 2,
            _ => 3,
        };
        let mem = if self.memory_gb < 5.5 {
            -3
        } else if self.memory_gb < 7.5 {
            -1
        } else if self.memory_gb < 15.0 {
            0
        } else if self.memory_gb < 30.0 {
            1
        } else {
            2
        };
        match gpu + cpu + mem {
            i32::MIN..=-1 => Profile::UltraPerformance,
            0..=2 => Profile::Performance,
            3..=4 => Profile::Balanced,
            5..=7 => Profile::Quality,
            _ => Profile::Maximum,
        }
    }

    /// The frame cache a profile gets on this computer (at most a quarter of the memory).
    pub fn frame_cache_mb(&self, p: Profile) -> usize {
        let quarter = (self.memory_gb * 1024.0 / 4.0) as usize;
        p.settings().frame_cache_mb.min(quarter.max(256))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hw(gpu_kind: GpuKind, cores: usize, memory_gb: f64) -> Hardware {
        Hardware {
            cpu: "cpu".into(),
            cores,
            threads: cores * 2,
            memory_gb,
            gpu: "gpu".into(),
            gpu_kind,
        }
    }

    #[test]
    fn recommendations_follow_the_hardware() {
        use GpuKind::*;
        // no graphics card at all
        assert_eq!(
            hw(Software, 8, 16.0).recommended(),
            Profile::UltraPerformance
        );
        // an old dual-core laptop with 4 GB
        assert_eq!(
            hw(Integrated, 2, 4.0).recommended(),
            Profile::UltraPerformance
        );
        // a typical office laptop
        assert_eq!(hw(Integrated, 4, 8.0).recommended(), Profile::Performance);
        assert_eq!(hw(Integrated, 6, 16.0).recommended(), Profile::Balanced);
        // an older gaming PC (RX 570 class, four cores)
        assert_eq!(hw(Discrete, 4, 8.0).recommended(), Profile::Balanced);
        assert_eq!(hw(Discrete, 6, 16.0).recommended(), Profile::Quality);
        // a workstation
        assert_eq!(hw(Discrete, 16, 64.0).recommended(), Profile::Maximum);
    }

    #[test]
    fn profiles_get_lighter_towards_ultra() {
        let a = Profile::UltraPerformance.settings();
        let m = Profile::Maximum.settings();
        assert!(a.animation_time == 0.0 && m.animation_time > 0.0);
        assert!(a.playback_resolution > m.playback_resolution);
        assert!(a.frame_cache_mb < m.frame_cache_mb);
        assert!(!a.thumbnails && m.thumbnails);
        for p in Profile::ALL {
            assert_eq!(Profile::from_index(p.index()), p);
        }
        // the cache never takes more than a quarter of a small machine's memory
        assert_eq!(
            hw(GpuKind::Discrete, 8, 4.0).frame_cache_mb(Profile::Maximum),
            1024
        );
    }
}
