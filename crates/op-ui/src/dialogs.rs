//! Dialogs: unsaved changes, recovery, new sequence and settings, export, speed/duration, gain,
//! paste attributes, nest, color matte, tracks, interpret footage, keyboard shortcuts,
//! preferences, project settings, import report and about.

use std::path::PathBuf;

use egui::{RichText, Ui};
use op_application::keymap::{self, Binding, Chord, Keymap};
use op_application::{ExportJob, ExportSettings};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, VideoCodec, VideoSettings};
use op_project::prproj::ImportReport;
use op_timeline::Attributes;

use crate::app::State;
use crate::i18n::{self, t, tf, tn};
use crate::{keys, theme, widgets};

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
        db: f64,
    },
    PasteAttributes(Attributes),
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
    About,
}

struct Preset {
    name: &'static str,
    w: u32,
    h: u32,
    rate: Rate,
}

const SEQ_PRESETS: &[Preset] = &[
    Preset {
        name: "HD 1080p 23.976",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_23_976,
    },
    Preset {
        name: "HD 1080p 24",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_24,
    },
    Preset {
        name: "HD 1080p 25",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_25,
    },
    Preset {
        name: "HD 1080p 29.97",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_29_97,
    },
    Preset {
        name: "HD 1080p 30",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_30,
    },
    Preset {
        name: "HD 1080p 50",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_50,
    },
    Preset {
        name: "HD 1080p 59.94",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_59_94,
    },
    Preset {
        name: "HD 1080p 60",
        w: 1920,
        h: 1080,
        rate: Rate::FPS_60,
    },
    Preset {
        name: "HD 720p 25",
        w: 1280,
        h: 720,
        rate: Rate::FPS_25,
    },
    Preset {
        name: "HD 720p 29.97",
        w: 1280,
        h: 720,
        rate: Rate::FPS_29_97,
    },
    Preset {
        name: "HD 720p 50",
        w: 1280,
        h: 720,
        rate: Rate::FPS_50,
    },
    Preset {
        name: "HD 720p 59.94",
        w: 1280,
        h: 720,
        rate: Rate::FPS_59_94,
    },
    Preset {
        name: "UHD 2160p 23.976",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_23_976,
    },
    Preset {
        name: "UHD 2160p 24",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_24,
    },
    Preset {
        name: "UHD 2160p 25",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_25,
    },
    Preset {
        name: "UHD 2160p 29.97",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_29_97,
    },
    Preset {
        name: "UHD 2160p 30",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_30,
    },
    Preset {
        name: "UHD 2160p 50",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_50,
    },
    Preset {
        name: "UHD 2160p 59.94",
        w: 3840,
        h: 2160,
        rate: Rate::FPS_59_94,
    },
    Preset {
        name: "DCI 4K 24",
        w: 4096,
        h: 2160,
        rate: Rate::FPS_24,
    },
    Preset {
        name: "Vertical 1080x1920 30",
        w: 1080,
        h: 1920,
        rate: Rate::FPS_30,
    },
    Preset {
        name: "Vertical 1080x1920 60",
        w: 1080,
        h: 1920,
        rate: Rate::FPS_60,
    },
    Preset {
        name: "Square 1080x1080 30",
        w: 1080,
        h: 1080,
        rate: Rate::FPS_30,
    },
];

struct ExportPreset {
    name: &'static str,
    codec: VideoCodec,
    /// Target bitrate in Mbps; 0 uses constant quality.
    mbps: u32,
    height: Option<u32>,
    audio: AudioCodec,
    video: bool,
}

const EXPORT_PRESETS: &[ExportPreset] = &[
    ExportPreset {
        name: "Match Source - High Quality (H.264)",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        audio: AudioCodec::Aac,
        video: true,
    },
    ExportPreset {
        name: "YouTube 1080p (H.264)",
        codec: VideoCodec::H264,
        mbps: 16,
        height: Some(1080),
        audio: AudioCodec::Aac,
        video: true,
    },
    ExportPreset {
        name: "YouTube 2160p 4K (H.264)",
        codec: VideoCodec::H264,
        mbps: 45,
        height: Some(2160),
        audio: AudioCodec::Aac,
        video: true,
    },
    ExportPreset {
        name: "Social Media 1080p (H.264)",
        codec: VideoCodec::H264,
        mbps: 10,
        height: Some(1080),
        audio: AudioCodec::Aac,
        video: true,
    },
    ExportPreset {
        name: "Match Source - High Quality (HEVC)",
        codec: VideoCodec::Hevc,
        mbps: 0,
        height: None,
        audio: AudioCodec::Aac,
        video: true,
    },
    ExportPreset {
        name: "Apple ProRes 422 HQ",
        codec: VideoCodec::ProRes422Hq,
        mbps: 0,
        height: None,
        audio: AudioCodec::Pcm24,
        video: true,
    },
    ExportPreset {
        name: "Apple ProRes 4444 (with alpha)",
        codec: VideoCodec::ProRes4444,
        mbps: 0,
        height: None,
        audio: AudioCodec::Pcm24,
        video: true,
    },
    ExportPreset {
        name: "DNxHR HQ",
        codec: VideoCodec::DnxhrHq,
        mbps: 0,
        height: None,
        audio: AudioCodec::Pcm24,
        video: true,
    },
    ExportPreset {
        name: "PNG (QuickTime, with alpha)",
        codec: VideoCodec::Png,
        mbps: 0,
        height: None,
        audio: AudioCodec::Pcm16,
        video: true,
    },
    ExportPreset {
        name: "Audio only - WAV",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        audio: AudioCodec::Pcm24,
        video: false,
    },
    ExportPreset {
        name: "Audio only - MP3",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        audio: AudioCodec::Mp3,
        video: false,
    },
];

fn audio_extension(c: AudioCodec) -> &'static str {
    match c {
        AudioCodec::Mp3 => "mp3",
        AudioCodec::Aac => "m4a",
        _ => "wav",
    }
}

impl Dialog {
    pub fn new_sequence(settings: SequenceSettings) -> Dialog {
        Dialog::NewSequence {
            name: t("Sequence 01").to_string(),
            settings,
        }
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
        Some(Dialog::Gain { db: c.gain_db })
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

fn dirs_video() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .map(|h| {
            let v = h.join("Videos");
            if v.is_dir() { v } else { h }
        })
}

fn apply_preset(f: &mut ExportForm, i: usize) {
    let Some(p) = EXPORT_PRESETS.get(i) else {
        return;
    };
    f.preset = i;
    f.video = p.video;
    f.codec = p.codec;
    f.audio_codec = p.audio;
    f.use_bitrate = p.mbps > 0;
    if p.mbps > 0 {
        f.bitrate = p.mbps;
    }
    let (sw, sh) = f.seq_size;
    match p.height {
        Some(h) if sh > 0 => {
            f.height = h;
            f.width = ((sw as f64 * h as f64 / sh as f64).round() as u32) & !1;
        }
        _ => {
            f.width = sw;
            f.height = sh;
        }
    }
    let ext = if p.video {
        p.codec.extension()
    } else {
        audio_extension(p.audio)
    };
    f.path.set_extension(ext);
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
    let resp = egui::Modal::new(egui::Id::new(id)).show(ctx, |ui| {
        ui.set_width(width);
        ui.label(RichText::new(title).size(15.0).color(theme::TEXT_BRIGHT));
        ui.add_space(8.0);
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
        Dialog::Gain { db } => {
            let ((ok, cancel), esc) = modal(ctx, "gain", t("Audio Gain"), 300.0, |ui| {
                ui.horizontal(|ui| {
                    ui.label(t("Set Gain to"));
                    ui.add(
                        egui::DragValue::new(db)
                            .range(-96.0..=96.0)
                            .speed(0.1)
                            .suffix(" dB"),
                    );
                });
                buttons(ui, t("OK"))
            });
            if ok {
                s.ed.set_gain(*db);
            }
            !(ok || cancel || esc)
        }
        Dialog::PasteAttributes(a) => {
            let ((ok, cancel), esc) = modal(ctx, "pasteattr", t("Paste Attributes"), 300.0, |ui| {
                ui.label(RichText::new(t("Video Attributes")).color(theme::TEXT_DIM));
                ui.checkbox(&mut a.motion, tn("Motion"));
                ui.checkbox(&mut a.opacity, tn("Opacity"));
                ui.checkbox(&mut a.effects, t("Effects"));
                ui.label(RichText::new(t("Audio Attributes")).color(theme::TEXT_DIM));
                ui.checkbox(&mut a.volume, tn("Volume"));
                ui.checkbox(&mut a.channel_volume, tn("Channel Volume"));
                ui.checkbox(&mut a.panner, tn("Panner"));
                buttons(ui, t("OK"))
            });
            if ok {
                s.ed.paste_attributes(*a);
            }
            !(ok || cancel || esc)
        }
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

fn sequence_form(ui: &mut Ui, st: &mut SequenceSettings, new: bool) {
    let current = SEQ_PRESETS
        .iter()
        .position(|p| p.w == st.width && p.h == st.height && p.rate == st.rate);
    egui::ComboBox::from_id_salt("seq-preset")
        .width(300.0)
        .selected_text(current.map(|i| SEQ_PRESETS[i].name).unwrap_or(t("Custom")))
        .show_ui(ui, |ui| {
            for p in SEQ_PRESETS {
                if ui
                    .selectable_label(Some(p.name) == current.map(|i| SEQ_PRESETS[i].name), p.name)
                    .clicked()
                {
                    st.width = p.w;
                    st.height = p.h;
                    st.rate = p.rate;
                    st.drop_frame = false;
                }
            }
        });
    ui.add_space(6.0);
    egui::Grid::new("seq-form")
        .num_columns(2)
        .spacing([12.0, 8.0])
        .show(ui, |ui| {
            ui.label(t("Frame Size"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut st.width).range(16..=16384));
                ui.label("x");
                ui.add(egui::DragValue::new(&mut st.height).range(16..=16384));
            });
            ui.end_row();
            ui.label(t("Frame Rate"));
            egui::ComboBox::from_id_salt("seq-rate")
                .selected_text(format!("{} fps", st.rate.label()))
                .show_ui(ui, |ui| {
                    for r in Rate::SEQUENCE_RATES {
                        ui.selectable_value(&mut st.rate, r, format!("{} fps", r.label()));
                    }
                });
            ui.end_row();
            if st.rate.supports_drop_frame() {
                ui.label(t("Timecode"));
                ui.checkbox(&mut st.drop_frame, t("Drop-Frame"));
                ui.end_row();
            }
            ui.label(t("Pixel Aspect Ratio"));
            egui::ComboBox::from_id_salt("seq-par")
                .selected_text(format!(
                    "{:.4}",
                    st.pixel_aspect.0 as f64 / st.pixel_aspect.1.max(1) as f64
                ))
                .show_ui(ui, |ui| {
                    for (par, name) in [
                        ((1, 1), "Square Pixels (1.0)"),
                        ((4, 3), "HD Anamorphic 1080 (1.333)"),
                        ((2, 1), "Anamorphic 2:1 (2.0)"),
                    ] {
                        ui.selectable_value(&mut st.pixel_aspect, par, t(name));
                    }
                });
            ui.end_row();
            ui.label(t("Audio Sample Rate"));
            egui::ComboBox::from_id_salt("seq-audio")
                .selected_text(format!("{} Hz", st.audio_rate))
                .show_ui(ui, |ui| {
                    for r in [32000u32, 44100, 48000, 88200, 96000] {
                        ui.selectable_value(&mut st.audio_rate, r, format!("{r} Hz"));
                    }
                });
            ui.end_row();
            ui.label(t("Master"));
            egui::ComboBox::from_id_salt("seq-master")
                .selected_text(match st.master {
                    MasterLayout::Mono => t("Mono"),
                    MasterLayout::Stereo => t("Stereo"),
                    MasterLayout::Surround51 => "5.1",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut st.master, MasterLayout::Stereo, t("Stereo"));
                    ui.selectable_value(&mut st.master, MasterLayout::Mono, t("Mono"));
                    ui.selectable_value(&mut st.master, MasterLayout::Surround51, "5.1");
                });
            ui.end_row();
            if new {
                ui.label(t("Video Tracks"));
                ui.add(egui::DragValue::new(&mut st.video_tracks).range(1..=99));
                ui.end_row();
                ui.label(t("Audio Tracks"));
                ui.add(egui::DragValue::new(&mut st.audio_tracks).range(1..=99));
                ui.end_row();
            }
            ui.label(t("Compositing"));
            ui.checkbox(&mut st.linear_compositing, t("Composite in Linear Color"));
            ui.end_row();
        });
}

fn export_dialog(s: &mut State, ctx: &egui::Context, f: &mut ExportForm) -> bool {
    let mut open = true;
    let mut start = false;
    window(ctx, t("Export Settings"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .show(ctx, |ui| {
            egui::Grid::new("export")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("Preset"));
                    egui::ComboBox::from_id_salt("exp-preset")
                        .width(320.0)
                        .selected_text(t(EXPORT_PRESETS[f.preset].name))
                        .show_ui(ui, |ui| {
                            for (i, p) in EXPORT_PRESETS.iter().enumerate() {
                                if ui.selectable_label(f.preset == i, t(p.name)).clicked() {
                                    apply_preset(f, i);
                                }
                            }
                        });
                    ui.end_row();
                    ui.label(t("Output"));
                    ui.horizontal(|ui| {
                        let mut text = f.path.display().to_string();
                        if ui
                            .add(egui::TextEdit::singleline(&mut text).desired_width(300.0))
                            .changed()
                        {
                            f.path = PathBuf::from(text);
                        }
                        if ui.button("\u{2026}").clicked() {
                            let ext = if f.video {
                                f.codec.extension()
                            } else {
                                audio_extension(f.audio_codec)
                            };
                            let mut d = rfd::FileDialog::new()
                                .add_filter(ext.to_uppercase(), &[ext])
                                .set_file_name(
                                    f.path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default(),
                                );
                            if let Some(dir) = f.path.parent() {
                                d = d.set_directory(dir);
                            }
                            if let Some(p) = d.save_file() {
                                f.path = p;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(t("Range"));
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut f.in_out, false, t("Entire Sequence"));
                        ui.add_enabled_ui(f.has_in_out, |ui| {
                            ui.radio_value(&mut f.in_out, true, t("Sequence In/Out"))
                        });
                    });
                    ui.end_row();
                });
            ui.separator();
            ui.checkbox(
                &mut f.video,
                RichText::new(t("Export Video")).color(theme::TEXT_BRIGHT),
            );
            ui.add_enabled_ui(f.video, |ui| {
                egui::Grid::new("exp-video")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(t("Format"));
                        egui::ComboBox::from_id_salt("exp-codec")
                            .selected_text(f.codec.label())
                            .show_ui(ui, |ui| {
                                for c in VideoCodec::ALL {
                                    if ui.selectable_value(&mut f.codec, c, c.label()).changed() {
                                        f.path.set_extension(c.extension());
                                    }
                                }
                            });
                        ui.end_row();
                        ui.label(t("Frame Size"));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut f.width).range(16..=16384));
                            ui.label("x");
                            ui.add(egui::DragValue::new(&mut f.height).range(16..=16384));
                        });
                        ui.end_row();
                        ui.label(t("Frame Rate"));
                        egui::ComboBox::from_id_salt("exp-rate")
                            .selected_text(format!("{} fps", f.rate.label()))
                            .show_ui(ui, |ui| {
                                for r in Rate::SEQUENCE_RATES {
                                    let text = if r == f.seq_rate {
                                        tf("{} fps (sequence)", &[&r.label()])
                                    } else {
                                        format!("{} fps", r.label())
                                    };
                                    ui.selectable_value(&mut f.rate, r, text);
                                }
                            })
                            .response
                            .on_hover_text(t(
                                "Frames per second of the exported file. The sequence rate keeps every frame as edited.",
                            ));
                        ui.end_row();
                        if matches!(f.codec, VideoCodec::H264 | VideoCodec::Hevc) {
                            ui.label(t("Bitrate"));
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut f.use_bitrate, false, t("Constant Quality"));
                                ui.radio_value(&mut f.use_bitrate, true, t("Target Bitrate"));
                            });
                            ui.end_row();
                            if f.use_bitrate {
                                ui.label("");
                                ui.add(
                                    egui::DragValue::new(&mut f.bitrate)
                                        .range(1..=800)
                                        .suffix(" Mbps"),
                                );
                            } else {
                                ui.label("");
                                ui.add(
                                    egui::Slider::new(&mut f.quality, 0..=40)
                                        .text(t("CRF (lower is better)")),
                                );
                            }
                            ui.end_row();
                            ui.label(t("Hardware Encoding"));
                            ui.checkbox(&mut f.hardware, t("Use the GPU encoder when available"));
                            ui.end_row();
                        }
                    });
            });
            ui.separator();
            ui.checkbox(
                &mut f.audio,
                RichText::new(t("Export Audio")).color(theme::TEXT_BRIGHT),
            );
            ui.add_enabled_ui(f.audio, |ui| {
                egui::Grid::new("exp-audio")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.label(t("Format"));
                        egui::ComboBox::from_id_salt("exp-acodec")
                            .selected_text(f.audio_codec.label())
                            .show_ui(ui, |ui| {
                                for c in [
                                    AudioCodec::Aac,
                                    AudioCodec::Pcm16,
                                    AudioCodec::Pcm24,
                                    AudioCodec::Mp3,
                                ] {
                                    ui.selectable_value(&mut f.audio_codec, c, c.label());
                                }
                            });
                        ui.end_row();
                        ui.label(t("Sample Rate"));
                        egui::ComboBox::from_id_salt("exp-arate")
                            .selected_text(format!("{} Hz", f.audio_rate))
                            .show_ui(ui, |ui| {
                                for r in [44100u32, 48000, 96000] {
                                    ui.selectable_value(&mut f.audio_rate, r, format!("{r} Hz"));
                                }
                            });
                        ui.end_row();
                        ui.label(t("Channels"));
                        egui::ComboBox::from_id_salt("exp-ach")
                            .selected_text(if f.channels == 1 {
                                t("Mono")
                            } else {
                                t("Stereo")
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut f.channels, 2, t("Stereo"));
                                ui.selectable_value(&mut f.channels, 1, t("Mono"));
                            });
                        ui.end_row();
                        if matches!(f.audio_codec, AudioCodec::Aac | AudioCodec::Mp3) {
                            ui.label(t("Bitrate"));
                            egui::ComboBox::from_id_salt("exp-abr")
                                .selected_text(format!("{} kbps", f.audio_bitrate))
                                .show_ui(ui, |ui| {
                                    for b in [128u32, 160, 192, 256, 320] {
                                        ui.selectable_value(
                                            &mut f.audio_bitrate,
                                            b,
                                            format!("{b} kbps"),
                                        );
                                    }
                                });
                            ui.end_row();
                        }
                    });
            });
            ui.separator();
            let seq = s.ed.project.sequence(f.sequence);
            let range = seq.map(|q| {
                if f.in_out {
                    q.in_out()
                } else {
                    SeqRange::new(SeqTime::ZERO, SeqTime::ZERO + q.duration())
                }
            });
            if let (Some(q), Some(r)) = (seq, range) {
                let fmt = TimecodeFormat::new(q.rate(), q.settings.drop_frame);
                ui.label(
                    RichText::new(tf("Duration: {}", &[&fmt.format(r.duration())]))
                        .color(theme::TEXT_DIM),
                );
            }
            let valid = (f.video || f.audio)
                && range.is_some_and(|r| !r.is_empty())
                && !f.path.as_os_str().is_empty();
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(
                            valid,
                            egui::Button::new(
                                RichText::new(t("Export")).color(egui::Color32::WHITE),
                            )
                            .fill(theme::ACCENT_DIM),
                        )
                        .clicked()
                    {
                        start = true;
                    }
                });
            });
        });
    if start {
        let Some(seq) = s.ed.project.sequence(f.sequence) else {
            return false;
        };
        let range = if f.in_out {
            seq.in_out()
        } else {
            SeqRange::new(SeqTime::ZERO, SeqTime::ZERO + seq.duration())
        };
        let video = f.video.then(|| VideoSettings {
            codec: f.codec,
            width: f.width & !1,
            height: f.height & !1,
            rate: f.rate,
            bitrate_kbps: (f.use_bitrate && matches!(f.codec, VideoCodec::H264 | VideoCodec::Hevc))
                .then_some(f.bitrate * 1000),
            quality: f.quality,
            hardware: f.hardware,
        });
        let audio = f.audio.then_some(AudioSettings {
            codec: f.audio_codec,
            rate: f.audio_rate,
            channels: f.channels,
            bitrate_kbps: f.audio_bitrate,
        });
        let mut path = f.path.clone();
        if path.extension().is_none() {
            path.set_extension(if f.video {
                f.codec.extension()
            } else {
                audio_extension(f.audio_codec)
            });
        }
        if let Some(dir) = path.parent() {
            s.ed.prefs.last_export_dir = Some(dir.to_path_buf());
        }
        s.ed.prefs.hardware_encoding = f.hardware;
        let settings = ExportSettings {
            path: path.clone(),
            sequence: f.sequence,
            range,
            video,
            audio,
        };
        let snap = s.ed.snapshot();
        let job = ExportJob::start(snap, settings, s.gpu.clone(), s.ed.media.clone());
        s.ed.exports.push(job);
        s.ed.info(tf("Exporting {}", &[&path.display()]));
        return false;
    }
    open
}

// ------------------------------------------------------------------------ keyboard shortcuts

/// Readable names of the commands this application implements.
pub fn command_name(cmd: &str) -> String {
    const NAMES: &[(&str, &str)] = &[
        ("cmd.edit.undo", "Undo"),
        ("cmd.edit.redo", "Redo"),
        ("cmd.edit.cut", "Cut"),
        ("cmd.edit.copy", "Copy"),
        ("cmd.edit.paste", "Paste"),
        ("cmd.edit.pasteinsert", "Paste Insert"),
        ("cmd.edit.pasteattributes", "Paste Attributes..."),
        ("cmd.edit.clear", "Clear"),
        ("cmd.edit.rippledelete", "Ripple Delete"),
        ("cmd.edit.selectall", "Select All"),
        ("cmd.edit.deselectall", "Deselect All"),
        ("cmd.edit.duplicate", "Duplicate"),
        ("cmd.edit.keyboardshortcuts", "Keyboard Shortcuts..."),
        ("cmd.edit.find", "Find"),
        ("cmd.file.new.project", "New Project"),
        ("cmd.file.new.sequence", "New Sequence"),
        ("cmd.file.new.bin", "New Bin"),
        ("cmd.file.openproject", "Open Project..."),
        ("cmd.file.close", "Close Project"),
        ("cmd.file.closepanel", "Close Sequence"),
        ("cmd.file.save", "Save"),
        ("cmd.file.saveas", "Save As..."),
        ("cmd.file.savecopy", "Save a Copy..."),
        ("cmd.file.import", "Import..."),
        ("cmd.file.export.movie", "Export Media..."),
        ("cmd.export.frame", "Export Frame..."),
        ("cmd.file.exit", "Exit"),
        ("cmd.clip.enable", "Enable"),
        ("cmd.clip.linkaudioandvideo", "Link / Unlink"),
        ("cmd.clip.group", "Group"),
        ("cmd.clip.ungroup", "Ungroup"),
        ("cmd.clip.insert", "Insert"),
        ("cmd.clip.overlay", "Overwrite"),
        ("cmd.clip.makesubclip", "Make Subclip"),
        ("cmd.clip.speed", "Speed/Duration..."),
        ("cmd.clip.audiooptions.gain", "Audio Gain..."),
        ("cmd.common.setin", "Mark In"),
        ("cmd.common.setout", "Mark Out"),
        ("cmd.clear.in", "Clear In"),
        ("cmd.clear.out", "Clear Out"),
        ("cmd.clear.inandout", "Clear In and Out"),
        ("cmd.goto.in", "Go to In"),
        ("cmd.goto.out", "Go to Out"),
        (
            "cmd.marker.setsequenceinoutmarkeraroundtargetclip",
            "Mark Clip",
        ),
        (
            "cmd.marker.setsequenceinoutmarkeraroundselection.out",
            "Mark Selection",
        ),
        ("cmd.sequence.lift", "Lift"),
        ("cmd.sequence.extract", "Extract"),
        ("cmd.sequence.razorateditline", "Add Edit"),
        ("cmd.sequence.razorateditline.all", "Add Edit to All Tracks"),
        (
            "cmd.sequence.applydefaultvideotransition",
            "Apply Video Transition",
        ),
        (
            "cmd.sequence.applydefaultaudiotransition",
            "Apply Audio Transition",
        ),
        (
            "cmd.sequence.applydefaulttransitions",
            "Apply Default Transitions to Selection",
        ),
        ("cmd.sequence.snap", "Snap in Timeline"),
        (
            "cmd.sequence.extendselectededittoplayhead",
            "Extend Selected Edit to Playhead",
        ),
        (
            "cmd.sequence.rippletrimpreviousedittoplayhead",
            "Ripple Trim Previous Edit to Playhead",
        ),
        (
            "cmd.sequence.rippletrimnextedittoplayhead",
            "Ripple Trim Next Edit to Playhead",
        ),
        ("cmd.sequence.matchframe", "Match Frame"),
        ("cmd.sequence.reversematchframe", "Reverse Match Frame"),
        ("cmd.sequence.increaseclipvolume", "Increase Clip Volume"),
        ("cmd.sequence.decreaseclipvolume", "Decrease Clip Volume"),
        (
            "cmd.sequence.increaseclipvolumemany",
            "Increase Clip Volume Many",
        ),
        (
            "cmd.sequence.decreaseclipvolumemany",
            "Decrease Clip Volume Many",
        ),
        (
            "cmd.sequence.findnextsequencegap",
            "Go to Next Gap in Sequence",
        ),
        (
            "cmd.sequence.findprevioussequencegap",
            "Go to Previous Gap in Sequence",
        ),
        ("cmd.sequence.makesubsequence", "Make Subsequence"),
        ("cmd.set.marker", "Add Marker"),
        ("cmd.marker.gotomarker.next", "Go to Next Marker"),
        ("cmd.marker.gotomarker.previous", "Go to Previous Marker"),
        ("cmd.marker.clearmarker.current", "Clear Selected Marker"),
        ("cmd.marker.clearmarker.all", "Clear All Markers"),
        ("cmd.transport.toggleplay", "Play/Stop"),
        ("cmd.transport.shuttle.left", "Shuttle Left"),
        ("cmd.transport.shuttle.right", "Shuttle Right"),
        ("cmd.transport.shuttle.stop", "Shuttle Stop"),
        ("cmd.transport.shuttle.slow.left", "Shuttle Slow Left"),
        ("cmd.transport.shuttle.slow.right", "Shuttle Slow Right"),
        ("cmd.transport.step.back", "Step Back 1 Frame"),
        ("cmd.transport.step.forward", "Step Forward 1 Frame"),
        ("cmd.transport.step.back.five", "Step Back Five Frames"),
        (
            "cmd.transport.step.forward.five",
            "Step Forward Five Frames",
        ),
        ("cmd.transport.sequence.start", "Go to Sequence Start"),
        ("cmd.transport.sequence.end", "Go to Sequence End"),
        (
            "cmd.transport.selectedclip.start",
            "Go to Selected Clip Start",
        ),
        ("cmd.transport.selectedclip.end", "Go to Selected Clip End"),
        ("cmd.transport.playintoout", "Play In to Out"),
        ("cmd.transport.play.ctitoout", "Play from Playhead to Out"),
        ("cmd.transport.playedit", "Play Around"),
        ("cmd.tlnav.next.edit", "Go to Next Edit Point"),
        ("cmd.tlnav.prev.edit", "Go to Previous Edit Point"),
        (
            "cmd.tlnav.next.edit.any.track",
            "Go to Next Edit Point on Any Track",
        ),
        (
            "cmd.tlnav.prev.edit.any.track",
            "Go to Previous Edit Point on Any Track",
        ),
        (
            "cmd.tlnav.select.clip.at.playhead",
            "Select Clip at Playhead",
        ),
        ("cmd.tlnav.select.next.clip", "Select Next Clip"),
        ("cmd.tlnav.select.previous.clip", "Select Previous Clip"),
        (
            "cmd.tlnav.toggle.all.target.video",
            "Toggle All Video Track Targets",
        ),
        (
            "cmd.tlnav.toggle.all.target.audio",
            "Toggle All Audio Track Targets",
        ),
        (
            "cmd.tlnav.toggle.all.source.video",
            "Toggle All Source Video",
        ),
        (
            "cmd.tlnav.toggle.all.source.audio",
            "Toggle All Source Audio",
        ),
        ("cmd.tlnav.trim.in.to.cti", "Trim Start to Playhead"),
        ("cmd.tlnav.trim.out.to.cti", "Trim End to Playhead"),
        ("cmd.tlnav.zoomto.sequence", "Zoom to Sequence"),
        (
            "cmd.timeline.nudge.left.one",
            "Nudge Clip Selection Left One Frame",
        ),
        (
            "cmd.timeline.nudge.right.one",
            "Nudge Clip Selection Right One Frame",
        ),
        (
            "cmd.timeline.nudge.left.several",
            "Nudge Clip Selection Left Five Frames",
        ),
        (
            "cmd.timeline.nudge.right.several",
            "Nudge Clip Selection Right Five Frames",
        ),
        ("cmd.timeline.nudge.up", "Nudge Clip Selection Up"),
        ("cmd.timeline.nudge.down", "Nudge Clip Selection Down"),
        (
            "cmd.timeline.slip.left.one",
            "Slip Clip Selection Left One Frame",
        ),
        (
            "cmd.timeline.slip.right.one",
            "Slip Clip Selection Right One Frame",
        ),
        (
            "cmd.timeline.slip.left.several",
            "Slip Clip Selection Left Five Frames",
        ),
        (
            "cmd.timeline.slip.right.several",
            "Slip Clip Selection Right Five Frames",
        ),
        (
            "cmd.timeline.slide.left.one",
            "Slide Clip Selection Left One Frame",
        ),
        (
            "cmd.timeline.slide.right.one",
            "Slide Clip Selection Right One Frame",
        ),
        (
            "cmd.timeline.slide.left.several",
            "Slide Clip Selection Left Five Frames",
        ),
        (
            "cmd.timeline.slide.right.several",
            "Slide Clip Selection Right Five Frames",
        ),
        ("cmd.timeline.paste.to.same.track", "Paste"),
        ("cmd.timeline.pasteinsert.to.same.track", "Paste Insert"),
        ("cmd.timeline.ripple.delete", "Ripple Delete"),
        (
            "cmd.timeline.increase.video.tracks.height",
            "Increase Video Track Height",
        ),
        (
            "cmd.timeline.decrease.video.tracks.height",
            "Decrease Video Track Height",
        ),
        (
            "cmd.timeline.increase.audio.tracks.height",
            "Increase Audio Track Height",
        ),
        (
            "cmd.timeline.decrease.audio.tracks.height",
            "Decrease Audio Track Height",
        ),
        ("cmd.timeline.expand.all.tracks", "Expand All Tracks"),
        ("cmd.timeline.minimize.all.tracks", "Minimize All Tracks"),
        ("cmd.timeline.show.next.screen", "Show Next Screen"),
        ("cmd.timeline.show.previous.screen", "Show Previous Screen"),
        ("cmd.graphics.add.text", "New Text Layer"),
        ("cmd.graphics.add.shape.rectangle", "New Rectangle"),
        ("cmd.graphics.add.shape.ellipse", "New Ellipse"),
        ("cmd.zoom.in", "Zoom In"),
        ("cmd.zoom.out", "Zoom Out"),
        ("cmd.toggle.audio.scrubbing", "Audio Scrubbing"),
        ("cmd.window.workspace.revert", "Reset to Saved Layout"),
        ("cmd.history.step.backward", "Step Backward"),
        ("cmd.history.step.forward", "Step Forward"),
        ("cmd.project.toggle.view", "Toggle Project View"),
        ("cmd.project.openinsource", "Open in Source Monitor"),
        ("uif.window.Projects", "Project"),
        ("uif.window.Source Monitors", "Source Monitor"),
        ("uif.window.Timelines", "Timeline"),
        ("uif.window.Program Monitors", "Program Monitor"),
        ("uif.window.Effect Controls", "Effect Controls"),
        ("uif.window.Audio Mixers", "Audio Track Mixer"),
        ("uif.window.Effects", "Effects"),
    ];
    if let Some(n) = NAMES.iter().find(|(c, _)| *c == cmd).map(|(_, n)| *n) {
        return t(n).to_string();
    }
    if let Some(tool) = op_application::Tool::from_command(cmd) {
        return t(crate::panels::tool_name(tool)).to_string();
    }
    if let Some(i) = cmd
        .strip_prefix("cmd.window.user.workspace.")
        .and_then(|i| i.parse::<usize>().ok())
        && let Some(w) = crate::workspace::Workspace::ALL.get(i)
    {
        return tf("Workspace: {}", &[&t(w.title())]);
    }
    cmd.to_string()
}

fn context_name(c: &str) -> &'static str {
    match c {
        keymap::GLOBAL => "Application",
        keymap::TIMELINE => "Timeline",
        keymap::PROGRAM => "Program Monitor",
        keymap::SOURCE => "Source Monitor",
        keymap::PROJECT => "Project",
        keymap::EFFECT_CONTROLS => "Effect Controls",
        keymap::EFFECTS => "Effects",
        keymap::HISTORY => "History",
        _ => "Other",
    }
}

fn implemented(cmd: &str) -> bool {
    op_application::commands::EDITOR_COMMANDS.contains(&cmd)
        || op_application::Tool::from_command(cmd).is_some()
        || command_name(cmd) != cmd
}

fn shortcuts_dialog(s: &mut State, ctx: &egui::Context, f: &mut ShortcutsForm) -> bool {
    let mut open = true;
    // a key pressed while capturing becomes the new binding
    if let Some((context, cmd)) = f.capture.clone()
        && let Some(c) = keys::capture(ctx)
    {
        if c.key == "Escape" && !c.ctrl && !c.shift && !c.alt {
            f.capture = None;
        } else {
            let keys = c.format();
            s.ed.prefs
                .shortcuts
                .retain(|(x, y, _)| !(*x == context && *y == cmd));
            s.ed.prefs
                .shortcuts
                .push((context.clone(), cmd.clone(), keys.clone()));
            s.ed.keymap = Keymap::with_overrides(&s.ed.prefs.shortcuts);
            let _ = s.ed.prefs.save(&s.ed.dirs);
            let others: Vec<String> =
                s.ed.keymap
                    .bindings
                    .iter()
                    .filter(|b| {
                        b.context == context
                            && b.command != cmd
                            && Chord::parse(&b.keys).as_ref() == Some(&c)
                    })
                    .map(|b| command_name(&b.command))
                    .collect();
            f.message = (!others.is_empty())
                .then(|| tf("{} is also used by: {}", &[&keys, &others.join(", ")]));
            f.capture = None;
        }
    }
    window(ctx, t("Keyboard Shortcuts"))
        .open(&mut open)
        .collapsible(false)
        .default_size([640.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut f.search)
                        .hint_text(t("Search"))
                        .desired_width(220.0),
                );
                if ui.button(t("Import from Premiere Pro (.kys)...")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter(t("Keyboard Shortcuts"), &["kys"])
                        .pick_file()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|text| keymap::import_kys(&text))
                    {
                        Ok(bindings) => {
                            let n = bindings.len();
                            for Binding {
                                context,
                                command,
                                keys,
                            } in bindings
                            {
                                s.ed.prefs
                                    .shortcuts
                                    .retain(|(x, y, _)| !(*x == context && *y == command));
                                s.ed.prefs.shortcuts.push((context, command, keys));
                            }
                            s.ed.keymap = Keymap::with_overrides(&s.ed.prefs.shortcuts);
                            let _ = s.ed.prefs.save(&s.ed.dirs);
                            f.message = Some(tf("Imported {} shortcuts", &[&n]));
                        }
                        Err(e) => f.message = Some(e),
                    }
                }
                if ui.button(t("Reset to Defaults")).clicked() {
                    s.ed.prefs.shortcuts.clear();
                    s.ed.keymap = Keymap::defaults();
                    let _ = s.ed.prefs.save(&s.ed.dirs);
                }
            });
            if let Some(m) = &f.message {
                ui.label(RichText::new(m).color(theme::WARN));
            }
            if f.capture.is_some() {
                ui.label(
                    RichText::new(t("Press the new shortcut (Escape cancels)"))
                        .color(theme::ACCENT),
                );
            }
            ui.separator();
            let q = f.search.to_lowercase();
            let mut rows: Vec<(String, String, String, String)> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for b in &s.ed.keymap.bindings {
                if !implemented(&b.command) || b.context == keymap::SOURCE {
                    continue;
                }
                let name = command_name(&b.command);
                if !q.is_empty()
                    && !name.to_lowercase().contains(&q)
                    && !b.keys.to_lowercase().contains(&q)
                {
                    continue;
                }
                if seen.insert((b.context.clone(), b.command.clone())) {
                    let keys =
                        s.ed.keymap
                            .bindings
                            .iter()
                            .filter(|x| x.context == b.context && x.command == b.command)
                            .map(|x| keys::display(&x.keys))
                            .collect::<Vec<_>>()
                            .join(", ");
                    rows.push((name, b.context.clone(), b.command.clone(), keys));
                }
            }
            rows.sort_by(|a, b| (context_name(&a.1), &a.0).cmp(&(context_name(&b.1), &b.0)));
            let conflicts: Vec<(String, String)> =
                s.ed.keymap
                    .conflicts()
                    .into_iter()
                    .flat_map(|(ctx, _, cmds)| cmds.into_iter().map(move |c| (ctx.clone(), c)))
                    .collect();
            egui::ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    egui::Grid::new("shortcuts")
                        .num_columns(3)
                        .striped(true)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (name, context, cmd, keys) in rows {
                                ui.label(name);
                                ui.label(
                                    RichText::new(t(context_name(&context))).color(theme::TEXT_DIM),
                                );
                                let capturing = f
                                    .capture
                                    .as_ref()
                                    .is_some_and(|(c, x)| *c == context && *x == cmd);
                                let conflict = conflicts.contains(&(context.clone(), cmd.clone()));
                                let text =
                                    if capturing {
                                        RichText::new(t("Press a key...")).color(theme::ACCENT)
                                    } else {
                                        RichText::new(if keys.is_empty() {
                                            "-".to_string()
                                        } else {
                                            keys
                                        })
                                        .monospace()
                                        .color(if conflict { theme::ERROR } else { theme::VALUE })
                                    };
                                ui.horizontal(|ui| {
                                    if ui.button(text).clicked() {
                                        f.capture = Some((context.clone(), cmd.clone()));
                                        f.message = None;
                                    }
                                    if ui
                                        .small_button("\u{00D7}")
                                        .on_hover_text(t("Remove shortcut"))
                                        .clicked()
                                    {
                                        s.ed.prefs
                                            .shortcuts
                                            .retain(|(x, y, _)| !(*x == context && *y == cmd));
                                        s.ed.prefs.shortcuts.push((
                                            context.clone(),
                                            cmd.clone(),
                                            String::new(),
                                        ));
                                        s.ed.keymap = Keymap::with_overrides(&s.ed.prefs.shortcuts);
                                        let _ = s.ed.prefs.save(&s.ed.dirs);
                                    }
                                });
                                ui.end_row();
                            }
                        });
                });
        });
    open
}

// ------------------------------------------------------------------------------ preferences

fn level_name(i: usize) -> &'static str {
    match i {
        0 => t("Errors only"),
        1 => t("Warnings and errors"),
        2 => t("Normal"),
        3 => t("Detailed"),
        _ => t("Everything (very detailed)"),
    }
}

fn preferences(s: &mut State, ctx: &egui::Context, scale: &mut f32) -> bool {
    let mut open = true;
    let mut show_log = false;
    let mut open_logs = false;
    window(ctx, t("Preferences"))
        .open(&mut open)
        .resizable(false)
        .default_width(500.0)
        .show(ctx, |ui| {
            ui.set_min_width(480.0);
            let p = &mut s.ed.prefs;
            let mut log_changed = false;
            ui.label(RichText::new(t("General")).strong().color(theme::TEXT_BRIGHT));
            ui.add_space(2.0);
            egui::Grid::new("prefs")
                .num_columns(2)
                .spacing([14.0, 8.0])
                .min_col_width(190.0)
                .show(ui, |ui| {
                    ui.label(t("Language"));
                    let current = if p.language.is_empty() {
                        t("System").to_string()
                    } else {
                        i18n::Lang::from_code(&p.language)
                            .map(|l| l.name().to_string())
                            .unwrap_or_default()
                    };
                    egui::ComboBox::from_id_salt("lang")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(p.language.is_empty(), t("System"))
                                .clicked()
                            {
                                p.language.clear();
                                i18n::set(i18n::system());
                            }
                            for l in i18n::Lang::ALL {
                                if ui
                                    .selectable_label(p.language == l.code(), l.name())
                                    .clicked()
                                {
                                    p.language = l.code().to_string();
                                    i18n::set(l);
                                }
                            }
                        });
                    ui.end_row();
                    // the scale is chosen first and applied on request: resizing the whole
                    // interface while the slider moves makes the slider jump away
                    ui.label(t("Interface Scale"));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Slider::new(scale, 0.75..=2.5)
                                .step_by(0.05)
                                .custom_formatter(|v, _| format!("{:.0} %", v * 100.0)),
                        );
                        let differs = (*scale - p.ui_scale).abs() > 0.001;
                        if ui
                            .add_enabled(differs, egui::Button::new(t("Apply")))
                            .clicked()
                        {
                            p.ui_scale = *scale;
                            log::info!("interface scale {:.0} %", *scale * 100.0);
                        }
                        if ui
                            .add_enabled(
                                (p.ui_scale - 1.0).abs() > 0.001 || differs,
                                egui::Button::new("100 %"),
                            )
                            .on_hover_text(t("Reset to the normal size"))
                            .clicked()
                        {
                            *scale = 1.0;
                            p.ui_scale = 1.0;
                        }
                    });
                    ui.end_row();
                    ui.label(t("Automatically Save Every"));
                    ui.add(
                        egui::DragValue::new(&mut p.autosave_minutes)
                            .range(0..=120)
                            .suffix(" min"),
                    );
                    ui.end_row();
                    ui.label(t("Autosaved Versions to Keep"));
                    ui.add(egui::DragValue::new(&mut p.autosave_keep).range(1..=100));
                    ui.end_row();
                    ui.label(t("Playback Resolution"));
                    egui::ComboBox::from_id_salt("pref-pres")
                        .selected_text(res_label(p.playback_resolution))
                        .show_ui(ui, |ui| {
                            for d in [1u32, 2, 4, 8] {
                                ui.selectable_value(&mut p.playback_resolution, d, res_label(d));
                            }
                        });
                    ui.end_row();
                    ui.label(t("Paused Resolution"));
                    egui::ComboBox::from_id_salt("pref-pau")
                        .selected_text(res_label(p.paused_resolution))
                        .show_ui(ui, |ui| {
                            for d in [1u32, 2, 4, 8] {
                                ui.selectable_value(&mut p.paused_resolution, d, res_label(d));
                            }
                        });
                    ui.end_row();
                    ui.label(t("Frame Cache"));
                    if ui
                        .add(
                            egui::DragValue::new(&mut p.frame_cache_mb)
                                .range(128..=65536)
                                .speed(16)
                                .suffix(" MB"),
                        )
                        .changed()
                    {
                        s.ed.media.set_frame_budget(p.frame_cache_mb << 20);
                    }
                    ui.end_row();
                    ui.label(t("Audio Scrubbing"));
                    ui.checkbox(&mut p.audio_scrubbing, "");
                    ui.end_row();
                    ui.label(t("Hardware Encoding"));
                    ui.checkbox(
                        &mut p.hardware_encoding,
                        t("Use the GPU encoder when available"),
                    );
                    ui.end_row();
                    ui.label(t("Snap in Timeline"));
                    ui.checkbox(&mut p.snapping, "");
                    ui.end_row();
                    ui.label(t("Linked Selection"));
                    ui.checkbox(&mut p.linked_selection, "");
                    ui.end_row();
                    // one grid for both sections keeps their columns aligned; an empty row
                    // separates them (grids do not take plain spacing)
                    ui.label("");
                    ui.end_row();
                    ui.label(RichText::new(t("Log")).strong().color(theme::TEXT_BRIGHT));
                    ui.end_row();
                    ui.label(t("Logging"));
                    log_changed |= ui
                        .checkbox(&mut p.logging_enabled, t("Write a log file"))
                        .on_hover_text(t(
                            "With logging off, only errors are kept in memory; crash reports are always written.",
                        ))
                        .changed();
                    ui.end_row();
                    ui.label(t("Detail Level"));
                    let cur = op_application::logging::LEVELS
                        .iter()
                        .position(|l| *l == p.log_level)
                        .unwrap_or(2);
                    ui.add_enabled_ui(p.logging_enabled, |ui| {
                        egui::ComboBox::from_id_salt("pref-log")
                            .selected_text(level_name(cur))
                            .show_ui(ui, |ui| {
                                for (i, l) in op_application::logging::LEVELS.iter().enumerate() {
                                    if ui.selectable_label(i == cur, level_name(i)).clicked() {
                                        p.log_level = l.to_string();
                                        log_changed = true;
                                    }
                                }
                            });
                    });
                    ui.end_row();
                    ui.label("");
                    ui.horizontal(|ui| {
                        if ui.button(t("Show Log...")).clicked() {
                            show_log = true;
                        }
                        if ui.button(t("Open Log Folder")).clicked() {
                            open_logs = true;
                        }
                    });
                    ui.end_row();
                });
            if log_changed {
                op_application::logging::configure(p.logging_enabled, &p.log_level);
            }
            ui.add_space(6.0);
            let (dev, rate, _) = s.ed.playback.device_info();
            ui.label(
                RichText::new(tf("Audio output: {} ({} Hz)", &[&dev, &rate]))
                    .color(theme::TEXT_DIM)
                    .size(11.0),
            );
            ui.label(
                RichText::new(tf("Settings folder: {}", &[&s.ed.dirs.config.display()]))
                    .color(theme::TEXT_DIM)
                    .size(11.0),
            );
        });
    if show_log && !s.dialogs.iter().any(|d| matches!(d, Dialog::Log { .. })) {
        s.dialogs.push(Dialog::log());
    }
    if open_logs && let Some(dir) = op_application::logging::folder() {
        crate::app::open_folder(&dir);
    }
    if !open {
        let _ = s.ed.prefs.save(&s.ed.dirs);
    }
    open
}

/// The program log with a detail filter and a search field.
fn log_window(ctx: &egui::Context, level: &mut usize, filter: &mut String) -> bool {
    let mut open = true;
    window(ctx, t("Log"))
        .open(&mut open)
        .resizable(true)
        .default_size([820.0, 480.0])
        .show(ctx, |ui| {
            let names = [
                t("Errors"),
                t("Warnings"),
                t("Information"),
                t("Debugging"),
                t("Everything"),
            ];
            let wanted = match *level {
                0 => log::LevelFilter::Error,
                1 => log::LevelFilter::Warn,
                2 => log::LevelFilter::Info,
                3 => log::LevelFilter::Debug,
                _ => log::LevelFilter::Trace,
            };
            let q = filter.to_lowercase();
            let lines: Vec<op_application::logging::Line> = op_application::logging::recent(wanted)
                .into_iter()
                .filter(|l| {
                    q.is_empty()
                        || l.message.to_lowercase().contains(&q)
                        || l.target.to_lowercase().contains(&q)
                })
                .collect();
            ui.horizontal(|ui| {
                ui.label(t("Show"));
                egui::ComboBox::from_id_salt("log-level")
                    .selected_text(names[(*level).min(4)])
                    .show_ui(ui, |ui| {
                        for (i, n) in names.iter().enumerate() {
                            ui.selectable_value(level, i, *n);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(filter)
                        .hint_text(t("Search"))
                        .desired_width(200.0),
                );
                if ui.button(t("Copy All")).clicked() {
                    let text: Vec<String> = lines.iter().map(|l| l.format()).collect();
                    ui.ctx().copy_text(text.join("\n"));
                }
                if ui.button(t("Clear")).clicked() {
                    op_application::logging::clear();
                }
                if ui.button(t("Open Log Folder")).clicked()
                    && let Some(dir) = op_application::logging::folder()
                {
                    crate::app::open_folder(&dir);
                }
            });
            ui.separator();
            let row_h = 16.0;
            egui::ScrollArea::both()
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show_rows(ui, row_h, lines.len(), |ui, range| {
                    for l in &lines[range] {
                        let color = match l.level {
                            log::Level::Error => theme::ERROR,
                            log::Level::Warn => theme::WARN,
                            log::Level::Info => theme::TEXT,
                            _ => theme::TEXT_DIM,
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(l.format())
                                    .monospace()
                                    .size(11.5)
                                    .color(color),
                            )
                            .extend(),
                        );
                    }
                });
            if lines.is_empty() {
                ui.label(RichText::new(t("No messages")).color(theme::TEXT_DIM));
            }
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    open
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

fn res_label(d: u32) -> String {
    match d {
        1 => t("Full").to_string(),
        n => format!("1/{n}"),
    }
}
