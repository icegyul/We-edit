//! The status bar: messages, and the progress of captions, proxies and exports.

use super::*;

impl State {
    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            if let Some(st) = &self.ed.status
                && st.at.elapsed() < Duration::from_secs(8)
            {
                let c = if st.error { theme::ERROR } else { theme::TEXT };
                ui.label(
                    RichText::new(translate_status(&st.text))
                        .color(c)
                        .size(12.0),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // exports sit in the bottom right corner, like background tasks
                self.export_status(ui);
                self.proxy_status(ui);
                self.caption_status(ui);
                ui.label(
                    RichText::new(self.gpu.description())
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );
                let conforming = self.ed.media.conforming();
                if conforming > 0 {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Conforming audio ({})", &[&conforming]))
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );
                }
                if self.ed.importing > 0 {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Importing {} files", &[&self.ed.importing]))
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        });
    }

    /// Automatic captions in progress: stage, progress and Cancel.
    pub(crate) fn caption_status(&mut self, ui: &mut Ui) {
        let Some(job) = &self.ed.captioning else {
            return;
        };
        let p = job.progress();
        if ui.small_button(t("Cancel")).clicked() {
            job.cancel();
        }
        let stage = match p.stage {
            op_application::captions::CaptionStage::Downloading => t("Downloading speech model"),
            op_application::captions::CaptionStage::MixingAudio => t("Preparing audio"),
            op_application::captions::CaptionStage::Transcribing => t("Transcribing"),
        };
        ui.add(
            egui::ProgressBar::new(p.fraction)
                .desired_width(140.0)
                .text(format!("{:.0} %", p.fraction * 100.0)),
        );
        ui.label(RichText::new(stage).size(12.0));
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    /// Proxy creation in progress: file, progress and Cancel.
    pub(crate) fn proxy_status(&mut self, ui: &mut Ui) {
        if !self.ed.proxies.busy() {
            return;
        }
        let p = self.ed.proxies.progress();
        if ui.small_button(t("Cancel")).clicked() {
            self.ed.proxies.cancel();
        }
        let n = (p.done + 1).min(p.total.max(1));
        ui.add(
            egui::ProgressBar::new(p.fraction)
                .desired_width(140.0)
                .text(format!("{:.0} %", p.fraction * 100.0)),
        )
        .on_hover_text(p.current.clone().unwrap_or_default());
        ui.label(RichText::new(tf("Creating proxies ({}/{})", &[&n, &p.total])).size(12.0));
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    /// One entry per export, right to left: progress with pause and cancel while it runs (its
    /// frames show in the Program Monitor), then the result until it expires or is closed.
    pub(crate) fn export_status(&mut self, ui: &mut Ui) {
        let mut cancel = Vec::new();
        let mut pause = Vec::new();
        let mut folder = None;
        let ui_state = &mut self.export_ui;
        for (i, job) in self.ed.exports.iter().enumerate().rev() {
            if ui_state.dismissed.contains(&i) {
                continue;
            }
            let p = job.progress.lock().clone();
            let name = job
                .settings
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !p.done {
                if ui.small_button(t("Cancel")).clicked() {
                    cancel.push(i);
                }
                let label = if p.paused { t("Resume") } else { t("Pause") };
                if ui.small_button(label).clicked() {
                    pause.push((i, !p.paused));
                }
                let text = if p.preparing {
                    t("Preparing audio...").to_string()
                } else if p.paused {
                    format!("{}  {:.0} %", t("Paused"), p.fraction() * 100.0)
                } else {
                    let eta = p.remaining.map(widgets::clock).unwrap_or_default();
                    format!("{:.0} %  {eta}", p.fraction() * 100.0)
                };
                ui.add(
                    egui::ProgressBar::new(p.fraction())
                        .desired_width(170.0)
                        .text(text),
                )
                .on_hover_ui(|ui| export_details(ui, job, &p));
                ui.label(RichText::new(tf("Exporting {}", &[&name])).size(12.0));
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                continue;
            }
            let since = *ui_state.finished_at.entry(i).or_insert_with(Instant::now);
            if let Some(e) = &p.error {
                if ui.small_button("x").on_hover_text(t("Close")).clicked() {
                    ui_state.dismissed.insert(i);
                }
                let mut details = translate_status(&format!("Export failed: {e}"));
                if e.starts_with("cannot write ") && cfg!(windows) {
                    details.push_str("\n\n");
                    details.push_str(t(
                        "Windows may be blocking this folder (Controlled folder access). Allow OpenPremier in Windows Security or choose another folder.",
                    ));
                }
                ui.label(
                    RichText::new(tf("Export of {} failed", &[&name]))
                        .size(12.0)
                        .color(theme::ERROR),
                )
                .on_hover_text(details);
            } else if p.cancelled || since.elapsed() > Duration::from_secs(15) {
                ui_state.dismissed.insert(i);
            } else {
                if ui.small_button(t("Open Folder")).clicked() {
                    folder = job.settings.path.parent().map(|d| d.to_path_buf());
                }
                ui.label(RichText::new(tf("Exported {}", &[&name])).size(12.0))
                    .on_hover_text(job.settings.path.display().to_string());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
        }
        for i in cancel {
            self.ed.exports[i].cancel();
        }
        for (i, paused) in pause {
            self.ed.exports[i].set_paused(paused);
        }
        if let Some(dir) = folder {
            open_folder(&dir);
        }
    }
}

/// Details of a running export, shown over its progress bar.
pub(crate) fn export_details(
    ui: &mut Ui,
    job: &op_application::ExportJob,
    p: &op_application::Progress,
) {
    ui.label(
        RichText::new(job.settings.path.display().to_string())
            .strong()
            .family(crate::fonts::strong()),
    );
    ui.label(tf("Frame {} of {}", &[&p.frame, &p.total]));
    ui.label(tf("Elapsed: {}", &[&widgets::clock(p.elapsed)]));
    if let Some(r) = p.remaining {
        ui.label(tf("Remaining: {}", &[&widgets::clock(r)]));
    }
    if p.fps > 0.0 {
        ui.label(format!("{:.1} fps", p.fps));
    }
    if !p.encoder.is_empty() {
        ui.label(RichText::new(&p.encoder).color(theme::TEXT_DIM));
    }
}

/// Translates the editor's status messages (written in English by the application layer).
pub(crate) fn translate_status(text: &str) -> String {
    if i18n::current() == i18n::Lang::En {
        return text.to_string();
    }
    for (prefix, fmt) in [
        ("Undo ", "Undo {}"),
        ("Redo ", "Redo {}"),
        ("Saved ", "Saved {}"),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = if prefix == "Saved " {
                rest.to_string()
            } else {
                tn(rest)
            };
            return tf(fmt, &[&rest]);
        }
    }
    if let Some(n) = text
        .strip_prefix("Imported ")
        .and_then(|r| r.strip_suffix(" files"))
    {
        return tf("Imported {} files", &[&n]);
    }
    if let Some(rest) = text.strip_prefix("Created ") {
        for (sep, fmt) in [
            (" captions in ", "Created {} captions in {}"),
            (
                " captions translated from ",
                "Created {} captions translated from {}",
            ),
        ] {
            if let Some((n, lang)) = rest.split_once(sep) {
                return tf(fmt, &[&n, &tn(lang)]);
            }
        }
    }
    if let Some(n) = text.strip_suffix(" media files are offline") {
        return tf("{} media files are offline", &[&n]);
    }
    // messages with one variable part, from the editing engine and the exporter
    const PATTERNS: [(&str, &str, &str); 17] = [
        ("Linked ", " media files", "Linked {} media files"),
        (
            "Assistants cannot connect: ",
            "",
            "Assistants cannot connect: {}",
        ),
        ("Not linked: ", "", "Not linked: {}"),
        ("track ", " is locked", "Track {} is locked"),
        ("clips would overlap on ", "", "Clips would overlap on {}"),
        ("not enough media: ", "", "Not enough media: {}"),
        ("not found: ", "", "Not found: {}"),
        (
            "Export failed: cannot write ",
            "",
            "Export failed: cannot write {}",
        ),
        ("Export failed: ", "", "Export failed: {}"),
        ("Created ", " captions", "Created {} captions"),
        ("Exported ", " captions", "Exported {} captions"),
        (
            "Style applied to ",
            " captions",
            "Style applied to {} captions",
        ),
        ("Captions not created: ", "", "Captions not created: {}"),
        ("Exported ", "", "Exported {}"),
        ("Proxies ready (", ")", "Proxies ready ({})"),
        ("Proxy not created: ", "", "Proxy not created: {}"),
        (
            "This project was saved by a newer version (format ",
            ").",
            "This project was saved by a newer version (format {}).",
        ),
    ];
    for (prefix, suffix, fmt) in PATTERNS {
        if let Some(rest) = text.strip_prefix(prefix)
            && let Some(mid) = rest.strip_suffix(suffix)
        {
            return tf(fmt, &[&tn(mid)]);
        }
    }
    tn(text)
}
