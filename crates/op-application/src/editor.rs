//! The editor: one project, its history, the session state and the services around them. The UI
//! reads it and changes it only through its methods, so every document change is one validated,
//! undoable transaction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use op_core::*;
use op_project::ProjectError;
use op_timeline::{Clipboard, EditOptions, Patch, SourceClip};

use crate::keymap::Keymap;
use crate::media::{self, MediaService};
use crate::prefs::{Dirs, Preferences};
use crate::session::*;

pub const APP_NAME: &str = "OpenPremier";
pub const GENERATOR: &str = concat!("OpenPremier ", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

#[derive(Clone, Debug, Default)]
pub struct Transport {
    pub playing: Option<Monitor>,
    pub speed: f64,
    /// Stop (or loop) when the playhead reaches this time.
    pub stop_at: Option<SeqTime>,
    pub loop_from: Option<SeqTime>,
}

/// The Source Monitor shows a project item through a hidden one-clip sequence, so it renders and
/// plays with exactly the same code as the Program Monitor.
#[derive(Clone, Default)]
pub struct SourceMonitor {
    pub item: Option<ItemId>,
    pub playhead: SeqTime,
    cache: Option<(ItemId, u64, Arc<Project>, SequenceId)>,
}

/// A project read from disk, ready to become the current one. Reading does not touch the editor,
/// so it can run on a background thread while the window stays responsive.
pub struct LoadedProject {
    pub path: PathBuf,
    pub project: Project,
    pub view: serde_json::Value,
    /// The file is a native project: saving writes back to it.
    pub native: bool,
    pub report: Option<op_project::prproj::ImportReport>,
    pub offline: usize,
}

pub struct Editor {
    pub project: Project,
    pub history: op_core::History,
    pub path: Option<PathBuf>,
    pub prefs: Preferences,
    pub dirs: Dirs,
    pub keymap: Keymap,
    pub media: Arc<MediaService>,
    pub playback: op_audio::Playback,
    pub tool: Tool,
    pub selection: Selection,
    pub active: Option<SequenceId>,
    pub open: Vec<SequenceId>,
    pub views: HashMap<SequenceId, SeqView>,
    pub playheads: HashMap<SequenceId, SeqTime>,
    pub source: SourceMonitor,
    pub transport: Transport,
    pub clipboard: Clipboard,
    /// Project panel selection and the bin new items go into.
    pub items: Vec<ItemId>,
    pub bin: ItemId,
    pub status: Option<Status>,
    pub imports: Vec<(
        ItemId,
        crossbeam_channel::Receiver<(PathBuf, Result<MediaAsset, String>)>,
    )>,
    pub importing: usize,
    /// Files being imported that go onto the timeline as soon as they are ready (pasted media).
    pub place_after_import: Vec<Placement>,
    pub exports: Vec<crate::export::ExportJob>,
    pub proxies: crate::proxies::ProxyQueue,
    /// Automatic captions being transcribed.
    pub captioning: Option<crate::captions::CaptionJob>,
    pub last_import_report: Option<op_project::prproj::ImportReport>,
    autosave_at: Instant,
    autosaving: Arc<AtomicBool>,
    snapshot: Option<(u64, Arc<Project>)>,
    /// Revision last handed to the crash-recovery writer, and the one last written.
    recovery_revision: u64,
    recovery_written: u64,
    recovery_at: Instant,
    /// Bumped on every change that needs a redraw of the monitors.
    pub frame_generation: u64,
}

/// Where a file being imported goes on the timeline once it is in the project (pasting).
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub path: PathBuf,
    /// Sequence time; None: the playhead.
    pub at: Option<SeqTime>,
    /// The track under the pointer, used when it is free there.
    pub track: Option<TrackRef>,
}

impl Placement {
    pub fn at_playhead(path: PathBuf) -> Placement {
        Placement {
            path,
            at: None,
            track: None,
        }
    }
}

impl Editor {
    pub fn new(dirs: Dirs, prefs: Preferences, audio: bool) -> Editor {
        let media = MediaService::new(dirs.cache.join("MediaCache"), prefs.frame_cache_mb << 20);
        media.set_hardware_decoding(crate::media::HardwareDecoding::from_pref(
            &prefs.hardware_decoding,
        ));
        media.set_use_proxies(prefs.use_proxies);
        media.set_read_ahead(prefs.performance().read_ahead);
        let source: Arc<dyn op_audio::AudioSource> = media.clone();
        let playback = if audio {
            op_audio::Playback::start(source)
        } else {
            op_audio::Playback::silent(source)
        };
        let keymap = Keymap::with_overrides(&prefs.shortcuts);
        let mut project = Project::new("Untitled");
        apply_prefs(&mut project, &prefs);
        let bin = project.root;
        Editor {
            project,
            history: op_core::History::default(),
            path: None,
            prefs,
            dirs,
            keymap,
            media,
            playback,
            tool: Tool::Selection,
            selection: Selection::default(),
            active: None,
            open: Vec::new(),
            views: HashMap::new(),
            playheads: HashMap::new(),
            source: SourceMonitor::default(),
            transport: Transport {
                speed: 1.0,
                ..Default::default()
            },
            clipboard: Clipboard::default(),
            items: Vec::new(),
            bin,
            status: None,
            imports: Vec::new(),
            importing: 0,
            place_after_import: Vec::new(),
            exports: Vec::new(),
            proxies: Default::default(),
            captioning: None,
            last_import_report: None,
            autosave_at: Instant::now(),
            autosaving: Arc::new(AtomicBool::new(false)),
            snapshot: None,
            recovery_revision: u64::MAX,
            recovery_written: u64::MAX,
            recovery_at: Instant::now(),
            frame_generation: 0,
        }
    }

    // ------------------------------------------------------------------------------- status

    pub fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status {
            text: text.into(),
            error: false,
            at: Instant::now(),
        });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::warn!("{text}");
        self.status = Some(Status {
            text,
            error: true,
            at: Instant::now(),
        });
    }

    pub fn opts(&self) -> EditOptions {
        EditOptions {
            linked_selection: self.prefs.linked_selection,
            ripple_markers: self.prefs.ripple_markers,
        }
    }

    pub fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.project.name.clone());
        let dirty = if self.history.is_dirty() { "*" } else { "" };
        format!("{APP_NAME} - {name}{dirty}")
    }

    // --------------------------------------------------------------------------- snapshots

    /// The current project as a shared snapshot (render threads, playback).
    pub fn snapshot(&mut self) -> Arc<Project> {
        let rev = self.history.revision();
        match &self.snapshot {
            Some((r, p)) if *r == rev => p.clone(),
            _ => {
                let p = Arc::new(self.project.clone());
                self.snapshot = Some((rev, p.clone()));
                p
            }
        }
    }

    fn changed(&mut self) {
        self.media.set_assets(self.project.assets.iter());
        self.frame_generation += 1;
        // forget session references to things that no longer exist
        let seqs: Vec<SequenceId> = self.project.sequences.keys().copied().collect();
        self.open.retain(|s| seqs.contains(s));
        if self.active.is_some_and(|a| !seqs.contains(&a)) {
            self.active = self.open.first().copied();
        }
        if let Some(seq) = self.active.and_then(|s| self.project.sequence(s)) {
            let exists = |c: &ClipId| seq.clip(*c).is_some();
            let trs: Vec<TransitionId> = seq
                .all_tracks()
                .flat_map(|(_, t)| t.transitions.iter().map(|x| x.id))
                .collect();
            self.selection.clips.retain(exists);
            self.selection.transitions.retain(|t| trs.contains(t));
        } else {
            self.selection.clear();
        }
        self.items.retain(|i| self.project.item(*i).is_some());
        if self.project.item(self.bin).is_none_or(|b| !b.is_bin()) {
            self.bin = self.project.root;
        }
        if self
            .source
            .item
            .is_some_and(|i| self.project.item(i).is_none())
        {
            self.source.item = None;
        }
        if self.transport.playing == Some(Monitor::Program) {
            let snap = self.snapshot();
            self.playback.update(snap);
        }
    }

    /// Runs an edit as one undoable step. Errors become a status message.
    pub fn edit<R>(
        &mut self,
        label: &str,
        f: impl FnOnce(&mut Project) -> EditResult<R>,
    ) -> Option<R> {
        self.edit_merge(label, None, f)
    }

    /// Like `edit`, merging with the previous step when `merge` matches (value drags).
    pub fn edit_merge<R>(
        &mut self,
        label: &str,
        merge: Option<String>,
        f: impl FnOnce(&mut Project) -> EditResult<R>,
    ) -> Option<R> {
        match self.project.transact(f) {
            Ok((next, r)) => {
                let before = std::mem::replace(&mut self.project, next);
                self.history.record(label, before, merge);
                self.changed();
                Some(r)
            }
            Err(EditError::Nothing) => None,
            Err(e) => {
                self.error(e.to_string());
                None
            }
        }
    }

    /// Ends a merged drag so the next change is a new step.
    pub fn seal(&mut self) {
        self.history.seal();
    }

    pub fn undo(&mut self) {
        let label = self.history.undo_label().map(str::to_string);
        if let Some(p) = self.history.undo(&self.project) {
            self.project = p;
            self.changed();
            if let Some(l) = label {
                self.info(format!("Undo {l}"));
            }
        }
    }

    pub fn redo(&mut self) {
        let label = self.history.redo_label().map(str::to_string);
        if let Some(p) = self.history.redo(&self.project) {
            self.project = p;
            self.changed();
            if let Some(l) = label {
                self.info(format!("Redo {l}"));
            }
        }
    }

    pub fn history_jump(&mut self, index: usize) {
        if let Some(p) = self.history.jump(index, &self.project) {
            self.project = p;
            self.changed();
        }
    }

    // ------------------------------------------------------------------------------ project

    pub fn new_project(&mut self, name: &str) {
        self.stop();
        crate::recovery::forget();
        log::info!("new project");
        self.project = Project::new(name);
        apply_prefs(&mut self.project, &self.prefs);
        self.history.clear();
        self.history.mark_saved();
        self.path = None;
        self.reset_session();
        self.changed();
    }

    fn reset_session(&mut self) {
        self.selection.clear();
        self.active = None;
        self.open.clear();
        self.views.clear();
        self.playheads.clear();
        self.source = SourceMonitor::default();
        self.items.clear();
        self.bin = self.project.root;
        self.clipboard = Clipboard::default();
    }

    /// Reads a native project, or imports a Premiere Pro / FCP XML / OTIO file as a new project.
    /// Pure: it can run on any thread (see `install`).
    pub fn load_project(path: &Path) -> Result<LoadedProject, String> {
        let started = Instant::now();
        log::info!("opening {}", path.display());
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut report = None;
        let mut offline = 0;
        let (mut project, view, native) = match ext.as_str() {
            "prproj" => {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                let (p, r) =
                    op_project::prproj::import(&bytes, &name).map_err(|e| e.to_string())?;
                report = Some(r);
                (p, serde_json::Value::Null, false)
            }
            "xml" => {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                (
                    op_project::fcpxml::import(&text, &name)
                        .map_err(|e| e.to_string())?
                        .0,
                    serde_json::Value::Null,
                    false,
                )
            }
            "otio" => {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                (
                    op_project::otio::import(&text, &name)
                        .map_err(|e| e.to_string())?
                        .0,
                    serde_json::Value::Null,
                    false,
                )
            }
            _ => {
                let loaded = op_project::native::load(path).map_err(|e| match e {
                    ProjectError::Version(v) => {
                        format!("This project was saved by a newer version (format {v}).")
                    }
                    e => e.to_string(),
                })?;
                offline = loaded.offline.len();
                (loaded.project, loaded.view, true)
            }
        };
        if !native {
            refresh_media(&mut project);
            project.name = name;
        }
        log::info!(
            "read {} in {} ms: {} items, {} media, {} sequences, {} offline",
            path.display(),
            started.elapsed().as_millis(),
            project.items.len(),
            project.assets.len(),
            project.sequences.len(),
            offline
        );
        Ok(LoadedProject {
            path: path.to_path_buf(),
            project,
            view,
            native,
            report,
            offline,
        })
    }

    /// Makes a loaded project the current one.
    pub fn install(&mut self, loaded: LoadedProject) {
        self.stop();
        crate::recovery::forget();
        self.project = loaded.project;
        self.history.clear();
        self.history.mark_saved();
        self.path = loaded.native.then(|| loaded.path.clone());
        self.reset_session();
        self.restore_view(&loaded.view);
        if loaded.report.is_some() {
            self.last_import_report = loaded.report;
        }
        self.changed();
        if loaded.offline > 0 {
            self.info(format!("{} media files are offline", loaded.offline));
        }
        if loaded.native {
            self.prefs.add_recent(&loaded.path);
            let _ = self.prefs.save(&self.dirs);
        }
        // open the first sequence when nothing was open
        if self.active.is_none()
            && let Some(first) = self.project.sequences.keys().next().copied()
        {
            self.open_sequence(first);
        }
    }

    /// Opens a project on the calling thread (`load_project` + `install`).
    pub fn open(&mut self, path: &Path) -> Result<(), String> {
        let loaded = Self::load_project(path)?;
        self.install(loaded);
        Ok(())
    }

    /// Re-probes media that exists on disk to complete descriptors made by importers.
    pub fn refresh_media(&mut self) {
        refresh_media(&mut self.project);
    }

    pub fn save(&mut self) -> Result<(), String> {
        match self.path.clone() {
            Some(p) => self.save_as(&p),
            None => Err("no file".into()),
        }
    }

    pub fn save_as(&mut self, path: &Path) -> Result<(), String> {
        let path = if path.extension().is_none() {
            path.with_extension(op_project::native::EXTENSION)
        } else {
            path.to_path_buf()
        };
        let mut p = self.project.clone();
        for (sid, t) in &self.playheads {
            if let Some(s) = p.sequence_mut(*sid) {
                s.playhead = *t;
            }
        }
        if let Some(stem) = path.file_stem() {
            p.name = stem.to_string_lossy().into_owned();
        }
        op_project::native::save(&path, &p, &self.view_state(), GENERATOR)
            .map_err(|e| e.to_string())?;
        self.path = Some(path.clone());
        self.history.mark_saved();
        crate::recovery::discard();
        self.recovery_written = self.history.revision();
        self.prefs.add_recent(&path);
        let _ = self.prefs.save(&self.dirs);
        log::info!("saved {}", path.display());
        self.info(format!("Saved {}", path.display()));
        Ok(())
    }

    /// Writes a copy without changing the current file.
    pub fn save_copy(&mut self, path: &Path) -> Result<(), String> {
        op_project::native::save(path, &self.project, &self.view_state(), GENERATOR)
            .map_err(|e| e.to_string())
    }

    fn view_state(&self) -> serde_json::Value {
        serde_json::json!({
            "open": self.open.iter().map(|s| s.0).collect::<Vec<_>>(),
            "active": self.active.map(|s| s.0),
            "views": self.views.iter().map(|(k, v)| (k.0.to_string(), serde_json::to_value(v).unwrap_or_default())).collect::<serde_json::Map<_, _>>(),
            "source": self.source.item.map(|i| i.0),
        })
    }

    fn restore_view(&mut self, v: &serde_json::Value) {
        for seq in self.project.sequences.values() {
            self.playheads.insert(seq.id, seq.playhead);
        }
        if let Some(open) = v.get("open").and_then(|o| o.as_array()) {
            self.open = open
                .iter()
                .filter_map(|x| x.as_u64())
                .map(SequenceId)
                .filter(|s| self.project.sequence(*s).is_some())
                .collect();
        }
        self.active = v
            .get("active")
            .and_then(|a| a.as_u64())
            .map(SequenceId)
            .filter(|s| self.project.sequence(*s).is_some());
        if let Some(views) = v.get("views").and_then(|o| o.as_object()) {
            for (k, val) in views {
                if let (Ok(id), Ok(view)) = (
                    k.parse::<u64>(),
                    serde_json::from_value::<SeqView>(val.clone()),
                ) {
                    self.views.insert(SequenceId(id), view);
                }
            }
        }
        self.source.item = v
            .get("source")
            .and_then(|s| s.as_u64())
            .map(ItemId)
            .filter(|i| self.project.item(*i).is_some());
    }

    // ------------------------------------------------------------------------------- import

    /// Starts importing files into a bin (probing runs in the background).
    pub fn import(&mut self, paths: Vec<PathBuf>, bin: ItemId) {
        let mut media = Vec::new();
        for p in paths {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            match ext.as_str() {
                "opproj" | "prproj" | "otio" => {
                    if let Err(e) = self.import_project(&p, bin) {
                        self.error(format!("{}: {e}", p.display()));
                    }
                }
                _ => media.push(p),
            }
        }
        if media.is_empty() {
            return;
        }
        self.importing += media.len();
        if let Some(dir) = media[0].parent() {
            self.prefs.last_import_dir = Some(dir.to_path_buf());
        }
        self.imports.push((bin, media::probe_many(media)));
    }

    /// Merges another project's bins, media and sequences into this one.
    fn import_project(&mut self, path: &Path, bin: ItemId) -> Result<(), String> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let other = match ext.as_str() {
            "prproj" => {
                let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                let (p, report) =
                    op_project::prproj::import(&bytes, &name).map_err(|e| e.to_string())?;
                self.last_import_report = Some(report);
                p
            }
            "otio" => {
                op_project::otio::import(
                    &std::fs::read_to_string(path).map_err(|e| e.to_string())?,
                    &name,
                )
                .map_err(|e| e.to_string())?
                .0
            }
            _ => {
                op_project::native::load(path)
                    .map_err(|e| e.to_string())?
                    .project
            }
        };
        self.edit("Import Project", |p| {
            crate::merge::merge_project(p, other, bin, &name);
            Ok(())
        });
        Ok(())
    }

    fn poll_imports(&mut self) {
        let mut done = Vec::new();
        let mut added = Vec::new();
        for (i, (bin, rx)) in self.imports.iter().enumerate() {
            loop {
                match rx.try_recv() {
                    Ok((_, Ok(asset))) => added.push((*bin, asset)),
                    Ok((path, Err(e))) => {
                        self.status = Some(Status {
                            text: format!("{}: {e}", path.display()),
                            error: true,
                            at: Instant::now(),
                        });
                        self.importing = self.importing.saturating_sub(1);
                    }
                    Err(crossbeam_channel::TryRecvError::Empty) => break,
                    Err(crossbeam_channel::TryRecvError::Disconnected) => {
                        done.push(i);
                        break;
                    }
                }
            }
        }
        for i in done.into_iter().rev() {
            self.imports.remove(i);
        }
        if added.is_empty() {
            return;
        }
        let n = added.len();
        self.importing = self.importing.saturating_sub(n);
        let place: Vec<Option<Placement>> = added
            .iter()
            .map(|(_, a)| {
                let at = self
                    .place_after_import
                    .iter()
                    .position(|p| Path::new(&a.path) == p.path.as_path());
                at.map(|i| self.place_after_import.remove(i))
            })
            .collect();
        let still = self.prefs.still_seconds;
        let new_items = self.edit("Import", |p| {
            let mut items = Vec::new();
            for (bin, asset) in added {
                let bin = if p.item(bin).is_some_and(|b| b.is_bin()) {
                    bin
                } else {
                    p.root
                };
                // the same file twice becomes one asset with two items, like a duplicate
                let (_, item) = p.add_asset(bin, asset);
                items.push(item);
            }
            p.settings.still_duration = Dur::from_seconds(still);
            Ok(items)
        });
        if let Some(items) = new_items {
            for (item, placement) in items.iter().zip(place) {
                if let Some(p) = placement {
                    // nothing open yet: the first pasted file makes a sequence of its own
                    if self.active.is_none() {
                        self.sequence_from_item(*item);
                    } else {
                        self.place_item(*item, p.at, p.track);
                    }
                }
            }
            self.items = items;
            self.info(format!("Imported {n} files"));
        }
    }

    // ---------------------------------------------------------------------------- sequences

    pub fn active_seq(&self) -> Option<&Sequence> {
        self.active.and_then(|s| self.project.sequence(s))
    }

    pub fn view(&mut self, sid: SequenceId) -> &mut SeqView {
        let (nv, na) = self
            .project
            .sequence(sid)
            .map(|s| (s.video.len(), s.audio.len()))
            .unwrap_or((1, 1));
        let v = self.views.entry(sid).or_default();
        if v.video_targets.len() < nv {
            v.video_targets.resize(nv, false);
        }
        if v.audio_targets.len() < na {
            v.audio_targets.resize(na, false);
        }
        v
    }

    pub fn open_sequence(&mut self, sid: SequenceId) {
        if self.project.sequence(sid).is_none() {
            return;
        }
        if !self.open.contains(&sid) {
            self.open.push(sid);
        }
        if self.active != Some(sid) {
            self.stop();
            self.selection.clear();
        }
        self.active = Some(sid);
        self.view(sid);
        self.frame_generation += 1;
    }

    pub fn close_sequence(&mut self, sid: SequenceId) {
        self.open.retain(|s| *s != sid);
        if self.active == Some(sid) {
            self.stop();
            self.active = self.open.last().copied();
            self.selection.clear();
        }
    }

    /// Creates a sequence in the current bin and opens it.
    pub fn new_sequence(&mut self, name: &str, settings: SequenceSettings) -> Option<SequenceId> {
        let bin = self.bin;
        let name = name.to_string();
        let sid = self.edit("New Sequence", move |p| {
            Ok(p.add_sequence(bin, name, settings).0)
        })?;
        self.open_sequence(sid);
        Some(sid)
    }

    /// A sequence matching a clip's frame size, rate and audio, containing the clip.
    pub fn sequence_from_item(&mut self, item: ItemId) -> Option<SequenceId> {
        let it = self.project.item(item)?.clone();
        let mut settings = self.prefs.default_sequence.clone();
        if let ItemKind::Media { asset, .. } = it.kind
            && let Some(a) = self.project.asset(asset)
        {
            if let Some(v) = &a.video {
                let (w, h) = v.display_size();
                settings.width = w;
                settings.height = h;
                if !a.is_still() {
                    settings.rate = a.frame_rate().unwrap_or(settings.rate);
                }
            }
            if let Some(s) = a.audio.first() {
                settings.audio_rate = s.sample_rate;
            }
        }
        let name = it
            .name
            .rsplit_once('.')
            .map(|(n, _)| n.to_string())
            .unwrap_or(it.name.clone());
        let sid = self.new_sequence(&name, settings)?;
        let spec = SourceClip::from_item(&self.project, item).ok()?;
        let patch = self.patch(sid);
        let opts = self.opts();
        self.edit("Overwrite", |p| {
            op_timeline::overwrite(p, sid, &spec, SeqTime::ZERO, &patch, opts)
        });
        Some(sid)
    }

    pub fn patch(&mut self, sid: SequenceId) -> Patch {
        let v = self.view(sid);
        Patch {
            video: v.video_patch,
            audio: v.audio_patch.clone(),
        }
    }

    pub fn targets(&mut self) -> Vec<TrackRef> {
        let Some(sid) = self.active else {
            return vec![];
        };
        let seq = self.project.sequence(sid).unwrap().clone();
        self.view(sid).targets(&seq)
    }

    // ------------------------------------------------------------------------------ playheads

    pub fn playhead(&self) -> SeqTime {
        self.active
            .and_then(|s| self.playheads.get(&s).copied())
            .unwrap_or(SeqTime::ZERO)
    }

    pub fn set_playhead(&mut self, t: SeqTime) {
        if let Some(sid) = self.active {
            let rate = self
                .project
                .sequence(sid)
                .map(|s| s.rate())
                .unwrap_or(Rate::FPS_25);
            let t = t.max(SeqTime::ZERO).floor_frame(rate);
            if self.playheads.get(&sid) != Some(&t) {
                self.playheads.insert(sid, t);
                self.frame_generation += 1;
            }
        }
    }

    /// Moves the playhead of the monitor that has focus.
    pub fn set_monitor_time(&mut self, monitor: Monitor, t: SeqTime) {
        match monitor {
            Monitor::Program => self.set_playhead(t),
            Monitor::Source => {
                let rate = self.source_rate();
                let len = self.source_duration();
                let t = t
                    .clamp(
                        SeqTime::ZERO,
                        (SeqTime::ZERO + len - rate.frame_duration()).max(SeqTime::ZERO),
                    )
                    .floor_frame(rate);
                if self.source.playhead != t {
                    self.source.playhead = t;
                    self.frame_generation += 1;
                }
            }
        }
    }

    pub fn monitor_time(&self, monitor: Monitor) -> SeqTime {
        match monitor {
            Monitor::Program => self.playhead(),
            Monitor::Source => self.source.playhead,
        }
    }

    pub fn monitor_rate(&self, monitor: Monitor) -> Rate {
        match monitor {
            Monitor::Program => self.active_seq().map(|s| s.rate()).unwrap_or(Rate::FPS_25),
            Monitor::Source => self.source_rate(),
        }
    }

    // ------------------------------------------------------------------------------- source

    pub fn load_source(&mut self, item: ItemId) {
        if self.project.item(item).is_some_and(|i| !i.is_bin()) {
            if self.source.item != Some(item) {
                if self.transport.playing == Some(Monitor::Source) {
                    self.stop();
                }
                self.source.item = Some(item);
                self.source.playhead = self
                    .project
                    .item(item)
                    .and_then(|i| i.mark_in)
                    .map(|t| t.cast())
                    .unwrap_or(SeqTime::ZERO);
            }
            self.frame_generation += 1;
        }
    }

    /// The hidden project and sequence that present the source item.
    pub fn source_view(&mut self) -> Option<(Arc<Project>, SequenceId)> {
        let item = self.source.item?;
        let rev = self.history.revision();
        if let Some((i, r, p, s)) = &self.source.cache
            && *i == item
            && *r == rev
        {
            return Some((p.clone(), *s));
        }
        let mut p = self.project.clone();
        let it = p.item(item)?.clone();
        let mut settings = SequenceSettings {
            video_tracks: 1,
            audio_tracks: 2,
            ..self.prefs.default_sequence.clone()
        };
        let mut spec = SourceClip::from_item(&p, item).ok()?;
        match &it.kind {
            ItemKind::Media { asset, subclip } => {
                let a = p.asset(*asset)?;
                if let Some(v) = &a.video {
                    let (w, h) = v.display_size();
                    settings.width = w;
                    settings.height = h;
                    if !a.is_still() {
                        settings.rate = a.frame_rate().unwrap_or(settings.rate);
                    }
                }
                settings.audio_tracks = a.audio.len().max(1);
                spec.range = subclip.or(a.available()).unwrap_or(SrcRange::with_duration(
                    SrcTime::ZERO,
                    p.settings.still_duration,
                ));
            }
            ItemKind::Sequence { sequence } => {
                let s = p.sequence(*sequence)?;
                settings = SequenceSettings {
                    video_tracks: 1,
                    audio_tracks: 1,
                    ..s.settings.clone()
                };
                spec.range = SrcRange::with_duration(
                    SrcTime::ZERO,
                    s.duration().max(s.settings.frame_duration()),
                );
            }
            ItemKind::Synthetic { duration, .. } => {
                spec.range = SrcRange::with_duration(SrcTime::ZERO, *duration);
            }
            ItemKind::Bin { .. } => return None,
        }
        // the source is shown from the media start so the playhead is source time
        spec.range = SrcRange::new(SrcTime::ZERO, spec.range.end);
        if spec.range.is_empty() {
            return None;
        }
        let root = p.root;
        let (sid, _) = p.add_sequence(root, "__source__", settings);
        let patch = Patch {
            video: Some(0),
            audio: (0..spec.audio.len()).map(Some).collect(),
        };
        let opts = EditOptions {
            linked_selection: true,
            ripple_markers: false,
        };
        if op_timeline::overwrite(&mut p, sid, &spec, SeqTime::ZERO, &patch, opts).is_err() {
            return None;
        }
        let p = Arc::new(p);
        self.source.cache = Some((item, rev, p.clone(), sid));
        Some((p, sid))
    }

    pub fn source_rate(&self) -> Rate {
        let Some(item) = self.source.item.and_then(|i| self.project.item(i)) else {
            return Rate::FPS_25;
        };
        match &item.kind {
            ItemKind::Media { asset, .. } => self
                .project
                .asset(*asset)
                .and_then(|a| if a.is_still() { None } else { a.frame_rate() })
                .unwrap_or(self.prefs.default_sequence.rate),
            ItemKind::Sequence { sequence } => self
                .project
                .sequence(*sequence)
                .map(|s| s.rate())
                .unwrap_or(Rate::FPS_25),
            _ => self.prefs.default_sequence.rate,
        }
    }

    pub fn source_duration(&self) -> Dur {
        let Some(item) = self.source.item.and_then(|i| self.project.item(i)) else {
            return Dur::ZERO;
        };
        match &item.kind {
            ItemKind::Media { asset, subclip } => subclip
                .map(|r| r.end.since_zero())
                .or_else(|| {
                    self.project
                        .asset(*asset)
                        .and_then(|a| a.available())
                        .map(|r| r.duration())
                })
                .unwrap_or(self.project.settings.still_duration),
            ItemKind::Sequence { sequence } => self
                .project
                .sequence(*sequence)
                .map(|s| s.duration())
                .unwrap_or(Dur::ZERO),
            ItemKind::Synthetic { duration, .. } => *duration,
            ItemKind::Bin { .. } => Dur::ZERO,
        }
    }

    // ------------------------------------------------------------------------------ transport

    pub fn is_playing(&self) -> bool {
        self.transport.playing.is_some()
    }

    pub fn play(&mut self, monitor: Monitor, speed: f64) {
        let (project, sid, from) = match monitor {
            Monitor::Program => {
                let Some(sid) = self.active else { return };
                let mut from = self.playhead();
                let end = SeqTime::ZERO
                    + self
                        .project
                        .sequence(sid)
                        .map(|s| s.duration())
                        .unwrap_or(Dur::ZERO);
                if speed > 0.0 && from >= end {
                    from = SeqTime::ZERO;
                    self.set_playhead(from);
                }
                (self.snapshot(), sid, from)
            }
            Monitor::Source => {
                let Some((p, sid)) = self.source_view() else {
                    return;
                };
                let mut from = self.source.playhead;
                if speed > 0.0
                    && from + self.source_rate().frame_duration()
                        >= SeqTime::ZERO + self.source_duration()
                {
                    from = SeqTime::ZERO;
                    self.source.playhead = from;
                }
                (p, sid, from)
            }
        };
        self.transport.playing = Some(monitor);
        self.transport.speed = speed;
        self.playback.play(project, sid, from, speed);
    }

    pub fn stop(&mut self) {
        if self.transport.playing.is_some() {
            self.playback.stop();
        }
        self.transport.playing = None;
        self.transport.stop_at = None;
        self.transport.loop_from = None;
        self.transport.speed = 1.0;
    }

    pub fn toggle_play(&mut self, monitor: Monitor) {
        if self.transport.playing.is_some() {
            self.stop();
        } else {
            self.play(monitor, 1.0);
        }
    }

    /// J/L shuttle: each press in the same direction doubles the speed (1, 2, 4, 8); the
    /// opposite key slows down before reversing (provisional ladder, KBD-X-001).
    pub fn shuttle(&mut self, monitor: Monitor, forward: bool) {
        let dir = if forward { 1.0 } else { -1.0 };
        let cur = if self.transport.playing == Some(monitor) {
            self.transport.speed
        } else {
            0.0
        };
        let next = if cur == 0.0 || cur.signum() != dir {
            if cur.abs() > 1.0 { cur / 2.0 } else { dir }
        } else {
            (cur * 2.0).clamp(-8.0, 8.0)
        };
        self.play(monitor, next);
    }

    pub fn step(&mut self, monitor: Monitor, frames: i64) {
        self.stop();
        let rate = self.monitor_rate(monitor);
        let t = self.monitor_time(monitor) + rate.frames_to_dur(frames);
        self.set_monitor_time(monitor, t);
        self.scrub_audio(monitor);
    }

    /// Plays one frame of audio at the monitor's time (audio scrubbing).
    pub fn scrub_audio(&mut self, monitor: Monitor) {
        if !self.prefs.audio_scrubbing || self.is_playing() {
            return;
        }
        let rate = self.monitor_rate(monitor);
        let len = rate.frame_duration().max(Dur::from_seconds(0.04));
        match monitor {
            Monitor::Program => {
                if let Some(sid) = self.active {
                    let t = self.playhead();
                    let p = self.snapshot();
                    self.playback.blip(p, sid, t, len);
                }
            }
            Monitor::Source => {
                if let Some((p, sid)) = self.source_view() {
                    self.playback.blip(p, sid, self.source.playhead, len);
                }
            }
        }
    }

    /// Advances playback state; call once per UI frame. Returns true while something changes
    /// continuously (playback, imports, exports).
    pub fn tick(&mut self) -> bool {
        self.poll_imports();
        self.poll_exports();
        self.poll_proxies();
        self.poll_captions();
        self.autosave();
        self.keep_recovery();
        if let Some(monitor) = self.transport.playing {
            if !self.playback.is_playing() {
                self.transport.playing = None;
                return true;
            }
            if let Some(t) = self.playback.position() {
                let (end, start) = match monitor {
                    Monitor::Program => (
                        SeqTime::ZERO
                            + self.active_seq().map(|s| s.duration()).unwrap_or(Dur::ZERO),
                        SeqTime::ZERO,
                    ),
                    Monitor::Source => (SeqTime::ZERO + self.source_duration(), SeqTime::ZERO),
                };
                let stop_at = self.transport.stop_at.unwrap_or(end);
                let forward = self.transport.speed > 0.0;
                if (forward && t >= stop_at) || (!forward && t <= start) {
                    let final_t = if forward {
                        stop_at - self.monitor_rate(monitor).frame_duration()
                    } else {
                        start
                    };
                    match self.transport.loop_from {
                        Some(from) => {
                            self.set_monitor_time(monitor, from);
                            let speed = self.transport.speed;
                            let stop_at = self.transport.stop_at;
                            self.play(monitor, speed);
                            self.transport.stop_at = stop_at;
                            self.transport.loop_from = Some(from);
                        }
                        None => {
                            self.stop();
                            self.set_monitor_time(monitor, final_t.max(start));
                        }
                    }
                } else {
                    self.set_monitor_time(monitor, t);
                }
            }
            return true;
        }
        !self.imports.is_empty()
            || self.exports.iter().any(|e| !e.finished())
            || self.media.conforming() > 0
    }

    // ------------------------------------------------------------------------------- proxies

    /// Queues proxy creation for the video assets among `assets` (Project panel > Proxy >
    /// Create Proxies). Assets that already have a proxy on disk are skipped.
    pub fn create_proxies(&mut self, assets: &[AssetId]) {
        let folder = self.dirs.data.join("Proxies");
        let mut jobs = Vec::new();
        let mut skipped = None;
        for id in assets {
            let Some(a) = self.project.assets.get(id).cloned() else {
                continue;
            };
            if !a.has_video() {
                continue;
            }
            if let Some(why) = crate::proxies::unsuitable(&a) {
                skipped = Some(why);
                continue;
            }
            if a.proxy.as_deref().is_some_and(|p| Path::new(p).is_file()) {
                continue;
            }
            let dst = crate::proxies::proxy_path(&folder, &a);
            jobs.push((a, dst));
        }
        if jobs.is_empty() {
            match skipped {
                Some(why) => self.info(why.to_string()),
                None => self.info("The selected clips already have proxies".to_string()),
            }
            return;
        }
        log::info!("creating {} proxies in {}", jobs.len(), folder.display());
        self.proxies.add(jobs);
    }

    /// Detaches proxies from `assets` and deletes the files OpenPremier made.
    pub fn remove_proxies(&mut self, assets: &[AssetId]) {
        let folder = self.dirs.data.join("Proxies");
        let with: Vec<(AssetId, String)> = assets
            .iter()
            .filter_map(|id| {
                let a = self.project.asset(*id)?;
                a.proxy.clone().map(|p| (*id, p))
            })
            .collect();
        if with.is_empty() {
            return;
        }
        let ids: Vec<AssetId> = with.iter().map(|(id, _)| *id).collect();
        self.edit("Remove Proxies", |p| {
            for id in &ids {
                if let Some(a) = p.assets.get_mut(id) {
                    Arc::make_mut(a).proxy = None;
                }
            }
            Ok(())
        });
        let mut doomed = Vec::new();
        for (id, path) in with {
            self.media.forget(id);
            if Path::new(&path).starts_with(&folder) {
                doomed.push(PathBuf::from(path));
            }
        }
        // the decoder that was reading a proxy closes it a moment later; Windows refuses to
        // delete an open file, so the files go once they are free
        if !doomed.is_empty() {
            let _ = std::thread::Builder::new()
                .name("remove-proxies".into())
                .spawn(move || {
                    for _ in 0..100 {
                        doomed.retain(|p| std::fs::remove_file(p).is_err() && p.exists());
                        if doomed.is_empty() {
                            return;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    for p in doomed {
                        log::warn!("proxy file still in use, not deleted: {}", p.display());
                    }
                });
        }
    }

    /// Switches the performance profile: preview resolutions, frame cache and read-ahead
    /// change now (the interface follows from the preference). `hw` limits the frame cache
    /// to this computer's memory.
    pub fn set_performance_profile(
        &mut self,
        p: crate::performance::Profile,
        hw: Option<&crate::performance::Hardware>,
    ) {
        let st = p.settings();
        self.prefs.performance_custom = None;
        self.prefs.performance_profile = Some(p.index());
        self.prefs.performance_mode = p <= crate::performance::Profile::Performance;
        self.prefs.playback_resolution = st.playback_resolution;
        self.prefs.paused_resolution = st.paused_resolution;
        self.prefs.frame_cache_mb = hw.map(|h| h.frame_cache_mb(p)).unwrap_or(st.frame_cache_mb);
        self.media.set_frame_budget(self.prefs.frame_cache_mb << 20);
        self.media.set_read_ahead(st.read_ahead);
        let _ = self.prefs.save(&self.dirs);
        log::info!(
            "performance profile: {} (cache {} MB)",
            p.label(),
            self.prefs.frame_cache_mb
        );
    }

    /// Switches to settings chosen one by one (preview resolutions and the frame cache are
    /// set as preferences of their own).
    pub fn set_performance_custom(&mut self, c: crate::performance::CustomSettings) {
        self.prefs.performance_custom = Some(c);
        self.media.set_read_ahead(c.settings().read_ahead);
        let _ = self.prefs.save(&self.dirs);
    }

    pub fn set_use_proxies(&mut self, on: bool) {
        self.prefs.use_proxies = on;
        let _ = self.prefs.save(&self.dirs);
        self.media.set_use_proxies(on);
        self.info(
            if on {
                "Proxies enabled"
            } else {
                "Proxies disabled"
            }
            .to_string(),
        );
    }

    /// Attaches finished proxies to their assets (one undo step per batch).
    fn poll_proxies(&mut self) {
        let results = self.proxies.take_results();
        if results.is_empty() {
            return;
        }
        let mut made = Vec::new();
        let mut failed = Vec::new();
        for (id, r) in results {
            match r {
                Ok(path) => made.push((id, path.to_string_lossy().into_owned())),
                Err(e) if e == "cancelled" => {}
                Err(e) => failed.push(e),
            }
        }
        if !made.is_empty() {
            let attach = made.clone();
            self.edit("Attach Proxies", |p| {
                for (id, path) in &attach {
                    if let Some(a) = p.assets.get_mut(id) {
                        Arc::make_mut(a).proxy = Some(path.clone());
                    }
                }
                Ok(())
            });
            for (id, _) in &made {
                self.media.forget(*id);
            }
            if !self.proxies.busy() {
                self.info(format!("Proxies ready ({})", made.len()));
            }
        }
        if let Some(e) = failed.first() {
            self.error(format!("Proxy not created: {e}"));
        }
    }

    fn poll_exports(&mut self) {
        let mut msgs = Vec::new();
        for e in &mut self.exports {
            if let Some(m) = e.take_message() {
                msgs.push(m);
            }
        }
        for (text, error) in msgs {
            if error {
                self.error(text)
            } else {
                self.info(text)
            }
        }
    }

    // ------------------------------------------------------------------------------ autosave

    /// Periodic autosave versions, written on a background thread from a snapshot.
    fn autosave(&mut self) {
        let every = Duration::from_secs(self.prefs.autosave_minutes.max(1) as u64 * 60);
        if self.prefs.autosave_minutes == 0 || self.autosave_at.elapsed() < every {
            return;
        }
        self.autosave_at = Instant::now();
        if !self.history.is_dirty() || self.autosaving.swap(true, Ordering::AcqRel) {
            return;
        }
        let project = self.snapshot();
        let dir = self.dirs.autosave();
        let name = self.document_name();
        let keep = self.prefs.autosave_keep;
        let busy = self.autosaving.clone();
        let spawned = std::thread::Builder::new()
            .name("autosave".into())
            .spawn(move || {
                match crate::autosave::write_project(&dir, &name, keep, &project) {
                    Ok(p) => log::info!("autosaved {}", p.display()),
                    Err(e) => log::warn!("autosave failed: {e}"),
                }
                busy.store(false, Ordering::Release);
            });
        if spawned.is_err() {
            self.autosaving.store(false, Ordering::Release);
        }
    }

    /// The name autosave and recovery files use.
    pub fn document_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.project.name.clone())
    }

    /// Keeps the crash-recovery copy current: unsaved changes reach disk within seconds, on a
    /// background thread, and the copy disappears once there is nothing unsaved.
    fn keep_recovery(&mut self) {
        let rev = self.history.revision();
        if !self.history.is_dirty() {
            if self.recovery_written != rev {
                crate::recovery::discard();
                self.recovery_written = rev;
            }
            return;
        }
        if rev != self.recovery_revision {
            let project = self.snapshot();
            let file = crate::recovery::file_for(&self.dirs.autosave(), &self.document_name());
            crate::recovery::remember(project, rev, file);
            self.recovery_revision = rev;
        }
        if self.recovery_written != rev
            && self.recovery_at.elapsed() >= crate::recovery::INTERVAL
            && let Some(written) = crate::recovery::write_in_background()
        {
            self.recovery_written = written;
            self.recovery_at = Instant::now();
        }
    }

    pub fn autosave_now(&mut self) -> Result<PathBuf, String> {
        crate::autosave::write(self)
    }
}

/// Re-probes media that exists on disk to complete descriptors made by importers.
pub fn refresh_media(project: &mut Project) {
    let ids: Vec<AssetId> = project.assets.keys().copied().collect();
    for id in ids {
        let a = project.asset(id).unwrap().clone();
        if !Path::new(&a.path).exists() {
            continue;
        }
        match op_media::probe(Path::new(&a.path)) {
            Ok(mut probed) => {
                probed.id = id;
                probed.interpretation = a.interpretation.clone();
                // keep ranges used by clips valid if the file is shorter than recorded
                if let (Some(old), Some(new)) = (a.available(), probed.available())
                    && new.end < old.end
                {
                    continue;
                }
                project.assets.insert(id, Arc::new(probed));
            }
            Err(e) => log::warn!("could not probe {}: {e}", a.path),
        }
    }
}

fn apply_prefs(p: &mut Project, prefs: &Preferences) {
    p.settings.still_duration = Dur::from_seconds(prefs.still_seconds);
    p.settings.video_transition_duration = Dur::from_seconds(prefs.video_transition_seconds);
    p.settings.audio_transition_duration = Dur::from_seconds(prefs.audio_transition_seconds);
    p.settings.media_scaling = prefs.media_scaling;
    p.settings.default_sequence = prefs.default_sequence.clone();
    p.settings.time_display = prefs.time_display;
}
