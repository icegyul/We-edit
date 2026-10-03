//! Pasting media from the system clipboard (files, images, image links).

use super::*;

impl State {
    /// Pastes media from the system clipboard. Returns false when it holds none.
    pub(crate) fn paste_media(&mut self) -> bool {
        use crate::paste::{self, Found, Job, Payload};
        let Some(found) = paste::read() else {
            return false;
        };
        // over the tracks: where the pointer is; else at the playhead when a timeline-side
        // panel has the focus; else only into the project
        let place = match self.tl.pointer_target(self.ctx.cumulative_pass_nr()) {
            Some((t, track)) => Some((Some(t), track)),
            None => (self.ed.active.is_some()
                && !matches!(self.focus, Focus::Project | Focus::Source | Focus::Effects))
            .then_some((None, None)),
        };
        let bin = self.ed.project.root;
        let (data, name, source) = match found {
            Found::Files(files) => {
                log::info!("pasting {} files", files.len());
                if let Some((at, track)) = place {
                    self.ed.place_after_import.extend(files.iter().map(|f| {
                        op_application::Placement {
                            path: f.clone(),
                            at,
                            track,
                        }
                    }));
                }
                self.ed.import(files, bin);
                return true;
            }
            Found::Bitmap(b) => {
                let stamp = op_application::autosave::stamp(std::time::SystemTime::now());
                (
                    Arc::new(parking_lot::Mutex::new(Some(Ok(Payload::Bitmap(b))))),
                    format!("{} {stamp}", t("Pasted Image")),
                    t("Image from the clipboard").to_string(),
                )
            }
            Found::Url(url) => {
                let slot = Arc::new(parking_lot::Mutex::new(None));
                let (s2, u2) = (slot.clone(), url.clone());
                let _ = std::thread::Builder::new()
                    .name("paste-download".into())
                    .spawn(move || {
                        let r = paste::download(&u2);
                        if let Err(e) = &r {
                            log::warn!("pasted link not downloaded: {u2}: {e}");
                        }
                        *s2.lock() = Some(r);
                    });
                let name = paste::name_from_url(&url).unwrap_or_else(|| t("Pasted Image").into());
                (slot, name, url)
            }
        };
        let id = self.next_paste;
        self.next_paste += 1;
        let folder = self.paste_folder();
        let auto = self.ed.prefs.paste_always && self.ed.prefs.paste_folder.is_some();
        self.pastes.push(Job {
            id,
            name: name.clone(),
            source,
            data,
            folder: auto.then(|| folder.clone()),
            place,
            preview: None,
        });
        if !auto {
            self.dialogs.push(Dialog::PasteMedia {
                id,
                folder: folder.display().to_string(),
                name,
                always: false,
            });
        }
        true
    }

    /// Where pasted images go by default: the chosen folder, else next to the project, else
    /// the user's pictures.
    pub fn paste_folder(&self) -> PathBuf {
        if let Some(f) = &self.ed.prefs.paste_folder {
            return f.clone();
        }
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            return dir.join(t("Pasted Media"));
        }
        // Pictures on Windows and macOS, the XDG pictures folder on Linux
        dirs::picture_dir()
            .filter(|p| p.is_dir())
            .or_else(dirs::home_dir)
            .unwrap_or_default()
            .join("OpenPremier")
    }

    /// Saves and imports pasted images whose folder is known and whose data is ready.
    pub(crate) fn poll_pastes(&mut self) {
        let mut i = 0;
        while i < self.pastes.len() {
            let job = &self.pastes[i];
            let Some(folder) = job.folder.clone() else {
                i += 1;
                continue;
            };
            let Some(result) = job.data.lock().take() else {
                i += 1;
                continue;
            };
            let job = self.pastes.remove(i);
            match result.and_then(|p| crate::paste::save(&p, &folder, &job.name)) {
                Ok(path) => {
                    log::info!("pasted image saved to {}", path.display());
                    if let Some((at, track)) = job.place {
                        self.ed.place_after_import.push(op_application::Placement {
                            path: path.clone(),
                            at,
                            track,
                        });
                    }
                    let bin = self.ed.project.root;
                    self.ed.import(vec![path], bin);
                }
                Err(e) => self
                    .ed
                    .error(tf("The pasted image could not be saved: {}", &[&e])),
            }
        }
        if !self.pastes.is_empty() {
            self.ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    pub(crate) fn clipboard_text(&self) {
        let names: Vec<String> = self
            .ed
            .clipboard
            .clips
            .iter()
            .map(|(_, c)| c.name.clone())
            .collect();
        self.ctx.copy_text(names.join("\n"));
    }
}
