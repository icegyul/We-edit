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

/// The user's videos folder (Videos on Windows, Movies on macOS, the XDG folder on Linux in the
/// user's language), else the home folder.
fn dirs_video() -> Option<PathBuf> {
    dirs::video_dir()
        .filter(|v| v.is_dir())
        .or_else(dirs::home_dir)
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

                            ui.label(t("Captions In"));
                            ui.vertical(|ui| {
                                ui.radio_value(
                                    &mut form.translate,
                                    false,
                                    t("The language spoken"),
                                );
                                ui.radio_value(
                                    &mut form.translate,
                                    true,
                                    t("English (translated)"),
                                )
                                .on_hover_text(t(
                                    "The speech model translates into English only; to caption in another language, choose it as the spoken language.",
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
        ("cmd.edit.removeattributes", "Remove Attributes..."),
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
    let mut new_profile: Option<Profile> = None;
    let mut new_custom: Option<op_application::performance::CustomSettings> = None;
    let mut show_log = false;
    let mut open_logs = false;
    window(ctx, t("Preferences"))
        .open(&mut open)
        .resizable(false)
        .default_width(500.0)
        .show(ctx, |ui| {
            ui.set_min_width(480.0);
            // taller than small screens: the settings scroll
            let max_h = (ui.ctx().content_rect().height() - 140.0).max(240.0);
            egui::ScrollArea::vertical()
                .max_height(max_h)
                .min_scrolled_height(max_h.min(640.0))
                .auto_shrink([false, true])
                .show(ui, |ui| {
            let p = &mut s.ed.prefs;
            let mut log_changed = false;
            ui.label(RichText::new(t("General")).strong().family(crate::fonts::strong()).color(theme::TEXT_BRIGHT));
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
                    ui.label(t("Interface Font"));
                    let fonts = [
                        ("system", t("System (Segoe UI, San Francisco...)")),
                        ("classic", t("Classic (built in)")),
                    ];
                    let shown = fonts
                        .iter()
                        .find(|(k, _)| *k == p.ui_font)
                        .map(|(_, l)| *l)
                        .unwrap_or(fonts[0].1);
                    egui::ComboBox::from_id_salt("pref-font")
                        .selected_text(shown)
                        .show_ui(ui, |ui| {
                            for (k, l) in fonts {
                                if ui.selectable_label(p.ui_font == k, l).clicked() && p.ui_font != k {
                                    p.ui_font = k.to_string();
                                    crate::fonts::install(ctx, k == "system");
                                }
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
                    let preview_before = (p.playback_resolution, p.paused_resolution, p.frame_cache_mb);
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
                    ui.label(RichText::new(t("Log")).strong().family(crate::fonts::strong()).color(theme::TEXT_BRIGHT));
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
                    ui.label("");
                    ui.end_row();
                    ui.label(
                        RichText::new(t("Performance"))
                            .strong().family(crate::fonts::strong())
                            .color(theme::TEXT_BRIGHT),
                    );
                    ui.end_row();
                    // a preview setting changed by hand turns the profile into custom settings
                    if (p.playback_resolution, p.paused_resolution, p.frame_cache_mb) != preview_before
                        && p.performance_custom.is_none()
                    {
                        new_custom = Some(op_application::performance::CustomSettings::from_profile(
                            p.profile(),
                        ));
                    }
                    ui.label(t("Performance Profile"));
                    let current = p.profile();
                    let custom = p.performance_custom;
                    let mut level = current.index() as f32;
                    ui.vertical(|ui| {
                        ui.spacing_mut().slider_width = 240.0;
                        let slider = egui::Slider::new(&mut level, 0.0..=4.0)
                            .step_by(1.0)
                            .show_value(false);
                        if ui.add(slider).changed() {
                            new_profile = Some(Profile::from_index(level.round() as u8));
                        }
                        // the two ends of the scale, under its ends
                        ui.allocate_ui_with_layout(
                            egui::vec2(240.0, 14.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
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
                            },
                        );
                    });
                    ui.end_row();
                    ui.label("");
                    ui.vertical(|ui| {
                        ui.set_max_width(300.0);
                        let (name, about) = match custom {
                            Some(c) => (
                                t("Custom"),
                                tf(
                                    "Settings chosen one by one, starting from {}. Move the slider to go back to a profile.",
                                    &[&t(Profile::from_index(c.base).label())],
                                ),
                            ),
                            None => (t(current.label()), t(current.description()).to_string()),
                        };
                        ui.label(RichText::new(name).strong().family(crate::fonts::strong()).color(theme::TEXT_BRIGHT));
                        ui.add(
                            egui::Label::new(
                                RichText::new(about).size(11.5).color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                    });
                    ui.end_row();
                    // every setting a profile makes, to change one by one
                    ui.label("");
                    egui::CollapsingHeader::new(t("Custom Settings"))
                        .id_salt("pref-perf-custom")
                        .default_open(custom.is_some())
                        .show(ui, |ui| {
                            let mut c = custom.unwrap_or_else(|| {
                                op_application::performance::CustomSettings::from_profile(current)
                            });
                            let before = c;
                            egui::Grid::new("pref-perf-grid")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(t("Interface Animations"));
                                    ui.checkbox(&mut c.animations, "");
                                    ui.end_row();
                                    ui.label(t("Smooth Scrolling"));
                                    ui.checkbox(&mut c.smooth_scroll, "");
                                    ui.end_row();
                                    ui.label(t("Redraws During Playback"));
                                    let fps_label = |f: u32| {
                                        if f == 0 {
                                            t("Display refresh rate").to_string()
                                        } else {
                                            tf("{} per second", &[&f])
                                        }
                                    };
                                    egui::ComboBox::from_id_salt("pref-fps")
                                        .selected_text(fps_label(c.playback_fps))
                                        .show_ui(ui, |ui| {
                                            for f in [24u32, 30, 60, 0] {
                                                ui.selectable_value(
                                                    &mut c.playback_fps,
                                                    f,
                                                    fps_label(f),
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    ui.label(t("Frames Decoded Ahead"));
                                    ui.add(egui::DragValue::new(&mut c.read_ahead).range(2..=96))
                                        .on_hover_text(t(
                                            "More frames ahead play smoother on slow disks and codecs, and use more memory.",
                                        ));
                                    ui.end_row();
                                    ui.label(t("Timeline Thumbnails"));
                                    ui.checkbox(&mut c.thumbnails, "");
                                    ui.end_row();
                                    ui.label(t("Audio Waveforms"));
                                    ui.checkbox(&mut c.waveforms, "");
                                    ui.end_row();
                                    ui.label(t("Prepare Effects in Background"));
                                    ui.checkbox(&mut c.warm_up, "").on_hover_text(t(
                                        "Compiles every effect while the program is idle after it starts, so the first use of an effect does not pause playback.",
                                    ));
                                    ui.end_row();
                                });
                            ui.label(
                                RichText::new(t(
                                    "Playback Resolution, Paused Resolution and Frame Cache are above.",
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            );
                            if c != before {
                                new_custom = Some(c);
                            }
                        });
                    ui.end_row();
                    ui.label(t("This Computer"));
                    let hw = &s.hardware;
                    let rec = hw.recommended();
                    ui.vertical(|ui| {
                        ui.set_max_width(300.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(hardware_summary(hw))
                                    .size(11.5)
                                    .color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(tf("Recommended: {}", &[&t(rec.label())]))
                                    .size(11.5),
                            );
                            if (rec != current || custom.is_some())
                                && ui.small_button(t("Use Recommended")).clicked()
                            {
                                new_profile = Some(rec);
                            }
                        });
                    });
                    ui.end_row();
                    ui.label(t("Hardware Decoding"));
                    let modes = [
                        ("auto", t("Automatic")),
                        ("always", t("Always (graphics card)")),
                        ("never", t("Never (processor only)")),
                    ];
                    let current = modes
                        .iter()
                        .find(|(k, _)| *k == p.hardware_decoding)
                        .map(|(_, l)| *l)
                        .unwrap_or(modes[0].1);
                    egui::ComboBox::from_id_salt("pref-hwdec")
                        .selected_text(current)
                        .show_ui(ui, |ui| {
                            for (k, l) in modes {
                                if ui.selectable_label(p.hardware_decoding == k, l).clicked() {
                                    p.hardware_decoding = k.to_string();
                                    s.ed.media.set_hardware_decoding(
                                        op_application::media::HardwareDecoding::from_pref(k),
                                    );
                                }
                            }
                        })
                        .response
                        .on_hover_text(t(
                            "Automatic uses the processor and moves a video to the graphics card's decoder when the processor cannot play it in real time.",
                        ));
                    ui.end_row();
                    ui.label(t("Graphics API"));
                    let mut apis = vec![("auto", t("Automatic"))];
                    if cfg!(windows) {
                        apis.push(("dx12", "Direct3D 12"));
                    }
                    if cfg!(target_os = "macos") {
                        apis.push(("metal", "Metal"));
                    } else {
                        apis.push(("vulkan", "Vulkan"));
                    }
                    apis.push(("gl", "OpenGL"));
                    apis.push(("software", t("Software Only (processor)")));
                    let current = apis
                        .iter()
                        .find(|(k, _)| *k == p.graphics_backend)
                        .map(|(_, l)| *l)
                        .unwrap_or(apis[0].1);
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("pref-api")
                            .selected_text(current)
                            .show_ui(ui, |ui| {
                                for (k, l) in &apis {
                                    if ui.selectable_label(p.graphics_backend == *k, *l).clicked() {
                                        p.graphics_backend = k.to_string();
                                    }
                                }
                            });
                        ui.label(
                            RichText::new(t("Applied at the next start"))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                        );
                    });
                    ui.end_row();
                    ui.label("");
                    ui.end_row();
                    ui.label(
                        RichText::new(t("Pasted Images"))
                            .strong().family(crate::fonts::strong())
                            .color(theme::TEXT_BRIGHT),
                    );
                    ui.end_row();
                    ui.label(t("Save In"));
                    ui.horizontal(|ui| {
                        let mut text = p
                            .paste_folder
                            .as_ref()
                            .map(|f| f.display().to_string())
                            .unwrap_or_default();
                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text(t("Next to the project"))
                                    .desired_width(220.0),
                            )
                            .changed()
                        {
                            p.paste_folder =
                                (!text.trim().is_empty()).then(|| PathBuf::from(text.trim()));
                        }
                        if ui.button(t("Browse...")).clicked()
                            && let Some(dir) = rfd::FileDialog::new().pick_folder()
                        {
                            p.paste_folder = Some(dir);
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut p.paste_always, t("Always save here without asking"));
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
        });
    if show_log && !s.dialogs.iter().any(|d| matches!(d, Dialog::Log { .. })) {
        s.dialogs.push(Dialog::log());
    }
    if open_logs && let Some(dir) = op_application::logging::folder() {
        crate::app::open_folder(&dir);
    }
    // a profile also sets the preview resolutions, frame cache and read-ahead, at once
    if let Some(p) = new_profile {
        let hw = s.hardware.clone();
        s.ed.set_performance_profile(p, Some(&hw));
    } else if let Some(c) = new_custom {
        s.ed.set_performance_custom(c);
    }
    if !open {
        let _ = s.ed.prefs.save(&s.ed.dirs);
    }
    open
}

/// The program log with a detail filter and a search field.
fn log_window(ctx: &egui::Context, level: &mut usize, filter: &mut String) -> bool {
    let mut open = true;
    let mut close = false;
    // a regular pop-up of a fixed size: the list scrolls inside it instead of the window
    // growing to the height of the screen
    let screen = ctx.content_rect().size();
    let size = egui::vec2(
        (screen.x * 0.8).clamp(360.0, 860.0),
        (screen.y * 0.5).clamp(200.0, 420.0),
    );
    window(ctx, t("Log"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(size.x);
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
                .max_height(size.y)
                .min_scrolled_height(size.y)
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
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("Close")).clicked() {
                    close = true;
                }
            });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    open && !close
}

/// Asks where to save a pasted image; "Always save here" skips the question next time.
fn paste_dialog(
    s: &mut State,
    ctx: &egui::Context,
    id: u64,
    folder: &mut String,
    name: &mut String,
    always: &mut bool,
) -> bool {
    let Some(pos) = s.pastes.iter().position(|j| j.id == id) else {
        return false;
    };
    // the preview is made once the data is there (links download in the background)
    let state = {
        let data = s.pastes[pos].data.lock();
        match data.as_ref() {
            None => None,
            Some(Ok(p)) => Some(Ok(crate::paste::preview_image(p))),
            Some(Err(e)) => Some(Err(e.clone())),
        }
    };
    if s.pastes[pos].preview.is_none()
        && let Some(Ok(Some(img))) = &state
    {
        s.pastes[pos].preview = Some(ctx.load_texture(
            format!("paste-{id}"),
            img.clone(),
            egui::TextureOptions::LINEAR,
        ));
    }
    let mut open = true;
    let mut save = false;
    let mut cancel = false;
    let mut browse = false;
    window(ctx, t("Paste Image"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            ui.set_width(480.0);
            let job = &s.pastes[pos];
            if let Some(tex) = &job.preview {
                let size = tex.size_vec2();
                let k = (480.0 / size.x).min(240.0 / size.y).min(1.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((tex.id(), size * k)));
                });
            }
            ui.label(
                RichText::new(&job.source)
                    .size(11.0)
                    .color(theme::TEXT_DIM),
            );
            match &state {
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t("Downloading..."));
                    });
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(tf("Could not get the image: {}", &[e])).color(theme::ERROR));
                }
                Some(Ok(_)) => {}
            }
            ui.add_space(6.0);
            egui::Grid::new("paste-grid")
                .num_columns(2)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    ui.label(t("File Name"));
                    ui.add(egui::TextEdit::singleline(name).desired_width(340.0));
                    ui.end_row();
                    ui.label(t("Save In"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(folder).desired_width(260.0));
                        if ui.button(t("Browse...")).clicked() {
                            browse = true;
                        }
                    });
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(always, t("Always save here"))
                        .on_hover_text(t("Pasted images will be saved in this folder without asking. You can change it in Preferences."));
                    ui.end_row();
                });
            let ready = matches!(state, Some(Ok(_)));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("Cancel")).clicked() {
                        cancel = true;
                    }
                    if ui
                        .add_enabled(
                            ready && !folder.trim().is_empty(),
                            egui::Button::new(t("Save and Import")),
                        )
                        .clicked()
                    {
                        save = true;
                    }
                });
            });
        });
    if browse
        && let Some(dir) = rfd::FileDialog::new()
            .set_directory(folder.trim())
            .pick_folder()
    {
        *folder = dir.display().to_string();
    }
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if save {
        let dir = PathBuf::from(folder.trim());
        s.ed.prefs.paste_folder = Some(dir.clone());
        if *always {
            s.ed.prefs.paste_always = true;
        }
        let _ = s.ed.prefs.save(&s.ed.dirs);
        let job = &mut s.pastes[pos];
        job.folder = Some(dir);
        job.name = name.trim().to_string();
        return false;
    }
    if cancel || esc || !open {
        s.pastes.remove(pos);
        return false;
    }
    true
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

/// "Processor (cores) - memory - graphics" for the performance settings.
pub fn hardware_summary(h: &Hardware) -> String {
    use op_application::performance::GpuKind;
    let kind = match h.gpu_kind {
        GpuKind::Discrete => t("dedicated graphics"),
        GpuKind::Integrated => t("integrated graphics"),
        GpuKind::Software => t("no graphics acceleration"),
        GpuKind::Unknown => t("graphics"),
    };
    format!(
        "{} ({} {}, {} {})\n{:.0} GB {}\n{} ({kind})",
        h.cpu,
        h.cores,
        t("cores"),
        h.threads,
        t("threads"),
        h.memory_gb,
        t("of memory"),
        h.gpu
    )
}

/// File > Link Media: locate missing files by hand or search this computer, then relink them.
fn link_media_dialog(s: &mut State, ctx: &egui::Context, form: &mut LinkForm) -> bool {
    use op_application::relink;
    // results of a running search
    if let Some(search) = &form.search {
        let st = search.state.lock();
        for (id, p) in &st.found {
            form.found.entry(*id).or_insert_with(|| p.clone());
        }
        if st.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }
    let searching = form.search.as_ref().is_some_and(|x| x.state.lock().running);
    let mut locate: Option<usize> = None;
    let mut start_search = false;
    let mut link = false;
    let mut close = false;
    let total = form.missing.len();
    let (_, esc) = modal(ctx, "link-media", t("Link Media"), 620.0, |ui| {
        ui.label(tf(
            "{} media files could not be found where the project expects them. Locate them, or let OpenPremier search this computer.",
            &[&total],
        ));
        ui.add_space(6.0);
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, m) in form.missing.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(470.0);
                            ui.label(
                                RichText::new(&m.name)
                                    .strong()
                                    .family(crate::fonts::strong())
                                    .color(theme::TEXT_BRIGHT),
                            );
                            match form.found.get(&m.asset) {
                                Some(p) => ui.label(
                                    RichText::new(tf("Found: {}", &[&p.display()]))
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(110, 200, 140)),
                                ),
                                None => ui.label(
                                    RichText::new(tf("Missing: {}", &[&m.path]))
                                        .size(11.0)
                                        .color(theme::ERROR),
                                ),
                            };
                        });
                        if ui.button(t("Locate...")).clicked() {
                            locate = Some(i);
                        }
                    });
                    ui.add_space(3.0);
                }
            });
        ui.add_space(6.0);
        if let Some(search) = &form.search {
            let st = search.state.lock();
            ui.horizontal(|ui| {
                if st.running {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Searching... {} folders", &[&st.folders]))
                            .color(theme::TEXT_DIM),
                    );
                } else {
                    ui.label(
                        RichText::new(tf(
                            "Search finished: {} of {} found",
                            &[&form.found.len(), &total],
                        ))
                        .color(theme::TEXT_DIM),
                    );
                }
            });
            if st.running {
                let cur: String = st
                    .current
                    .chars()
                    .rev()
                    .take(80)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                ui.label(RichText::new(cur).size(10.5).color(theme::TEXT_DIM));
            }
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if searching {
                if ui.button(t("Stop Search")).clicked()
                    && let Some(x) = &form.search
                {
                    x.cancel();
                }
            } else if ui
                .add_enabled(form.found.len() < total, egui::Button::new(t("Search Automatically")))
                .on_hover_text(t(
                    "Looks in the project folder, your media folders and every drive, by file name; when several files share a name, the one whose duration and size match wins.",
                ))
                .clicked()
            {
                start_search = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let n = form.found.len();
                if ui
                    .add_enabled(
                        n > 0,
                        egui::Button::new(
                            RichText::new(tf("Link {} Files", &[&n])).color(egui::Color32::WHITE),
                        )
                        .fill(theme::ACCENT_DIM),
                    )
                    .clicked()
                {
                    link = true;
                }
                if ui.button(t("Offline All")).on_hover_text(t("Keep working without them; File > Link Media finds them later.")).clicked() {
                    close = true;
                }
            });
        });
    });
    if let Some(i) = locate {
        let m = form.missing[i].clone();
        let start = Path::new(&m.path)
            .parent()
            .filter(|d| d.is_dir())
            .map(|d| d.to_path_buf())
            .or_else(|| {
                s.ed.path
                    .as_ref()
                    .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            });
        let mut dlg = rfd::FileDialog::new()
            .set_title(tf("Where is {}?", &[&m.name]))
            .set_file_name(&m.name);
        if let Some(dir) = start {
            dlg = dlg.set_directory(dir);
        }
        if let Some(p) = dlg.pick_file() {
            // the others are often in the same folder
            if let Some(dir) = p.parent() {
                for (id, sib) in relink::siblings(dir, &form.missing) {
                    form.found.entry(id).or_insert(sib);
                }
            }
            form.found.insert(m.asset, p);
        }
    }
    if start_search {
        let hints: Vec<PathBuf> = form
            .found
            .values()
            .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
            .collect();
        let roots = relink::search_roots(s.ed.path.as_deref(), &hints);
        let list: Vec<(MediaAsset, String)> = form
            .missing
            .iter()
            .filter(|m| !form.found.contains_key(&m.asset))
            .filter_map(|m| {
                s.ed.project
                    .asset(m.asset)
                    .map(|a| (a.clone(), m.name.clone()))
            })
            .collect();
        form.search = Some(relink::Search::start(list, roots));
    }
    if link {
        if let Some(x) = &form.search {
            x.cancel();
        }
        let links: Vec<(AssetId, PathBuf)> =
            form.found.iter().map(|(a, p)| (*a, p.clone())).collect();
        match s.ed.link_media(&links) {
            Ok(n) => s.ed.info(format!("Linked {n} media files")),
            Err(e) => s.ed.error(e),
        }
        return false;
    }
    if close || esc {
        if let Some(x) = &form.search {
            x.cancel();
        }
        return false;
    }
    true
}

/// Paste Attributes (from the copied clip) and Remove Attributes share their choices.
fn attributes_dialog(s: &mut State, ctx: &egui::Context, a: &mut Attributes, paste: bool) -> bool {
    let title = if paste {
        t("Paste Attributes")
    } else {
        t("Remove Attributes")
    };
    let ((ok, cancel), esc) = modal(ctx, "attributes", title, 300.0, |ui| {
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
    if ok && paste {
        s.ed.paste_attributes(*a);
    } else if ok {
        s.ed.remove_attributes(*a);
    }
    !(ok || cancel || esc)
}
