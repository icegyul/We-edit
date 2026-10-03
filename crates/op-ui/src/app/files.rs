//! Opening, saving, importing and exporting files.

use super::*;

impl State {
    /// Asks to save changes first when there are any.
    pub fn confirm(&mut self, then: Then) {
        if self.ed.history.is_dirty() {
            self.dialogs.push(Dialog::Unsaved(then));
        } else {
            self.proceed(then);
        }
    }

    pub fn proceed(&mut self, then: Then) {
        match then {
            Then::NewProject => {
                self.ed.new_project(t("Untitled"));
                self.after_open();
            }
            Then::Open(None) => {
                let mut d = rfd::FileDialog::new()
                    .add_filter(t("All Projects"), &["opproj", "prproj", "xml", "otio"])
                    .add_filter("OpenPremier", &[op_project::native::EXTENSION])
                    .add_filter("Adobe Premiere Pro", &["prproj"])
                    .add_filter("Final Cut Pro XML", &["xml"])
                    .add_filter("OpenTimelineIO", &["otio"]);
                if let Some(dir) = self.ed.prefs.recent.first().and_then(|p| p.parent()) {
                    d = d.set_directory(dir);
                }
                if let Some(p) = d.pick_file() {
                    self.open_path(&p);
                }
            }
            Then::Open(Some(p)) => self.open_path(&p),
            Then::Quit => {
                self.allow_close = true;
                self.quit = true;
            }
        }
    }

    /// Reads a project on a background thread; the window stays responsive meanwhile.
    pub fn open_path(&mut self, p: &Path) {
        self.open_in_background(p, false);
    }

    /// Opens a recovered copy: it is shown as unsaved and must be saved under a name.
    pub fn open_recovered(&mut self, p: &Path) {
        self.open_in_background(p, true);
    }

    pub(crate) fn open_in_background(&mut self, p: &Path, recovered: bool) {
        if self.opening.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let path = p.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("open-project".into())
            .spawn(move || {
                let r =
                    op_application::media::guarded("open project", || Editor::load_project(&path));
                let _ = tx.send(r);
            });
        match spawned {
            Ok(_) => {
                self.opening = Some(Opening {
                    path: p.to_path_buf(),
                    rx,
                    started: Instant::now(),
                    recovered,
                })
            }
            Err(e) => self.ed.error(format!("{}: {e}", p.display())),
        }
    }

    pub(crate) fn poll_opening(&mut self) {
        let Some(o) = &self.opening else { return };
        let result = match o.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the project could not be read".into())
            }
        };
        let o = self.opening.take().unwrap();
        match result {
            Ok(loaded) => {
                self.ed.install(loaded);
                self.after_open();
                if o.recovered {
                    // the copy is not the user's file: keep it unsaved until saved by name
                    self.ed.path = None;
                    self.ed.history.clear();
                    self.ed
                        .info(t("Recovered project opened; save it to keep it"));
                }
                if let Some(r) = self.ed.last_import_report.take() {
                    self.dialogs.push(Dialog::ImportReport(Box::new(r)));
                }
                log::info!("project ready in {} ms", o.started.elapsed().as_millis());
            }
            Err(e) => self.ed.error(format!("{}: {e}", o.path.display())),
        }
    }

    pub(crate) fn after_open(&mut self) {
        self.tl = self.tl.fresh();
        self.program.reset();
        self.source.reset();
        self.thumbs.clear();
        // files that moved: offer to find them, like Premiere's Link Media
        if let Some(d) = Dialog::link_media(self) {
            self.dialogs.push(d);
        }
    }

    /// Opens a project given on the command line or dropped on the window, or imports media.
    pub fn open_or_import(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let is_project = |p: &PathBuf| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "opproj" | "prproj" | "otio"
                )
            })
        };
        if paths.len() == 1 && is_project(&paths[0]) {
            let p = paths[0].clone();
            self.confirm(Then::Open(Some(p)));
            return;
        }
        // folders import their files, keeping one bin per folder
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let parent = self.ed.bin;
                if let Some(bin) = self.ed.edit("New Bin", |pr| Ok(pr.add_bin(parent, name))) {
                    let mut inner: Vec<PathBuf> = std::fs::read_dir(&p)
                        .map(|r| {
                            r.flatten()
                                .map(|e| e.path())
                                .filter(|x| x.is_file())
                                .collect()
                        })
                        .unwrap_or_default();
                    inner.sort();
                    self.ed.import(inner, bin);
                }
            } else {
                files.push(p);
            }
        }
        if !files.is_empty() {
            let bin = self.ed.bin;
            self.ed.import(files, bin);
        }
    }

    pub fn save(&mut self) -> bool {
        if self.ed.path.is_none() {
            return self.save_as();
        }
        match self.ed.save() {
            Ok(()) => true,
            Err(e) => {
                self.ed.error(tf("Could not save: {}", &[&e]));
                false
            }
        }
    }

    pub fn save_as(&mut self) -> bool {
        let name = format!("{}.{}", self.ed.project.name, op_project::native::EXTENSION);
        let mut d = rfd::FileDialog::new()
            .add_filter("OpenPremier", &[op_project::native::EXTENSION])
            .set_file_name(name);
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else {
            return false;
        };
        match self.ed.save_as(&path) {
            Ok(()) => true,
            Err(e) => {
                self.ed.error(tf("Could not save: {}", &[&e]));
                false
            }
        }
    }

    pub(crate) fn save_copy(&mut self) {
        let name = format!(
            "{} {}.{}",
            self.ed.project.name,
            t("Copy"),
            op_project::native::EXTENSION
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("OpenPremier", &[op_project::native::EXTENSION])
            .set_file_name(name)
            .save_file()
        {
            match self.ed.save_copy(&path) {
                Ok(()) => self.ed.info(tf("Saved a copy to {}", &[&path.display()])),
                Err(e) => self.ed.error(tf("Could not save: {}", &[&e])),
            }
        }
    }

    pub fn import_dialog(&mut self) {
        let mut all: Vec<&str> = vec![
            "mp4", "mov", "mkv", "avi", "mxf", "m4v", "webm", "mts", "m2ts", "mpg", "mpeg", "wmv",
            "flv", "3gp", "ts", "wav", "mp3", "aac", "m4a", "flac", "ogg", "opus", "aif", "aiff",
            "wma", "opproj", "prproj", "otio",
        ];
        all.extend(op_media::probe::STILL_EXTENSIONS);
        let mut d = rfd::FileDialog::new()
            .add_filter(t("All Supported Media"), &all)
            .add_filter(t("Images"), op_media::probe::STILL_EXTENSIONS)
            .add_filter(t("All Files"), &["*"]);
        if let Some(dir) = &self.ed.prefs.last_import_dir {
            d = d.set_directory(dir);
        }
        if let Some(files) = d.pick_files() {
            let bin = self.ed.bin;
            self.ed.import(files, bin);
        }
    }

    pub(crate) fn export_frame(&mut self) {
        let Some(sid) = self.ed.active else { return };
        let t0 = self.ed.playhead();
        let name = format!(
            "{}_{}.png",
            self.ed
                .active_seq()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            t0.ticks() / (TICKS_PER_SECOND / 1000)
        );
        let mut d = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name(name);
        if let Some(dir) = &self.ed.prefs.last_export_dir {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else { return };
        let snap = self.ed.snapshot();
        match op_application::export::export_frame(
            &snap,
            sid,
            t0,
            &path,
            self.gpu.clone(),
            self.ed.media.clone(),
        ) {
            Ok(()) => self.ed.info(tf("Exported frame to {}", &[&path.display()])),
            Err(e) => self.ed.error(tf("Export failed: {}", &[&e])),
        }
    }

    pub(crate) fn export_interchange(&mut self, kind: &str) {
        let Some(sid) = self.ed.active else { return };
        let p = &self.ed.project;
        let (filter, text) = match kind {
            "otio" => ("OpenTimelineIO", op_project::otio::export(p, sid)),
            "xml" => ("Final Cut Pro XML", op_project::fcpxml::export(p, sid)),
            _ => ("EDL", op_project::edl::export(p, sid)),
        };
        let text = match text {
            Ok(t) => t,
            Err(e) => {
                self.ed.error(tf("Export failed: {}", &[&e]));
                return;
            }
        };
        let name = format!(
            "{}.{kind}",
            self.ed
                .active_seq()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter, &[kind])
            .set_file_name(name)
            .save_file()
        {
            match std::fs::write(&path, text) {
                Ok(()) => self.ed.info(tf("Exported {}", &[&path.display()])),
                Err(e) => self.ed.error(tf("Export failed: {}", &[&e])),
            }
        }
    }

    /// The export shown in the Program Monitor: the newest one still running.
    /// Graphics > Captions > Import Captions File: SRT or WebVTT into caption clips.
    pub(crate) fn import_captions(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(t("Import Captions File..."))
            .add_filter(t("Captions"), &["srt", "vtt"])
            .pick_file()
        else {
            return;
        };
        let opts = self.caption_options.clone();
        match self.ed.import_captions(&path, &opts) {
            Ok(n) => self.ed.info(format!("Created {n} captions")),
            Err(e) => self.ed.error(e),
        }
    }

    /// Graphics > Captions > Export Captions File: SRT (or WebVTT by extension).
    pub(crate) fn export_captions(&mut self) {
        let name = self
            .ed
            .active_seq()
            .map(|q| q.name.clone())
            .unwrap_or_else(|| "Captions".into());
        let Some(path) = rfd::FileDialog::new()
            .set_title(t("Export Captions File..."))
            .set_file_name(format!("{name}.srt"))
            .add_filter("SubRip (.srt)", &["srt"])
            .add_filter("WebVTT (.vtt)", &["vtt"])
            .save_file()
        else {
            return;
        };
        match self.ed.export_captions(&path) {
            Ok(n) => self.ed.info(format!("Exported {n} captions")),
            Err(e) => self.ed.error(e),
        }
    }
}
