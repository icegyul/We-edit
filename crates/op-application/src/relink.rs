//! Missing media (File > Link Media): find offline files again, by hand or by searching this
//! computer, and relink them all in one step.
//!
//! The search walks likely folders first (the project's folder and its parents, the folders of
//! media already found, the user's media folders) and then each drive, skipping system folders,
//! matching file names without regard to case. When a name appears more than once, the candidate
//! whose duration and picture size match the original wins.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use op_core::*;
use parking_lot::Mutex;

use crate::editor::Editor;

/// An offline asset: its id, file name and the path it had.
#[derive(Clone, Debug, PartialEq)]
pub struct Missing {
    pub asset: AssetId,
    pub name: String,
    pub path: String,
}

/// Assets whose file is not where the project says.
pub fn offline(p: &Project) -> Vec<Missing> {
    let mut v: Vec<Missing> = p
        .assets
        .iter()
        .filter(|(_, a)| !a.path.is_empty() && !Path::new(&a.path).exists())
        .map(|(id, a)| Missing {
            asset: *id,
            name: a.file_name().to_string(),
            path: a.path.clone(),
        })
        .collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    v
}

/// Folders never searched: system and program folders, caches and version control.
fn skipped(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with('.')
        || n.starts_with('$')
        || matches!(
            n.as_str(),
            "windows"
                | "program files"
                | "program files (x86)"
                | "programdata"
                | "appdata"
                | "system volume information"
                | "recovery"
                | "node_modules"
                | "target"
                | "library"
                | "system"
                | "proc"
                | "sys"
                | "dev"
                | "usr"
                | "bin"
                | "sbin"
                | "etc"
                | "var"
                | "boot"
                | "snap"
                | "applications"
        )
}

/// Where to look, most likely first.
pub fn search_roots(project_file: Option<&Path>, hints: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if p.is_dir() && !roots.iter().any(|r| r == &p) {
            roots.push(p);
        }
    };
    for h in hints {
        add(h.clone());
    }
    if let Some(dir) = project_file.and_then(|f| f.parent()) {
        let mut d = Some(dir);
        for _ in 0..3 {
            if let Some(x) = d {
                add(x.to_path_buf());
                d = x.parent();
            }
        }
    }
    for d in [
        dirs::video_dir(),
        dirs::picture_dir(),
        dirs::audio_dir(),
        dirs::desktop_dir(),
        dirs::download_dir(),
        dirs::document_dir(),
        dirs::home_dir(),
    ]
    .into_iter()
    .flatten()
    {
        add(d);
    }
    if cfg!(windows) {
        for letter in b'C'..=b'Z' {
            add(PathBuf::from(format!("{}:\\", letter as char)));
        }
    } else {
        add(PathBuf::from("/media"));
        add(PathBuf::from("/mnt"));
        add(PathBuf::from("/Volumes"));
    }
    roots
}

/// Candidate paths by lower-case file name, found under `roots` (breadth first, at most
/// `max_depth` folders deep and `max_dirs` folders in all). `found` gets each new candidate as it
/// is found; the walk stops when `done` says every name is settled or when cancelled.
pub fn find_files(
    names: &HashSet<String>,
    roots: &[PathBuf],
    max_depth: usize,
    max_dirs: usize,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, &Path),
    found: &mut dyn FnMut(&str, PathBuf) -> bool,
) {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut scanned = 0usize;
    for root in roots {
        let mut queue: VecDeque<(PathBuf, usize)> = VecDeque::new();
        queue.push_back((root.clone(), 0));
        while let Some((dir, depth)) = queue.pop_front() {
            if cancel.load(Ordering::Relaxed) || scanned >= max_dirs {
                return;
            }
            let canon = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            if !seen.insert(canon) {
                continue;
            }
            scanned += 1;
            if scanned.is_multiple_of(64) {
                progress(scanned, &dir);
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    if depth < max_depth && !skipped(&name) {
                        queue.push_back((e.path(), depth + 1));
                    }
                } else if names.contains(&name.to_lowercase())
                    && !found(&name.to_lowercase(), e.path())
                {
                    return;
                }
            }
        }
    }
}

/// How well a candidate file matches the original asset (higher is better; None: not media).
fn match_score(original: &MediaAsset, candidate: &Path) -> Option<i32> {
    let probed = op_media::probe(candidate).ok()?;
    let mut score = 0;
    if (probed.duration.seconds() - original.duration.seconds()).abs() < 0.1 {
        score += 2;
    }
    match (&probed.video, &original.video) {
        (Some(a), Some(b)) if (a.width, a.height) == (b.width, b.height) => score += 1,
        (Some(_), Some(_)) => score -= 1,
        _ => {}
    }
    if probed.audio.len() == original.audio.len() {
        score += 1;
    }
    Some(score)
}

/// The best of several files with the right name for an asset.
pub fn best_candidate(original: &MediaAsset, candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .filter_map(|c| match_score(original, c).map(|s| (s, c)))
        .max_by_key(|(s, _)| *s)
        .map(|(_, c)| c.clone())
}

#[derive(Clone, Debug, Default)]
pub struct SearchState {
    pub running: bool,
    pub folders: usize,
    pub current: String,
    /// Asset -> file found for it.
    pub found: HashMap<AssetId, PathBuf>,
}

/// A search for missing files on a background thread.
pub struct Search {
    pub state: Arc<Mutex<SearchState>>,
    cancel: Arc<AtomicBool>,
}

impl Search {
    pub fn start(missing: Vec<(MediaAsset, String)>, roots: Vec<PathBuf>) -> Search {
        let state = Arc::new(Mutex::new(SearchState {
            running: true,
            ..Default::default()
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (st, c) = (state.clone(), cancel.clone());
        std::thread::Builder::new()
            .name("link-media".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let mut by_name: HashMap<String, Vec<AssetId>> = HashMap::new();
                let mut originals: HashMap<AssetId, MediaAsset> = HashMap::new();
                for (a, name) in missing {
                    by_name.entry(name.to_lowercase()).or_default().push(a.id);
                    originals.insert(a.id, a);
                }
                let names: HashSet<String> = by_name.keys().cloned().collect();
                let mut candidates: HashMap<String, Vec<PathBuf>> = HashMap::new();
                let total = originals.len();
                let mut settled: HashSet<AssetId> = HashSet::new();
                find_files(
                    &names,
                    &roots,
                    12,
                    250_000,
                    &c,
                    &mut |n, dir| {
                        let mut s = st.lock();
                        s.folders = n;
                        s.current = dir.display().to_string();
                    },
                    &mut |name, path| {
                        let list = candidates.entry(name.to_string()).or_default();
                        list.push(path);
                        for id in by_name.get(name).into_iter().flatten() {
                            if let Some(best) = best_candidate(&originals[id], list) {
                                st.lock().found.insert(*id, best);
                                // an exact match settles the asset; others keep looking
                                if match_score(&originals[id], &st.lock().found[id]).unwrap_or(0)
                                    >= 3
                                {
                                    settled.insert(*id);
                                }
                            }
                        }
                        settled.len() < total
                    },
                );
                let mut s = st.lock();
                s.running = false;
                log::info!(
                    "media search: {} of {} found, {} folders in {:.1} s",
                    s.found.len(),
                    total,
                    s.folders,
                    started.elapsed().as_secs_f32()
                );
            })
            .ok();
        Search { state, cancel }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Editor {
    /// Points assets at new files (one undo step). Each file is probed so the asset describes
    /// what is really there; interpretation settings are kept.
    pub fn link_media(&mut self, links: &[(AssetId, PathBuf)]) -> Result<usize, String> {
        let mut probed = Vec::new();
        let mut errors = Vec::new();
        for (id, path) in links {
            let Some(old) = self.project.asset(*id) else {
                continue;
            };
            match op_media::probe(path) {
                Ok(mut a) => {
                    a.id = *id;
                    a.interpretation = old.interpretation.clone();
                    a.proxy = old.proxy.clone();
                    probed.push((*id, Arc::new(a)));
                }
                Err(e) => errors.push(format!("{}: {e}", path.display())),
            }
        }
        if probed.is_empty() {
            return Err(errors.join("; "));
        }
        let n = probed.len();
        for (id, _) in &probed {
            self.media.forget(*id);
        }
        self.edit("Link Media", move |p| {
            for (id, a) in probed {
                p.assets.insert(id, a);
            }
            Ok(())
        });
        log::info!("linked {n} media files");
        if let Some(e) = errors.first() {
            self.error(format!("Not linked: {e}"));
        }
        Ok(n)
    }
}

/// Files in `folder` named like the other missing assets (after one is located by hand, the rest
/// are often beside it).
pub fn siblings(folder: &Path, missing: &[Missing]) -> Vec<(AssetId, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let files: HashMap<String, PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| (e.file_name().to_string_lossy().to_lowercase(), e.path()))
        .collect();
    missing
        .iter()
        .filter_map(|m| {
            files
                .get(&m.name.to_lowercase())
                .map(|p| (m.asset, p.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::{Dirs, Preferences};
    use op_media::{Muxer, VideoCodec, VideoSettings};

    fn clip(path: &Path, w: u32, frames: u32) {
        let v = VideoSettings {
            codec: VideoCodec::H264,
            width: w,
            height: 64,
            rate: Rate::FPS_25,
            bitrate_kbps: None,
            quality: 30,
            hardware: false,
        };
        let mut m = Muxer::create(path, Some(&v), None).unwrap();
        let luma = vec![90u8; (w * 64) as usize];
        let chroma = vec![128u8; (w / 2 * 32) as usize];
        for _ in 0..frames {
            m.push_video(&[&luma, &chroma, &chroma]).unwrap();
        }
        m.finish().unwrap();
    }

    #[test]
    fn moved_media_is_found_and_relinked() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("shoot");
        std::fs::create_dir_all(&old).unwrap();
        clip(&old.join("A001.mp4"), 64, 10);
        clip(&old.join("B002.mp4"), 64, 20);
        let mut e = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
        let root = e.project.root;
        for f in ["A001.mp4", "B002.mp4"] {
            let a = op_media::probe(&old.join(f)).unwrap();
            e.edit("Import", |p| Ok(p.add_asset(root, a)));
        }
        // the shoot folder is moved somewhere deeper, next to a same-named impostor
        let new = dir.path().join("disk").join("2026").join("shoot");
        std::fs::create_dir_all(new.parent().unwrap()).unwrap();
        std::fs::rename(&old, &new).unwrap();
        let decoy = dir.path().join("disk").join("other");
        std::fs::create_dir_all(&decoy).unwrap();
        clip(&decoy.join("B002.mp4"), 128, 5);
        let missing = offline(&e.project);
        assert_eq!(missing.len(), 2);

        let list: Vec<(MediaAsset, String)> = missing
            .iter()
            .map(|m| ((*e.project.asset(m.asset).unwrap()).clone(), m.name.clone()))
            .collect();
        let search = Search::start(list, vec![dir.path().join("disk")]);
        let start = std::time::Instant::now();
        while search.state.lock().running && start.elapsed().as_secs() < 30 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let found = search.state.lock().found.clone();
        assert_eq!(found.len(), 2);
        for p in found.values() {
            assert!(
                p.starts_with(&new),
                "the real file wins over the decoy: {}",
                p.display()
            );
        }
        let links: Vec<(AssetId, PathBuf)> = found.into_iter().collect();
        assert_eq!(e.link_media(&links).unwrap(), 2);
        assert!(offline(&e.project).is_empty());
        // one undo brings the old paths back
        e.execute("cmd.edit.undo", crate::Focus::Project);
        assert_eq!(offline(&e.project).len(), 2);
    }

    #[test]
    fn siblings_of_a_located_file_are_linked_too() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Clip1.MOV"), b"x").unwrap();
        std::fs::write(dir.path().join("clip2.mov"), b"x").unwrap();
        let missing = vec![
            Missing {
                asset: AssetId(1),
                name: "clip1.mov".into(),
                path: "/gone/clip1.mov".into(),
            },
            Missing {
                asset: AssetId(2),
                name: "Clip2.mov".into(),
                path: "/gone/Clip2.mov".into(),
            },
            Missing {
                asset: AssetId(3),
                name: "other.mov".into(),
                path: "/gone/other.mov".into(),
            },
        ];
        let s = siblings(dir.path(), &missing);
        assert_eq!(s.len(), 2);
        assert!(skipped("Windows") && skipped(".git") && !skipped("Videos"));
    }
}
