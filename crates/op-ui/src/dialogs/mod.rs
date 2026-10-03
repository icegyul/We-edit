//! Dialogs: unsaved changes, recovery, new sequence and settings, export, speed/duration, gain,
//! paste attributes, nest, color matte, tracks, interpret footage, keyboard shortcuts,
//! preferences, project settings, import report and about.

use std::path::{Path, PathBuf};

use egui::{RichText, Ui};
use op_application::captions::ModelSize;
use op_application::keymap::{self, Binding, Chord, Keymap};
use op_application::performance::{Hardware, Profile};
use op_application::{ExportJob, ExportSettings};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, VideoCodec, VideoSettings};
use op_project::prproj::ImportReport;
use op_timeline::Attributes;

use crate::app::State;
use crate::i18n::{self, t, tf, tn};
use crate::{keys, theme, widgets};

mod export;
mod media;
mod paste;
mod preferences;
mod sequence;
mod shortcuts;

use export::*;
use media::*;
use paste::*;
pub use preferences::hardware_summary;
use preferences::*;
use sequence::*;
use shortcuts::*;

/// What to do after the unsaved-changes question.
pub enum Then {
    NewProject,
    Open(Option<PathBuf>),
    Quit,
}

pub struct ExportForm {
    sequence: SequenceId,
    path: PathBuf,
    preset: usize,
    video: bool,
    codec: VideoCodec,
    width: u32,
    height: u32,
    rate: Rate,
    use_bitrate: bool,
    bitrate: u32,
    quality: u8,
    hardware: bool,
    audio: bool,
    audio_codec: AudioCodec,
    audio_rate: u32,
    channels: u16,
    audio_bitrate: u32,
    in_out: bool,
    has_in_out: bool,
    seq_size: (u32, u32),
    seq_rate: Rate,
}

pub struct ShortcutsForm {
    search: String,
    capture: Option<(String, String)>,
    message: Option<String>,
}

/// File > Link Media: the files a project points to that are not there any more.
pub struct LinkForm {
    pub missing: Vec<op_application::relink::Missing>,
    /// Files chosen or found for missing assets.
    pub found: std::collections::HashMap<AssetId, PathBuf>,
    pub search: Option<op_application::relink::Search>,
}

/// Graphics > Captions > Transcribe and Create Captions.
pub struct CaptionForm {
    /// Index into CAPTION_LANGUAGES.
    pub language: usize,
    /// Captions translated into English instead of in the language spoken.
    pub translate: bool,
    pub model: op_application::captions::ModelSize,
    pub options: op_application::captions::CaptionOptions,
}

pub enum Dialog {
    Unsaved(Then),
    /// Closing the program while an export runs.
    ExportRunning,
    Recover(PathBuf),
    ImportReport(Box<ImportReport>),
    NewSequence {
        name: String,
        settings: SequenceSettings,
    },
    SequenceSettings {
        name: String,
        settings: SequenceSettings,
    },
    ProjectSettings,
    Export(Box<ExportForm>),
    Speed {
        percent: f64,
        duration: Dur,
        reverse: bool,
        ripple: bool,
        linked: bool,
        base: Dur,
        base_percent: f64,
        rate: Rate,
    },
    Gain {
        /// 0 set, 1 adjust, 2 normalize max peak, 3 normalize all peaks
        mode: u8,
        db: f64,
        adjust: f64,
        peak_target: f64,
    },
    PasteAttributes(Attributes),
    RemoveAttributes(Attributes),
    Nest(String),
    RenameClip(ClipId, String),
    ColorMatte {
        color: [f32; 3],
        name: String,
    },
    AddTracks {
        video: usize,
        audio: usize,
    },
    Captions(Box<CaptionForm>),
    LinkMedia(Box<LinkForm>),
    /// First start: choose the recommended performance profile or pick one.
    PerformanceSetup {
        manual: bool,
        choice: Profile,
    },
    Interpret {
        asset: AssetId,
        fps: f64,
        alpha_ignore: bool,
    },
    Shortcuts(Box<ShortcutsForm>),
    Preferences {
        /// Interface scale being chosen; applied with the Apply button.
        scale: f32,
    },
    Log {
        level: usize,
        filter: String,
    },
    /// Where to save an image pasted from the clipboard.
    PasteMedia {
        id: u64,
        folder: String,
        name: String,
        always: bool,
    },
    About,
}

impl Dialog {
    pub fn new_sequence(settings: SequenceSettings) -> Dialog {
        Dialog::NewSequence {
            name: t("Sequence 01").to_string(),
            settings,
        }
    }

    /// Link Media for the current project, or None when nothing is missing.
    pub fn link_media(s: &State) -> Option<Dialog> {
        let missing = op_application::relink::offline(&s.ed.project);
        (!missing.is_empty()).then(|| {
            Dialog::LinkMedia(Box::new(LinkForm {
                missing,
                found: Default::default(),
                search: None,
            }))
        })
    }

    pub fn captions(s: &State) -> Dialog {
        Dialog::Captions(Box::new(CaptionForm {
            language: s.caption_language,
            translate: s.caption_translate,
            model: s.caption_model,
            options: s.caption_options.clone(),
        }))
    }

    pub fn export(s: &State) -> Option<Dialog> {
        let seq = s.ed.active_seq()?;
        let dir =
            s.ed.prefs
                .last_export_dir
                .clone()
                .or_else(|| {
                    s.ed.path
                        .as_ref()
                        .and_then(|p| p.parent().map(|x| x.to_path_buf()))
                })
                .or_else(dirs_video)
                .unwrap_or_default();
        let mut f = ExportForm {
            sequence: seq.id,
            path: dir.join(format!("{}.mp4", seq.name)),
            preset: 0,
            video: true,
            codec: VideoCodec::H264,
            width: seq.settings.width,
            height: seq.settings.height,
            rate: seq.rate(),
            use_bitrate: false,
            bitrate: 20,
            quality: 18,
            hardware: s.ed.prefs.hardware_encoding,
            audio: true,
            audio_codec: AudioCodec::Aac,
            audio_rate: seq.settings.audio_rate,
            channels: seq.settings.master.channels().min(2) as u16,
            audio_bitrate: 320,
            in_out: seq.mark_in.is_some() && seq.mark_out.is_some(),
            has_in_out: seq.mark_in.is_some() || seq.mark_out.is_some(),
            seq_rate: seq.rate(),
            seq_size: (seq.settings.width, seq.settings.height),
        };
        apply_preset(&mut f, 0);
        Some(Dialog::Export(Box::new(f)))
    }

    pub fn speed(s: &State) -> Option<Dialog> {
        let seq = s.ed.active_seq()?;
        let c = s.ed.selection.clips.first().and_then(|c| seq.clip(*c))?;
        Some(Dialog::Speed {
            percent: c.speed.percent(),
            duration: c.duration,
            reverse: c.reverse,
            ripple: false,
            linked: true,
            base: c.duration,
            base_percent: c.speed.percent(),
            rate: seq.rate(),
        })
    }

    pub fn gain(s: &State) -> Option<Dialog> {
        let seq = s.ed.active_seq()?;
        let c =
            s.ed.selection
                .clips
                .iter()
                .filter_map(|c| seq.clip(*c))
                .find(|c| !c.is_video())?;
        Some(Dialog::Gain {
            mode: 0,
            db: c.gain_db,
            adjust: 0.0,
            peak_target: 0.0,
        })
    }

    pub fn preferences(s: &State) -> Dialog {
        Dialog::Preferences {
            scale: s.ed.prefs.ui_scale,
        }
    }

    pub fn log() -> Dialog {
        Dialog::Log {
            level: 2,
            filter: String::new(),
        }
    }

    pub fn shortcuts() -> Dialog {
        Dialog::Shortcuts(Box::new(ShortcutsForm {
            search: String::new(),
            capture: None,
            message: None,
        }))
    }
}

/// Shows the open dialogs. Each returns whether it stays open.
pub fn show(s: &mut State, ctx: &egui::Context) {
    let list = std::mem::take(&mut s.dialogs);
    let mut keep = Vec::new();
    for mut d in list {
        if dialog(s, ctx, &mut d) {
            keep.push(d);
        }
    }
    keep.append(&mut s.dialogs);
    s.dialogs = keep;
}

/// Every window opens centered on the program window.
fn window<'a>(ctx: &egui::Context, title: &'a str) -> egui::Window<'a> {
    egui::Window::new(title)
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .constrain(true)
        .collapsible(false)
}

/// Languages offered for transcription: Whisper code (empty: detect) and name.
const CAPTION_LANGUAGES: &[(&str, &str)] = &[
    ("", "Detect Automatically"),
    ("es", "Español"),
    ("en", "English"),
    ("pt", "Português"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("it", "Italiano"),
    ("ca", "Català"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("ru", "Русский"),
    ("tr", "Türkçe"),
    ("ar", "العربية"),
    ("hi", "हिन्दी"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("zh", "中文"),
];

/// The transcription language offered first: the program's language, except English, whose
/// users more often caption other languages (they get automatic detection).
pub fn default_caption_language() -> usize {
    let ui = crate::i18n::current().code();
    if ui == "en" {
        return 0;
    }
    CAPTION_LANGUAGES
        .iter()
        .position(|(code, _)| *code == ui)
        .unwrap_or(0)
}

/// What each caption style does, for the dialog.
fn caption_style_hint(i: usize) -> &'static str {
    match i {
        0 => "White text with an outline, like classic subtitles",
        1 => "Text on a solid box",
        2 => "Each word fills with the highlight color as it is said",
        3 => "The word being said sits on a colored box",
        4 => "The word being said grows and changes color",
        5 => "One big word at a time, popping in",
        6 => "Words appear as they are said",
        7 => "Words bounce in as they are said",
        8 => "Glowing neon text; the word being said burns brighter",
        9 => "Bold capitals with a thick outline; the word being said is highlighted",
        10 => "Words fade in from faint as they are said",
        _ => "A line grows under the word being said",
    }
}

fn buttons(ui: &mut Ui, ok: &str) -> (bool, bool) {
    let mut r = (false, false);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(t("Cancel")).clicked() {
                r.1 = true;
            }
            if ui
                .add(
                    egui::Button::new(RichText::new(ok).color(egui::Color32::WHITE))
                        .fill(theme::ACCENT_DIM),
                )
                .clicked()
            {
                r.0 = true;
            }
        });
    });
    r
}

fn modal<R>(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
) -> (R, bool) {
    // dialogs get the room of a window, not the tight margin of a menu
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(16));
    let resp = egui::Modal::new(egui::Id::new(id))
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.label(
                RichText::new(title)
                    .size(16.0)
                    .family(crate::fonts::strong())
                    .color(theme::TEXT_BRIGHT),
            );
            ui.add_space(10.0);
            body(ui)
        });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    (resp.inner, esc)
}

fn dialog(s: &mut State, ctx: &egui::Context, d: &mut Dialog) -> bool {
    match d {
        Dialog::Unsaved(then) => {
            let name =
                s.ed.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| s.ed.project.name.clone());
            let (choice, esc) = modal(ctx, "unsaved", t("Save changes?"), 380.0, |ui| {
                ui.label(tf("Save changes to \"{}\" before closing?", &[&name]));
                ui.add_space(10.0);
                let mut c = 0;
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("Cancel")).clicked() {
                            c = 3;
                        }
                        if ui.button(t("Don't Save")).clicked() {
                            c = 2;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(t("Save")).color(egui::Color32::WHITE),
                                )
                                .fill(theme::ACCENT_DIM),
                            )
                            .clicked()
                        {
                            c = 1;
                        }
                    });
                });
                c
            });
            let then = std::mem::replace(then, Then::NewProject);
            match choice {
                1 => {
                    if s.save() {
                        s.proceed(then);
                    }
                    false
                }
                2 => {
                    s.proceed(then);
                    false
                }
                3 => false,
                _ if esc => false,
                _ => {
                    *d = Dialog::Unsaved(then);
                    true
                }
            }
        }
        Dialog::ExportRunning => {
            let (choice, esc) = modal(
                ctx,
                "export-running",
                t("An export is running"),
                380.0,
                |ui| {
                    ui.label(t(
                        "Closing the program stops the export and deletes the unfinished file.",
                    ));
                    ui.add_space(10.0);
                    let mut c = 0;
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(t("Keep Exporting")).clicked() {
                                c = 2;
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(t("Stop and Exit"))
                                            .color(egui::Color32::WHITE),
                                    )
                                    .fill(theme::ACCENT_DIM),
                                )
                                .clicked()
                            {
                                c = 1;
                            }
                        });
                    });
                    c
                },
            );
            if choice == 1 {
                for job in &s.ed.exports {
                    if !job.finished() {
                        job.cancel();
                    }
                }
                s.command("cmd.file.exit");
            }
            choice == 0 && !esc
        }
        Dialog::Recover(path) => {
            let p = path.clone();
            let (choice, esc) = modal(ctx, "recover", t("Recover your work?"), 420.0, |ui| {
                ui.label(t("OpenPremier did not close normally last time. An autosaved copy of your project is available."));
                ui.label(
                    RichText::new(p.display().to_string())
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );
                let mut c = 0;
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(t("Ignore")).clicked() {
                            c = 2;
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(t("Open Autosave")).color(egui::Color32::WHITE),
                                )
                                .fill(theme::ACCENT_DIM),
                            )
                            .clicked()
                        {
                            c = 1;
                        }
                    });
                });
                c
            });
            if choice == 1 {
                s.open_recovered(&p);
                // the recovered copy must be saved somewhere new
                s.ed.path = None;
            }
            choice == 0 && !esc
        }
        Dialog::ImportReport(r) => {
            let mut open = true;
            window(ctx, t("Import Report"))
                .open(&mut open)
                .collapsible(false)
                .default_width(460.0)
                .show(ctx, |ui| {
                    egui::Grid::new("report").num_columns(2).show(ui, |ui| {
                        for (k, v) in [
                            (t("Bins"), r.bins),
                            (t("Media"), r.media),
                            (t("Sequences"), r.sequences),
                            (t("Clips"), r.clips),
                            (t("Markers"), r.markers),
                            (t("Transitions"), r.transitions),
                            (t("Effects imported"), r.components_mapped),
                            (
                                t("Effects replaced by an equivalent"),
                                r.components_equivalent,
                            ),
                            (t("Effects kept but not rendered"), r.components_opaque),
                        ] {
                            ui.label(k);
                            ui.label(v.to_string());
                            ui.end_row();
                        }
                    });
                    if !r.skipped.is_empty() {
                        ui.separator();
                        ui.label(RichText::new(t("Not imported")).color(theme::WARN));
                        for (why, n) in &r.skipped {
                            ui.label(format!("{n} x {why}"));
                        }
                    }
                    if !r.warnings.is_empty() {
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(160.0)
                            .show(ui, |ui| {
                                for w in &r.warnings {
                                    ui.label(RichText::new(w).size(11.0).color(theme::TEXT_DIM));
                                }
                            });
                    }
                });
            open
        }
        Dialog::NewSequence { name, settings } => {
            let ((ok, cancel), esc) = modal(ctx, "newseq", t("New Sequence"), 460.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Sequence Name"));
                    ui.text_edit_singleline(name);
                });
                ui.add_space(6.0);
                sequence_form(ui, settings, true);
                buttons(ui, t("OK"))
            });
            if ok {
                let (n, st) = (name.clone(), settings.clone());
                s.ed.prefs.default_sequence = st.clone();
                s.ed.new_sequence(&n, st);
            }
            !(ok || cancel || esc)
        }
        Dialog::SequenceSettings { name, settings } => {
            let ((ok, cancel), esc) =
                modal(ctx, "seqsettings", t("Sequence Settings"), 460.0, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t("Sequence Name"));
                        ui.text_edit_singleline(name);
                    });
                    ui.add_space(6.0);
                    sequence_form(ui, settings, false);
                    buttons(ui, t("OK"))
                });
            if ok {
                let (n, st) = (name.clone(), settings.clone());
                s.ed.seq_edit("Sequence Settings", |p, sid, _| {
                    let item = p.sequence_item(sid);
                    let q = p.sequence_mut(sid).unwrap();
                    q.name = n.clone();
                    let (vt, at) = (q.video.len(), q.audio.len());
                    q.settings = SequenceSettings {
                        video_tracks: vt,
                        audio_tracks: at,
                        ..st
                    };
                    if let Some(i) = item.and_then(|i| p.item_mut(i)) {
                        i.name = n;
                    }
                    Ok(())
                });
            }
            !(ok || cancel || esc)
        }
        Dialog::ProjectSettings => {
            let mut open = true;
            window(ctx, t("Project Settings"))
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    let mut st = s.ed.project.settings.clone();
                    egui::Grid::new("projset")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(t("Still Image Duration"));
                            let mut secs = st.still_duration.seconds();
                            ui.add(
                                egui::DragValue::new(&mut secs)
                                    .range(0.04..=3600.0)
                                    .speed(0.1)
                                    .suffix(" s"),
                            );
                            st.still_duration = Dur::from_seconds(secs);
                            ui.end_row();
                            ui.label(t("Video Transition Duration"));
                            let mut v = st.video_transition_duration.seconds();
                            ui.add(
                                egui::DragValue::new(&mut v)
                                    .range(0.04..=60.0)
                                    .speed(0.05)
                                    .suffix(" s"),
                            );
                            st.video_transition_duration = Dur::from_seconds(v);
                            ui.end_row();
                            ui.label(t("Audio Transition Duration"));
                            let mut a = st.audio_transition_duration.seconds();
                            ui.add(
                                egui::DragValue::new(&mut a)
                                    .range(0.01..=60.0)
                                    .speed(0.05)
                                    .suffix(" s"),
                            );
                            st.audio_transition_duration = Dur::from_seconds(a);
                            ui.end_row();
                            ui.label(t("Default Media Scaling"));
                            egui::ComboBox::from_id_salt("scaling")
                                .selected_text(match st.media_scaling {
                                    MediaScaling::None => t("None"),
                                    MediaScaling::SetToFrameSize => t("Set to Frame Size"),
                                    MediaScaling::ScaleToFrameSize => t("Scale to Frame Size"),
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut st.media_scaling,
                                        MediaScaling::None,
                                        t("None"),
                                    );
                                    ui.selectable_value(
                                        &mut st.media_scaling,
                                        MediaScaling::SetToFrameSize,
                                        t("Set to Frame Size"),
                                    );
                                    ui.selectable_value(
                                        &mut st.media_scaling,
                                        MediaScaling::ScaleToFrameSize,
                                        t("Scale to Frame Size"),
                                    );
                                });
                            ui.end_row();
                        });
                    if st != s.ed.project.settings {
                        s.ed.prefs.still_seconds = st.still_duration.seconds();
                        s.ed.prefs.video_transition_seconds =
                            st.video_transition_duration.seconds();
                        s.ed.prefs.audio_transition_seconds =
                            st.audio_transition_duration.seconds();
                        s.ed.prefs.media_scaling = st.media_scaling;
                        s.ed.edit_merge("Project Settings", Some("project-settings".into()), |p| {
                            p.settings = st;
                            Ok(())
                        });
                    }
                });
            open
        }
        Dialog::Export(f) => export_dialog(s, ctx, f),
        Dialog::Speed {
            percent,
            duration,
            reverse,
            ripple,
            linked,
            base,
            base_percent,
            rate,
        } => {
            let ((ok, cancel), esc) =
                modal(ctx, "speed", t("Clip Speed / Duration"), 340.0, |ui| {
                    egui::Grid::new("speed")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(t("Speed"));
                            if ui
                                .add(
                                    egui::DragValue::new(percent)
                                        .range(1.0..=100_000.0)
                                        .speed(0.5)
                                        .suffix(" %"),
                                )
                                .changed()
                                && *linked
                            {
                                *duration =
                                    Dur::from_seconds(base.seconds() * *base_percent / *percent)
                                        .round_frames(*rate);
                            }
                            ui.end_row();
                            ui.label(t("Duration"));
                            let fmt = TimecodeFormat::new(*rate, false);
                            if let Some(d) = widgets::timecode(
                                ui,
                                ui.id().with("speed-dur"),
                                &fmt,
                                *duration,
                                13.0,
                                theme::VALUE,
                            ) && d.0 > 0
                            {
                                *duration = d;
                                if *linked {
                                    *percent = (*base_percent * base.seconds() / d.seconds())
                                        .clamp(1.0, 100_000.0);
                                }
                            }
                            ui.end_row();
                        });
                    ui.checkbox(linked, t("Link speed and duration"));
                    ui.checkbox(reverse, t("Reverse Speed"));
                    ui.checkbox(ripple, t("Ripple Edit, Shifting Trailing Clips"));
                    buttons(ui, t("OK"))
                });
            if ok {
                s.ed.set_speed(*percent, *reverse, *ripple);
            }
            !(ok || cancel || esc)
        }
        Dialog::Gain {
            mode,
            db,
            adjust,
            peak_target,
        } => {
            let peaks = s.ed.selected_audio_peaks();
            let ((ok, cancel), esc) = modal(ctx, "gain", t("Audio Gain"), 340.0, |ui| {
                egui::Grid::new("gain-grid")
                    .num_columns(2)
                    .spacing([10.0, 8.0])
                    .show(ui, |ui| {
                        fn row(
                            ui: &mut Ui,
                            mode: &mut u8,
                            m: u8,
                            label: &'static str,
                            v: &mut f64,
                            max: f64,
                        ) {
                            ui.radio_value(mode, m, t(label));
                            ui.add_enabled(
                                *mode == m,
                                egui::DragValue::new(v)
                                    .range(-96.0..=max)
                                    .speed(0.1)
                                    .fixed_decimals(1)
                                    .suffix(" dB"),
                            );
                            ui.end_row();
                        }
                        row(ui, mode, 0, "Set Gain to", db, 96.0);
                        row(ui, mode, 1, "Adjust Gain by", adjust, 96.0);
                        // both normalize choices share one target level
                        row(ui, mode, 2, "Normalize Max Peak to", peak_target, 0.0);
                        row(ui, mode, 3, "Normalize All Peaks to", peak_target, 0.0);
                    });
                ui.add_space(6.0);
                // the loudest point of the selection as it plays now, gain included
                let loudest = peaks
                    .iter()
                    .filter_map(|(_, g, p)| p.map(|p| 20.0 * (p.max(1e-6) as f64).log10() + g))
                    .fold(f64::NEG_INFINITY, f64::max);
                let text = if peaks.iter().any(|(_, _, p)| p.is_none()) {
                    t("Peak Amplitude: analyzing...").to_string()
                } else if loudest.is_finite() && loudest > -120.0 {
                    tf("Peak Amplitude: {} dB", &[&format!("{loudest:.1}")])
                } else {
                    t("Peak Amplitude: silent").to_string()
                };
                ui.label(RichText::new(text).color(theme::TEXT_DIM));
                buttons(ui, t("OK"))
            });
            if peaks.iter().any(|(_, _, p)| p.is_none()) {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            if ok {
                match *mode {
                    1 => s.ed.adjust_gain(*adjust),
                    2 => s.ed.normalize_gain(*peak_target, false),
                    3 => s.ed.normalize_gain(*peak_target, true),
                    _ => s.ed.set_gain(*db),
                }
            }
            !(ok || cancel || esc)
        }
        Dialog::PasteAttributes(a) => attributes_dialog(s, ctx, a, true),
        Dialog::RemoveAttributes(a) => attributes_dialog(s, ctx, a, false),
        Dialog::Nest(name) => {
            let ((ok, cancel), esc) = modal(ctx, "nest", t("Nested Sequence Name"), 320.0, |ui| {
                let r = ui.text_edit_singleline(name);
                r.request_focus();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let (ok, c) = buttons(ui, t("OK"));
                (ok || enter, c)
            });
            if ok {
                let n = name.clone();
                s.ed.nest(&n);
            }
            !(ok || cancel || esc)
        }
        Dialog::RenameClip(id, name) => {
            let ((ok, cancel), esc) = modal(ctx, "renameclip", t("Rename Clip"), 320.0, |ui| {
                let r = ui.text_edit_singleline(name);
                r.request_focus();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let (ok, c) = buttons(ui, t("OK"));
                (ok || enter, c)
            });
            if ok {
                let (id, n) = (*id, name.clone());
                s.ed.seq_edit("Rename", |p, sid, _| {
                    p.sequence_mut(sid)
                        .unwrap()
                        .clip_mut(id)
                        .ok_or(EditError::Nothing)?
                        .name = n;
                    Ok(())
                });
            }
            !(ok || cancel || esc)
        }
        Dialog::ColorMatte { color, name } => {
            let ((ok, cancel), esc) = modal(ctx, "matte", t("Color Matte"), 300.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Name"));
                    ui.text_edit_singleline(name);
                });
                let mut rgba = [color[0], color[1], color[2], 1.0];
                if crate::color::picker(ui, egui::Id::new("matte-picker"), &mut rgba, false) {
                    *color = [rgba[0], rgba[1], rgba[2]];
                }
                buttons(ui, t("OK"))
            });
            if ok {
                let c = Rgba::new(color[0], color[1], color[2], 1.0);
                let n = name.clone();
                s.new_synthetic(Generator::ColorMatte { color: c }, &n);
            }
            !(ok || cancel || esc)
        }
        Dialog::PerformanceSetup { manual, choice } => {
            let rec = s.hardware.recommended();
            let mut decided: Option<Profile> = None;
            let (_, esc) = modal(
                ctx,
                "performance-setup",
                t("Performance Setup"),
                440.0,
                |ui| {
                    ui.label(t(
                        "OpenPremier checked this computer to choose how it should run:",
                    ));
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(theme::PANEL_DARK)
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(hardware_summary(&s.hardware))
                                    .size(12.0)
                                    .color(theme::TEXT_DIM),
                            );
                        });
                    ui.add_space(8.0);
                    if !*manual {
                        ui.label(
                            RichText::new(tf("Recommended profile: {}", &[&t(rec.label())]))
                                .strong()
                                .family(crate::fonts::strong())
                                .color(theme::TEXT_BRIGHT),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(t(rec.description()))
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                RichText::new(t("Use Recommended Settings"))
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(theme::ACCENT_DIM),
                                        )
                                        .clicked()
                                    {
                                        decided = Some(rec);
                                    }
                                    if ui.button(t("Choose Manually")).clicked() {
                                        *manual = true;
                                        *choice = rec;
                                    }
                                },
                            );
                        });
                    } else {
                        ui.label(t("Performance Profile"));
                        let mut level = choice.index() as f32;
                        let slider = egui::Slider::new(&mut level, 0.0..=4.0)
                            .step_by(1.0)
                            .show_value(false);
                        if ui.add_sized([ui.available_width(), 18.0], slider).changed() {
                            *choice = Profile::from_index(level.round() as u8);
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(t(Profile::UltraPerformance.label()))
                                    .size(10.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(t(Profile::Maximum.label()))
                                            .size(10.5)
                                            .color(theme::TEXT_DIM),
                                    );
                                },
                            );
                        });
                        ui.add_space(4.0);
                        let label = if *choice == rec {
                            format!("{} ({})", t(choice.label()), t("recommended"))
                        } else {
                            t(choice.label()).to_string()
                        };
                        ui.label(
                            RichText::new(label)
                                .strong()
                                .family(crate::fonts::strong())
                                .color(theme::TEXT_BRIGHT),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(t(choice.description()))
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                        if buttons(ui, t("Apply")).0 {
                            decided = Some(*choice);
                        }
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(t(
                            "You can change this at any time in Edit > Preferences > Performance.",
                        ))
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );
                },
            );
            // closing the window keeps the recommendation
            let chosen = decided.or(esc.then_some(rec));
            if let Some(p) = chosen {
                let hw = s.hardware.clone();
                s.ed.prefs.performance_setup_done = true;
                s.ed.set_performance_profile(p, Some(&hw));
                log::info!(
                    "performance setup: {} (recommended {})",
                    p.label(),
                    rec.label()
                );
            }
            chosen.is_none()
        }
        Dialog::LinkMedia(form) => link_media_dialog(s, ctx, form),
        Dialog::Captions(form) => {
            let models = op_application::captions::models_dir(&s.ed);
            let ((ok, cancel), esc) = modal(
                ctx,
                "captions",
                t("Transcribe and Create Captions"),
                460.0,
                |ui| {
                    ui.label(
                        RichText::new(t(
                            "The speech in the sequence becomes animated captions. Everything runs on this computer; nothing is uploaded.",
                        ))
                        .color(theme::TEXT_DIM),
                    );
                    ui.add_space(8.0);
                    egui::Grid::new("captions-form")
                        .num_columns(2)
                        .spacing([14.0, 8.0])
                        .show(ui, |ui| {
                            ui.label(t("Spoken Language"));
                            let current = CAPTION_LANGUAGES
                                .get(form.language)
                                .map(|(_, n)| if form.language == 0 { t(n) } else { *n })
                                .unwrap_or("");
                            egui::ComboBox::from_id_salt("cap-lang")
                                .selected_text(current)
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for (i, (_, name)) in CAPTION_LANGUAGES.iter().enumerate() {
                                        let label = if i == 0 { t(name) } else { name };
                                        ui.selectable_value(&mut form.language, i, label);
                                    }
                                });
                            ui.end_row();

                            ui.label(t("Caption Language"));
                            ui.vertical(|ui| {
                                ui.radio_value(
                                    &mut form.translate,
                                    false,
                                    t("Same as spoken (transcribe)"),
                                );
                                ui.radio_value(
                                    &mut form.translate,
                                    true,
                                    t("Translate to English"),
                                )
                                .on_hover_text(t(
                                    "The speech model only translates into English. Choosing a spoken language other than the real one does not translate; it lowers the quality.",
                                ));
                            });
                            ui.end_row();

                            ui.label(t("Speech Model"));
                            let model_label = |m: ModelSize| {
                                let note = match m {
                                    ModelSize::Tiny => t("fastest"),
                                    ModelSize::Base => t("recommended"),
                                    ModelSize::Small => t("most accurate"),
                                };
                                format!("{} ({} MB, {note})", m.label(), m.megabytes())
                            };
                            egui::ComboBox::from_id_salt("cap-model")
                                .selected_text(model_label(form.model))
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for m in ModelSize::ALL {
                                        ui.selectable_value(&mut form.model, m, model_label(m));
                                    }
                                });
                            ui.end_row();
                            ui.label("");
                            let status = if form.model.is_downloaded(&models) {
                                t("Downloaded").to_string()
                            } else {
                                tf(
                                    "Downloaded once ({} MB) from Hugging Face when you continue",
                                    &[&form.model.megabytes()],
                                )
                            };
                            ui.label(RichText::new(status).size(11.5).color(theme::TEXT_DIM));
                            ui.end_row();

                            ui.label(t("Caption Style"));
                            egui::ComboBox::from_id_salt("cap-style")
                                .selected_text(t(catalog::CAPTION_STYLES
                                    [form.options.style as usize % catalog::CAPTION_STYLES.len()]))
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    for (i, name) in catalog::CAPTION_STYLES.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut form.options.style,
                                            i as u32,
                                            t(name),
                                        )
                                        .on_hover_text(t(caption_style_hint(i)));
                                    }
                                });
                            ui.end_row();
                            ui.label("");
                            ui.label(
                                RichText::new(t(caption_style_hint(form.options.style as usize)))
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            );
                            ui.end_row();

                            ui.label(t("Words per Caption"));
                            ui.add(egui::Slider::new(&mut form.options.max_words, 1..=8));
                            ui.end_row();

                            ui.label(t("All Caps"));
                            ui.checkbox(&mut form.options.uppercase, "");
                            ui.end_row();
                        });
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(t(
                            "Style, colors and font can be changed later for every caption at once in the Captions panel (Window > Captions).",
                        ))
                        .size(11.5)
                        .color(theme::TEXT_DIM),
                    );
                    buttons(ui, t("Create Captions"))
                },
            );
            if ok {
                s.caption_language = form.language;
                s.caption_translate = form.translate;
                s.caption_model = form.model;
                s.caption_options = form.options.clone();
                let language = CAPTION_LANGUAGES
                    .get(form.language)
                    .map(|(code, _)| code.to_string())
                    .filter(|c| !c.is_empty());
                s.ed.transcribe_captions(op_application::captions::TranscribeOptions {
                    model: form.model,
                    language,
                    task: if form.translate {
                        op_application::captions::Task::Translate
                    } else {
                        op_application::captions::Task::Transcribe
                    },
                    captions: form.options.clone(),
                });
            }
            !(ok || cancel || esc)
        }
        Dialog::AddTracks { video, audio } => {
            let ((ok, cancel), esc) = modal(ctx, "addtracks", t("Add Tracks"), 300.0, |ui| {
                egui::Grid::new("addtracks").num_columns(2).show(ui, |ui| {
                    ui.label(t("Video Tracks"));
                    ui.add(egui::DragValue::new(video).range(0..=99));
                    ui.end_row();
                    ui.label(t("Audio Tracks"));
                    ui.add(egui::DragValue::new(audio).range(0..=99));
                    ui.end_row();
                });
                buttons(ui, t("OK"))
            });
            if ok {
                let (v, a) = (*video, *audio);
                s.ed.seq_edit("Add Tracks", |p, sid, _| {
                    let (nv, na) = p
                        .sequence(sid)
                        .map(|q| (q.video.len(), q.audio.len()))
                        .unwrap_or((0, 0));
                    if v > 0 {
                        op_timeline::add_tracks(
                            p,
                            sid,
                            TrackKind::Video,
                            v,
                            nv,
                            AudioTrackLayout::Standard,
                        )?;
                    }
                    if a > 0 {
                        op_timeline::add_tracks(
                            p,
                            sid,
                            TrackKind::Audio,
                            a,
                            na,
                            AudioTrackLayout::Standard,
                        )?;
                    }
                    Ok(())
                });
            }
            !(ok || cancel || esc)
        }
        Dialog::Interpret {
            asset,
            fps,
            alpha_ignore,
        } => {
            let ((ok, cancel), esc) =
                modal(ctx, "interpret", t("Interpret Footage"), 320.0, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(t("Assume this frame rate"));
                        ui.add(
                            egui::DragValue::new(fps)
                                .range(1.0..=240.0)
                                .speed(0.01)
                                .max_decimals(3)
                                .suffix(" fps"),
                        );
                    });
                    ui.checkbox(alpha_ignore, t("Ignore Alpha Channel"));
                    buttons(ui, t("OK"))
                });
            if ok {
                let (id, rate, ignore) = (*asset, Rate::from_f64(*fps), *alpha_ignore);
                s.ed.media.forget(id);
                s.ed.edit("Interpret Footage", |p| {
                    let mut a = p.asset(id).ok_or(EditError::Nothing)?.clone();
                    a.interpretation.frame_rate = Some(rate);
                    a.interpretation.ignore_alpha = ignore;
                    p.assets.insert(id, std::sync::Arc::new(a));
                    Ok(())
                });
            }
            !(ok || cancel || esc)
        }
        Dialog::Shortcuts(f) => shortcuts_dialog(s, ctx, f),
        Dialog::Preferences { scale } => preferences(s, ctx, scale),
        Dialog::Log { level, filter } => log_window(ctx, level, filter),
        Dialog::PasteMedia {
            id,
            folder,
            name,
            always,
        } => paste_dialog(s, ctx, *id, folder, name, always),
        Dialog::About => {
            let mut open = true;
            window(ctx, t("About OpenPremier")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.label(RichText::new(format!("OpenPremier {}", env!("CARGO_PKG_VERSION"))).size(18.0).color(theme::TEXT_BRIGHT));
                ui.label(t("A free and open source video editor for Windows and Linux."));
                ui.add_space(8.0);
                ui.label(RichText::new(t("License: GNU General Public License, version 3 or later.")).size(11.5));
                ui.label(RichText::new(format!("FFmpeg {}", op_media::ffmpeg_version())).size(11.5).color(theme::TEXT_DIM));
                ui.label(RichText::new(s.gpu.description()).size(11.5).color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.label(RichText::new(t("Adobe and Premiere Pro are trademarks of Adobe Inc. OpenPremier is an independent project and is not affiliated with Adobe.")).size(10.5).color(theme::TEXT_DIM));
            });
            open
        }
    }
}

/// The notice shown while a project is read in the background.
pub fn opening(s: &mut State, ctx: &egui::Context) {
    let Some(o) = &s.opening else { return };
    let name = o
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let secs = o.started.elapsed().as_secs_f32();
    egui::Modal::new(egui::Id::new("opening-project")).show(ctx, |ui| {
        ui.set_width(380.0);
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new(t("Opening project..."))
                    .size(15.0)
                    .color(theme::TEXT_BRIGHT),
            );
        });
        ui.add_space(4.0);
        ui.label(RichText::new(name).color(theme::TEXT_DIM));
        if secs > 2.0 {
            ui.label(
                RichText::new(tf("{} seconds", &[&format!("{secs:.0}")]))
                    .color(theme::TEXT_DIM)
                    .size(11.0),
            );
        }
    });
    ctx.request_repaint_after(std::time::Duration::from_millis(100));
}
