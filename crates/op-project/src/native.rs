//! The native project file (`.opproj`): versioned JSON.
//!
//! Saving is atomic (PR-SAVE-005 applied to our own format): the data goes to a sibling temporary
//! file, is flushed and parsed back, the previous file is kept as `.bak`, and only then is the
//! temporary file moved over the target. Media paths are stored absolute and relative to the
//! project file, so a project folder can move to another disk or computer.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component as PathPart, Path, PathBuf};
use std::sync::Arc;

use op_core::*;
use serde::{Deserialize, Serialize};

use crate::ProjectError;

pub const EXTENSION: &str = "opproj";
const FORMAT: &str = "openpremier-project";

#[derive(Serialize, Deserialize)]
struct FileData {
    format: String,
    version: u32,
    generator: String,
    project: Project,
    /// Asset -> path relative to the project folder.
    #[serde(default)]
    relative_paths: BTreeMap<u64, String>,
    /// Application view state (open sequences, panels); opaque to this crate.
    #[serde(default)]
    view: serde_json::Value,
}

/// A loaded project.
#[derive(Debug)]
pub struct Loaded {
    pub project: Project,
    pub view: serde_json::Value,
    /// Assets found through their relative path after the absolute one went missing.
    pub relinked: Vec<AssetId>,
    /// Assets whose files are missing (offline).
    pub offline: Vec<AssetId>,
}

/// Serializes a project. `base` is the folder the relative paths are computed from.
pub fn to_string(
    project: &Project,
    view: &serde_json::Value,
    base: Option<&Path>,
    generator: &str,
) -> Result<String, ProjectError> {
    let mut relative_paths = BTreeMap::new();
    if let Some(base) = base {
        for (id, a) in &project.assets {
            if let Some(rel) = relative(base, Path::new(&a.path)) {
                relative_paths.insert(id.0, rel);
            }
        }
    }
    let data = FileData {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        generator: generator.into(),
        project: project.clone(),
        relative_paths,
        view: view.clone(),
    };
    serde_json::to_string_pretty(&data).map_err(|e| ProjectError::Format(e.to_string()))
}

/// Parses a project file's text. `base` resolves relative media paths.
pub fn from_str(text: &str, base: Option<&Path>) -> Result<Loaded, ProjectError> {
    let head: serde_json::Value =
        serde_json::from_str(text).map_err(|e| ProjectError::Format(e.to_string()))?;
    if head.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(ProjectError::Format("not an OpenPremier project".into()));
    }
    let version = head.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version == 0 || version > FORMAT_VERSION {
        return Err(ProjectError::Version(version));
    }
    let data: FileData =
        serde_json::from_value(head).map_err(|e| ProjectError::Format(e.to_string()))?;
    let mut project = data.project;
    project.format_version = FORMAT_VERSION;
    project.refresh_ids();
    complete_components(&mut project);
    project.validate().map_err(ProjectError::Invalid)?;

    let mut relinked = Vec::new();
    let mut offline = Vec::new();
    let ids: Vec<AssetId> = project.assets.keys().copied().collect();
    for id in ids {
        let path = project.asset(id).unwrap().path.clone();
        if Path::new(&path).exists() {
            continue;
        }
        let candidate = base
            .zip(data.relative_paths.get(&id.0))
            .map(|(b, rel)| b.join(rel));
        match candidate.filter(|c| c.exists()) {
            Some(found) => {
                let asset = Arc::make_mut(project.assets.get_mut(&id).unwrap());
                asset.path = found.to_string_lossy().into_owned();
                relinked.push(id);
            }
            None => offline.push(id),
        }
    }
    Ok(Loaded {
        project,
        view: data.view,
        relinked,
        offline,
    })
}

/// Parameters added to effects in newer versions get their defaults.
fn complete_components(project: &mut Project) {
    let ids: Vec<SequenceId> = project.sequences.keys().copied().collect();
    for sid in ids {
        let seq = project.sequence_mut(sid).unwrap();
        for kind in [TrackKind::Video, TrackKind::Audio] {
            for t in seq.tracks_mut(kind) {
                let incomplete = t
                    .clips
                    .iter()
                    .any(|c| c.components.iter().any(needs_params));
                if incomplete {
                    for c in &mut Arc::make_mut(t).clips {
                        for comp in &mut c.components {
                            comp.complete_params();
                        }
                    }
                }
            }
        }
    }
}

fn needs_params(c: &Component) -> bool {
    c.def()
        .is_some_and(|d| d.params.iter().any(|p| c.param(p.key).is_none()))
}

/// Saves atomically, keeping the previous version as `<file>.bak`.
pub fn save(
    path: &Path,
    project: &Project,
    view: &serde_json::Value,
    generator: &str,
) -> Result<(), ProjectError> {
    let base = path.parent();
    let text = to_string(project, view, base, generator)?;
    write_atomic(path, text.as_bytes(), |bytes| {
        let s = std::str::from_utf8(bytes).map_err(|e| ProjectError::Format(e.to_string()))?;
        from_str(s, None).map(|_| ())
    })
}

pub fn load(path: &Path) -> Result<Loaded, ProjectError> {
    let text = fs::read_to_string(path)?;
    from_str(&text, path.parent())
}

/// Writes through a verified temporary file and keeps a backup of the previous version.
pub fn write_atomic(
    path: &Path,
    bytes: &[u8],
    verify: impl Fn(&[u8]) -> Result<(), ProjectError>,
) -> Result<(), ProjectError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| ProjectError::Format("no file name".into()))?
        .to_string_lossy()
        .into_owned();
    let tmp = path.with_file_name(format!(".{file_name}.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    let written = fs::read(&tmp)?;
    if let Err(e) = verify(&written) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if path.exists() {
        let bak = path.with_file_name(format!("{file_name}.bak"));
        fs::copy(path, &bak)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Path of `target` relative to `base`, when both are on the same root.
pub fn relative(base: &Path, target: &Path) -> Option<String> {
    let base: Vec<PathPart> = base.components().collect();
    let target: Vec<PathPart> = target.components().collect();
    let same_root =
        matches!((base.first(), target.first()), (Some(a), Some(b)) if norm(a) == norm(b));
    if !same_root {
        return None;
    }
    let common = base
        .iter()
        .zip(&target)
        .take_while(|(a, b)| norm(a) == norm(b))
        .count();
    let mut out = PathBuf::new();
    for _ in common..base.len() {
        out.push("..");
    }
    for c in &target[common..] {
        out.push(c.as_os_str());
    }
    Some(out.to_string_lossy().replace('\\', "/"))
}

fn norm(c: &PathPart) -> String {
    let s = c.as_os_str().to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_asset(path: &str) -> Project {
        let mut p = Project::new("demo");
        let asset = MediaAsset {
            id: AssetId(0),
            path: path.into(),
            proxy: None,
            kind: MediaKind::Audio,
            video: None,
            audio: vec![AudioStream {
                index: 0,
                codec: "pcm_s16le".into(),
                sample_rate: 48000,
                layout: ChannelLayout::Stereo,
                samples: 48000,
                start: Dur::ZERO,
            }],
            duration: Dur::from_seconds(1.0),
            interpretation: Interpretation::default(),
            file_size: 0,
            modified_unix: 0,
        };
        p.add_asset(p.root, asset);
        p.add_sequence(p.root, "Sequence 01", SequenceSettings::default());
        p
    }

    #[test]
    fn save_load_round_trip_with_backup() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media").join("tone.wav");
        fs::create_dir_all(media.parent().unwrap()).unwrap();
        fs::write(&media, b"x").unwrap();
        let p = project_with_asset(&media.to_string_lossy());
        let file = dir.path().join("proj").join("demo.opproj");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        let view = serde_json::json!({"open": [1]});
        save(&file, &p, &view, "test").unwrap();
        save(&file, &p, &view, "test").unwrap();
        assert!(file.with_file_name("demo.opproj.bak").exists());
        let loaded = load(&file).unwrap();
        assert_eq!(loaded.project, p);
        assert_eq!(loaded.view, view);
        assert!(loaded.offline.is_empty());
    }

    #[test]
    fn moved_folders_relink_through_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old");
        fs::create_dir_all(old.join("media")).unwrap();
        fs::write(old.join("media").join("tone.wav"), b"x").unwrap();
        let p = project_with_asset(&old.join("media").join("tone.wav").to_string_lossy());
        save(
            &old.join("demo.opproj"),
            &p,
            &serde_json::Value::Null,
            "test",
        )
        .unwrap();
        let new = dir.path().join("new");
        fs::rename(&old, &new).unwrap();
        let loaded = load(&new.join("demo.opproj")).unwrap();
        assert_eq!(loaded.relinked.len(), 1);
        let asset = loaded.project.assets.values().next().unwrap();
        assert!(Path::new(&asset.path).exists());
    }

    #[test]
    fn foreign_or_future_files_are_refused() {
        assert!(matches!(
            from_str("{\"format\":\"x\"}", None),
            Err(ProjectError::Format(_))
        ));
        let future = format!("{{\"format\":\"{FORMAT}\",\"version\":999}}");
        assert!(matches!(
            from_str(&future, None),
            Err(ProjectError::Version(999))
        ));
    }

    #[test]
    fn relative_paths() {
        let base = Path::new("/a/b/proj");
        assert_eq!(
            relative(base, Path::new("/a/b/media/x.mov")).as_deref(),
            Some("../media/x.mov")
        );
        assert_eq!(
            relative(base, Path::new("/a/b/proj/x.mov")).as_deref(),
            Some("x.mov")
        );
    }
}
