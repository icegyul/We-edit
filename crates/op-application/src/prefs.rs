//! User preferences, stored as JSON in the configuration folder.

use std::path::{Path, PathBuf};

use op_core::{MediaScaling, SequenceSettings, TimeDisplay};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// UI language code ("en", "es"); empty follows the system.
    pub language: String,
    pub autosave_minutes: u32,
    pub autosave_keep: usize,
    pub recent: Vec<PathBuf>,
    pub still_seconds: f64,
    pub video_transition_seconds: f64,
    pub audio_transition_seconds: f64,
    pub media_scaling: MediaScaling,
    pub default_sequence: SequenceSettings,
    pub time_display: TimeDisplay,
    /// Preview resolution divisor while playing (1 full, 2 half, 4 quarter).
    pub playback_resolution: u32,
    pub paused_resolution: u32,
    /// Decoded frame cache in megabytes.
    pub frame_cache_mb: usize,
    pub snapping: bool,
    pub linked_selection: bool,
    pub ripple_markers: bool,
    /// Custom key bindings: (context, command) -> keys; an empty string unbinds.
    pub shortcuts: Vec<(String, String, String)>,
    pub ui_scale: f32,
    pub audio_scrubbing: bool,
    pub hardware_encoding: bool,
    pub last_export_dir: Option<PathBuf>,
    pub last_import_dir: Option<PathBuf>,
    /// Write the program log to a file (errors are always kept in memory).
    pub logging_enabled: bool,
    /// Log detail: "error", "warn", "info", "debug" or "trace".
    pub log_level: String,
    /// Share of Effect Controls given to the parameters; the keyframe timeline gets the rest.
    pub effect_controls_split: f32,
    /// Effect Controls shows its keyframe timeline.
    pub effect_controls_timeline: bool,
    /// Folder for images pasted from the clipboard.
    pub paste_folder: Option<PathBuf>,
    /// Save pasted images in `paste_folder` without asking.
    pub paste_always: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            language: String::new(),
            autosave_minutes: 5,
            autosave_keep: 10,
            recent: Vec::new(),
            still_seconds: 5.0,
            video_transition_seconds: 1.0,
            audio_transition_seconds: 1.0,
            media_scaling: MediaScaling::None,
            default_sequence: SequenceSettings::default(),
            time_display: TimeDisplay::Timecode,
            playback_resolution: 2,
            paused_resolution: 1,
            frame_cache_mb: 1536,
            snapping: true,
            linked_selection: true,
            ripple_markers: true,
            shortcuts: Vec::new(),
            ui_scale: 1.0,
            audio_scrubbing: true,
            hardware_encoding: true,
            last_export_dir: None,
            last_import_dir: None,
            logging_enabled: true,
            log_level: "info".into(),
            effect_controls_split: 0.62,
            effect_controls_timeline: true,
            paste_folder: None,
            paste_always: false,
        }
    }
}

/// Folders used by the application.
#[derive(Clone, Debug)]
pub struct Dirs {
    pub config: PathBuf,
    pub cache: PathBuf,
    pub data: PathBuf,
}

impl Dirs {
    pub fn system() -> Dirs {
        let base = |d: Option<PathBuf>| d.unwrap_or_else(std::env::temp_dir).join("OpenPremier");
        Dirs {
            config: base(dirs::config_dir()),
            cache: base(dirs::cache_dir()),
            data: base(dirs::data_dir()),
        }
    }

    /// Everything under one folder (portable mode, tests).
    pub fn portable(root: &Path) -> Dirs {
        Dirs {
            config: root.join("config"),
            cache: root.join("cache"),
            data: root.join("data"),
        }
    }

    pub fn autosave(&self) -> PathBuf {
        self.data.join("Autosave")
    }

    pub fn workspaces(&self) -> PathBuf {
        self.config.join("workspaces")
    }

    pub fn logs(&self) -> PathBuf {
        self.data.join("Logs")
    }
}

impl Preferences {
    pub fn load(dirs: &Dirs) -> Preferences {
        std::fs::read_to_string(dirs.config.join("preferences.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Writes the preferences atomically (a crash while saving keeps the previous file).
    pub fn save(&self, dirs: &Dirs) -> std::io::Result<()> {
        std::fs::create_dir_all(&dirs.config)?;
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let path = dirs.config.join("preferences.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &path)
    }

    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(12);
    }
}
