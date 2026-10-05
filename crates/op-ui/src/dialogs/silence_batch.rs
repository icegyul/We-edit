//! Clip > Remove Silence from Files: the same settings applied to many video and audio files, each
//! cut and written to a folder, one after the other, in the background.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use op_application::silence::{FileCut, SilenceSettings, cut_file, output_name};
use parking_lot::Mutex;

use super::silence::settings_panel;
use super::*;

#[derive(Clone)]
enum Status {
    Waiting,
    Working,
    Done(FileCut),
    Failed(String),
}

/// A batch in progress, shared with its thread.
struct Run {
    status: Mutex<Vec<Status>>,
    cancel: AtomicBool,
}

pub struct BatchForm {
    settings: SilenceSettings,
    files: Vec<PathBuf>,
    out_dir: String,
    suffix: String,
    run: Option<Arc<Run>>,
}

impl Dialog {
    pub fn silence_batch(s: &State) -> Dialog {
        Dialog::SilenceBatch(Box::new(BatchForm {
            settings: s.silence_settings.clone(),
            files: Vec::new(),
            out_dir: String::new(),
            suffix: "_cut".into(),
            run: None,
        }))
    }
}

const MEDIA: &[&str] = &[
    "mp4", "mov", "mkv", "avi", "mxf", "m4v", "webm", "mts", "m2ts", "mpg", "mpeg", "wmv", "flv",
    "3gp", "ts", "wav", "mp3", "aac", "m4a", "flac", "ogg", "opus", "aif", "aiff", "wma",
];

fn running(form: &BatchForm) -> bool {
    form.run.as_ref().is_some_and(|r| {
        r.status
            .lock()
            .iter()
            .any(|s| matches!(s, Status::Waiting | Status::Working))
    })
}

fn start(s: &State, form: &mut BatchForm) {
    let out = if form.out_dir.trim().is_empty() {
        form.files
            .first()
            .and_then(|f| f.parent())
            .map(|p| p.join(t("Silence Removed")))
            .unwrap_or_default()
    } else {
        PathBuf::from(form.out_dir.trim())
    };
    form.out_dir = out.display().to_string();
    let run = Arc::new(Run {
        status: Mutex::new(vec![Status::Waiting; form.files.len()]),
        cancel: AtomicBool::new(false),
    });
    form.run = Some(run.clone());
    let (files, settings, suffix) = (
        form.files.clone(),
        form.settings.clone(),
        form.suffix.clone(),
    );
    let (dirs, prefs, gpu) = (s.ed.dirs.clone(), s.ed.prefs.clone(), s.gpu.clone());
    let _ = std::thread::Builder::new()
        .name("silence-batch".into())
        .spawn(move || {
            for (i, file) in files.iter().enumerate() {
                if run.cancel.load(Ordering::Relaxed) {
                    run.status.lock()[i] = Status::Failed("cancelled".into());
                    continue;
                }
                run.status.lock()[i] = Status::Working;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut ed = op_application::Editor::new(dirs.clone(), prefs.clone(), false);
                    ed.background = true;
                    let output = output_name(&out, file, &suffix);
                    cut_file(&mut ed, &gpu, file, &output, &settings, &run.cancel)
                }))
                .unwrap_or_else(|_| Err("it stopped unexpectedly".into()));
                run.status.lock()[i] = match result {
                    Ok(d) => Status::Done(d),
                    Err(e) => Status::Failed(e),
                };
            }
        });
}

pub(super) fn silence_batch_dialog(
    s: &mut State,
    ctx: &egui::Context,
    form: &mut BatchForm,
) -> bool {
    let busy = running(form);
    if busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
    let statuses: Vec<Status> = form
        .run
        .as_ref()
        .map(|r| r.status.lock().clone())
        .unwrap_or_default();
    let mut go = false;
    let mut close = false;
    let (_, esc) = modal(
        ctx,
        "silence-batch",
        t("Remove Silence from Files"),
        860.0,
        |ui| {
            ui.add(
            egui::Label::new(
                RichText::new(t(
                    "Each file is cut with these settings and written to the folder below; the originals are not touched.",
                ))
                .color(theme::TEXT_DIM),
            )
            .wrap(),
        );
            ui.add_space(6.0);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(350.0);
                    egui::ScrollArea::vertical()
                        .id_salt("sil-batch-settings")
                        .max_height(400.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.add_enabled_ui(!busy, |ui| settings_panel(ui, &mut form.settings));
                        });
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!busy, egui::Button::new(t("Add Files...")))
                            .clicked()
                            && let Some(files) = rfd::FileDialog::new()
                                .add_filter(t("Video and Audio"), MEDIA)
                                .pick_files()
                        {
                            for f in files {
                                if !form.files.contains(&f) {
                                    form.files.push(f);
                                }
                            }
                            form.run = None;
                        }
                        if ui
                            .add_enabled(
                                !busy && !form.files.is_empty(),
                                egui::Button::new(t("Clear")),
                            )
                            .clicked()
                        {
                            form.files.clear();
                            form.run = None;
                        }
                    });
                    egui::ScrollArea::vertical()
                        .id_salt("sil-batch-files")
                        .max_height(350.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if form.files.is_empty() {
                                ui.add_space(30.0);
                                ui.label(
                                    RichText::new(t("Add the files to cut."))
                                        .color(theme::TEXT_DIM),
                                );
                            }
                            let mut remove: Option<usize> = None;
                            for (i, f) in form.files.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let (text, color) = match statuses.get(i) {
                                        Some(Status::Working) => {
                                            (t("Working...").to_string(), theme::ACCENT)
                                        }
                                        Some(Status::Done(d)) => (
                                            tf(
                                                "{} cuts, {} s removed",
                                                &[&d.count, &format!("{:.1}", d.removed)],
                                            ),
                                            egui::Color32::from_rgb(110, 200, 140),
                                        ),
                                        Some(Status::Failed(e)) => (e.clone(), theme::ERROR),
                                        _ => (String::new(), theme::TEXT_DIM),
                                    };
                                    if !busy
                                        && ui.small_button("x").on_hover_text(t("Remove")).clicked()
                                    {
                                        remove = Some(i);
                                    }
                                    ui.label(
                                        RichText::new(
                                            f.file_name()
                                                .map(|n| n.to_string_lossy().into_owned())
                                                .unwrap_or_default(),
                                        )
                                        .color(theme::TEXT_BRIGHT),
                                    )
                                    .on_hover_text(f.display().to_string());
                                    ui.label(RichText::new(text).size(11.0).color(color));
                                });
                            }
                            if let Some(i) = remove {
                                form.files.remove(i);
                                form.run = None;
                            }
                        });
                });
            });
            ui.add_space(6.0);
            ui.separator();
            egui::Grid::new("sil-batch-out")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label(t("Output Folder"));
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            !busy,
                            egui::TextEdit::singleline(&mut form.out_dir)
                                .hint_text(t("Next to the first file"))
                                .desired_width(380.0),
                        );
                        if ui
                            .add_enabled(!busy, egui::Button::new(t("Browse...")))
                            .clicked()
                            && let Some(d) = rfd::FileDialog::new().pick_folder()
                        {
                            form.out_dir = d.display().to_string();
                        }
                    });
                    ui.end_row();
                    ui.label(t("Add to the Name"));
                    ui.add_enabled(
                        !busy,
                        egui::TextEdit::singleline(&mut form.suffix).desired_width(120.0),
                    );
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let done = statuses
                    .iter()
                    .filter(|s| matches!(s, Status::Done(_)))
                    .count();
                if !statuses.is_empty() {
                    ui.label(
                        RichText::new(tf("{} of {} files", &[&done, &statuses.len()]))
                            .color(theme::TEXT_DIM),
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if busy {
                        if ui.button(t("Stop")).clicked()
                            && let Some(r) = &form.run
                        {
                            r.cancel.store(true, Ordering::Relaxed);
                        }
                    } else {
                        if ui.button(t("Close")).clicked() {
                            close = true;
                        }
                        if ui
                            .add_enabled(
                                !form.files.is_empty(),
                                egui::Button::new(
                                    RichText::new(t("Start")).color(egui::Color32::WHITE),
                                )
                                .fill(theme::ACCENT_DIM),
                            )
                            .clicked()
                        {
                            go = true;
                        }
                    }
                });
            });
        },
    );
    if go {
        start(s, form);
    }
    if (close || esc) && !busy {
        s.silence_settings = form.settings.clone();
        return false;
    }
    true
}
