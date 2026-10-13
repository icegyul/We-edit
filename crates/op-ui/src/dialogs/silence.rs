//! Clip > Remove Silence: finds silences, filler words and repeated takes in the audio, shows
//! them over the waveform to review one by one, and cuts, speeds up or marks them.

use op_application::captions::{CaptionStage, ModelSize};
use op_application::silence::{Action, Detection, Scope, SilenceSettings};
use op_silence as sil;
use sil::{Kind, Method};

use super::*;

/// What the analysis in the dialog was made with. A change means "analyze again" only when the
/// analysis lacks something the settings now need; turning something off never asks for it.
#[derive(Clone, PartialEq)]
struct Key {
    scope: Scope,
    neural: bool,
    words: bool,
    /// The speech model that made the words (the helper's model id, or the built-in one's).
    engine: String,
    language: Option<String>,
}

impl Key {
    /// Whether an analysis made with `self` has everything `need` asks for.
    fn covers(&self, need: &Key) -> bool {
        self.scope == need.scope
            && (self.neural || !need.neural)
            && (!need.words
                || (self.words && self.engine == need.engine && self.language == need.language))
    }
}

fn key_of(s: &SilenceSettings) -> Key {
    Key {
        scope: s.scope,
        neural: true,
        words: s.needs_words(),
        engine: engine_id(s),
        language: s.language.clone(),
    }
}

/// Which speech model the settings will use: the helper's (graphics card) when it is installed.
fn engine_id(s: &SilenceSettings) -> String {
    if op_application::sentences::helper_files().is_some() {
        s.whisper.clone()
    } else {
        s.model.id().to_string()
    }
}

pub struct SilenceForm {
    pub settings: SilenceSettings,
    /// Midpoints of the stretches the user chose to keep.
    keep: Vec<f64>,
    /// Zoom of the preview in pixels per second; 0 fits the whole recording.
    zoom: f32,
    analyzed: Option<Key>,
    error: Option<String>,
}

impl Dialog {
    /// Clip > Remove Silence. Starts measuring right away when no transcript is needed.
    pub fn silence(s: &mut State) -> Option<Dialog> {
        s.ed.active?;
        let mut settings = s.silence_settings.clone();
        // the selected clips when there are some, else the whole sequence
        settings.scope = if s.ed.silence_targets(Scope::Selection).is_ok() {
            Scope::Selection
        } else {
            Scope::Sequence
        };
        s.ed.silence_result = None;
        let mut form = SilenceForm {
            settings,
            keep: Vec::new(),
            zoom: 0.0,
            analyzed: None,
            error: None,
        };
        if !form.settings.needs_words() {
            form.start(s);
        }
        Some(Dialog::Silence(Box::new(form)))
    }
}

impl SilenceForm {
    fn start(&mut self, s: &mut State) {
        self.error = None;
        self.keep.clear();
        s.ed.silence_result = None;
        match s.ed.analyze_silence(&self.settings) {
            Ok(()) => self.analyzed = Some(key_of(&self.settings)),
            Err(e) => self.error = Some(e),
        }
    }

    fn excluded(&self, c: &sil::Cut) -> bool {
        self.keep
            .iter()
            .any(|k| *k >= c.range.start && *k <= c.range.end)
    }
}

fn kind_color(k: Kind) -> egui::Color32 {
    match k {
        Kind::Silence => egui::Color32::from_rgb(230, 90, 90),
        Kind::Gap => egui::Color32::from_rgb(235, 150, 60),
        Kind::Filler => egui::Color32::from_rgb(170, 110, 235),
        Kind::Repeat => egui::Color32::from_rgb(235, 205, 70),
        Kind::Retake => egui::Color32::from_rgb(235, 110, 180),
    }
}

/// Minutes, seconds and tenths: 1:07.3.
fn clock(t: f64) -> String {
    let t = t.max(0.0);
    format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0)
}

fn method_param(ui: &mut Ui, m: &mut Method) -> bool {
    let mut changed = false;
    match m {
        Method::Loudness { threshold_db } => {
            ui.label(t("Silence Below"));
            changed |= ui
                .add(
                    egui::Slider::new(threshold_db, -70.0..=-10.0)
                        .suffix(" dB")
                        .fixed_decimals(0),
                )
                .on_hover_text(t("Anything quieter than this level is silence."))
                .changed();
        }
        Method::Relative { below_peak_db } => {
            ui.label(t("Below the Loudest"));
            changed |= ui
                .add(
                    egui::Slider::new(below_peak_db, 10.0..=60.0)
                        .suffix(" dB")
                        .fixed_decimals(0),
                )
                .on_hover_text(t(
                    "Silence is this much quieter than the recording's loud parts, so it follows the recording's level.",
                ))
                .changed();
        }
        Method::Adaptive { margin_db } => {
            ui.label(t("Above the Noise"));
            changed |= ui
                .add(
                    egui::Slider::new(margin_db, 3.0..=30.0)
                        .suffix(" dB")
                        .fixed_decimals(0),
                )
                .on_hover_text(t(
                    "Sound is this much louder than the recording's own background noise: no level to guess.",
                ))
                .changed();
        }
        Method::Speech { sensitivity } | Method::Neural { sensitivity } => {
            ui.label(t("Sensitivity"));
            let mut pct = *sensitivity * 100.0;
            if ui
                .add(
                    egui::Slider::new(&mut pct, 0.0..=100.0)
                        .suffix(" %")
                        .fixed_decimals(0),
                )
                .on_hover_text(t("Higher keeps more: quiet or unclear speech stays."))
                .changed()
            {
                *sensitivity = pct / 100.0;
                changed = true;
            }
        }
    }
    ui.end_row();
    changed
}

pub(super) fn silence_dialog(s: &mut State, ctx: &egui::Context, form: &mut SilenceForm) -> bool {
    let busy = s.ed.silencing.is_some();
    if busy || s.ed.whisper_dl.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let detection: Option<std::sync::Arc<Detection>> = s.ed.silence_result.clone();
    let stale = !form
        .analyzed
        .as_ref()
        .is_some_and(|a| a.covers(&key_of(&form.settings)));
    let cuts: Vec<sil::Cut> = detection
        .as_ref()
        .map(|d| d.cuts(&form.settings))
        .unwrap_or_default();
    let active: Vec<&sil::Cut> = cuts.iter().filter(|c| !form.excluded(c)).collect();
    let removed: f64 = active.iter().map(|c| c.range.len()).sum();
    let total = detection
        .as_ref()
        .map(|d| d.analysis.duration())
        .unwrap_or(0.0);

    let mut analyze = false;
    let mut apply = false;
    let mut close = false;
    let mut toggle: Option<f64> = None;
    let (_, esc) = modal(ctx, "silence", t("Remove Silence"), 880.0, |ui| {
        ui.horizontal(|ui| {
            ui.label(t("Settings"));
            let presets = sil::presets::all();
            egui::ComboBox::from_id_salt("sil-preset")
                .selected_text(t("Choose a preset..."))
                .width(180.0)
                .show_ui(ui, |ui| {
                    for p in &presets {
                        if ui
                            .selectable_label(false, t(p.name))
                            .on_hover_text(t(p.about))
                            .clicked()
                        {
                            form.settings.sound = p.params;
                            form.settings.words = p.text.clone();
                            form.settings.by_words = p.words;
                        }
                    }
                });
            ui.separator();
            ui.label(t("Apply to"));
            ui.radio_value(
                &mut form.settings.scope,
                Scope::Selection,
                t("Selected clips"),
            );
            ui.radio_value(
                &mut form.settings.scope,
                Scope::Sequence,
                t("Whole sequence"),
            );
            ui.radio_value(&mut form.settings.scope, Scope::InOut, t("In to Out"));
        });
        ui.horizontal(|ui| {
            ui.label(t("Find"));
            ui.checkbox(&mut form.settings.by_sound, t("Silences"));
            ui.checkbox(&mut form.settings.by_words, t("Fillers, Repeats and Retakes"))
                .on_hover_text(t(
                    "Filler words (um, eh), repeated words and phrases, and sentences said twice (the last take stays). Transcribes the audio on this computer; the speech model is downloaded once.",
                ));
            ui.checkbox(&mut form.settings.speech_only, "문장 밖 빈 공간 모두 삭제")
                .on_hover_text("받아쓴 글(단어·문장)이 없는 구간, 즉 말이 시작되기 전, 문장 사이, 끝난 뒤를 모두 지웁니다. 위스퍼로 받아쓰기를 하며, 글이 있는 부분만 남습니다.");
        });
        if form.settings.needs_words() {
            speech_model_row(s, ui, &mut form.settings);
        }
        if form.settings.speech_only {
            ui.horizontal(|ui| {
                ui.label("문장 밖 삭제:");
                ui.label("이보다 긴 빈 공간만");
                ui.add(egui::Slider::new(&mut form.settings.speech_gap, 0.05..=2.0).suffix(" 초"))
                    .on_hover_text("글과 글 사이가 이 길이 이상 비어 있으면 지웁니다. 작을수록 말이 빽빽해집니다.");
                ui.label("앞뒤로 남길 여유");
                ui.add(egui::Slider::new(&mut form.settings.speech_air, 0.0..=0.5).suffix(" 초"))
                    .on_hover_text("말이 끊기지 않게 글 앞뒤로 남겨 두는 시간입니다.");
            });
        }
        ui.add_space(6.0);
        ui.horizontal_top(|ui| {
            // ------------------------------------------------------------------ settings
            ui.vertical(|ui| {
                ui.set_width(350.0);
                egui::ScrollArea::vertical()
                    .id_salt("sil-settings")
                    .max_height(430.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        settings_panel(ui, &mut form.settings);
                    });
            });
            ui.add_space(12.0);
            // ------------------------------------------------------------------- preview
            ui.vertical(|ui| {
                if let Some(e) = &form.error {
                    ui.label(RichText::new(tn(e)).color(theme::ERROR));
                }
                if busy {
                    let p = s.ed.silencing.as_ref().map(|j| j.progress());
                    if let Some(p) = p {
                        let stage = match p.stage {
                            CaptionStage::Downloading => t("Downloading speech model"),
                            CaptionStage::MixingAudio => t("Preparing audio"),
                            CaptionStage::Analyzing => t("Analyzing audio"),
                            CaptionStage::Transcribing => t("Transcribing"),
                        };
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(stage);
                            ui.add(
                                egui::ProgressBar::new(p.fraction)
                                    .desired_width(160.0)
                                    .text(format!("{:.0} %", p.fraction * 100.0)),
                            );
                            if ui.button(t("Cancel")).clicked()
                                && let Some(j) = &s.ed.silencing
                            {
                                j.cancel();
                            }
                        });
                    }
                }
                match &detection {
                    Some(d) => {
                        preview(ui, form, d, &cuts, &mut toggle);
                        ui.add_space(6.0);
                        cut_list(ui, form, &cuts, &mut toggle);
                    }
                    None if !busy => {
                        ui.add_space(40.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(t(
                                    "Press Analyze to measure the audio. Nothing is cut until you press Apply.",
                                ))
                                .color(theme::TEXT_DIM),
                            )
                            .wrap(),
                        );
                    }
                    None => {}
                }
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            let summary = if detection.is_some() {
                tf(
                    "{} cuts, {} of {} ({} %)",
                    &[
                        &active.len(),
                        &clock(removed),
                        &clock(total),
                        &format!(
                            "{:.0}",
                            if total > 0.0 {
                                removed / total * 100.0
                            } else {
                                0.0
                            }
                        ),
                    ],
                )
            } else {
                String::new()
            };
            ui.label(RichText::new(summary).color(theme::TEXT_DIM));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t("Close")).clicked() {
                    close = true;
                }
                let can = detection.is_some() && !active.is_empty() && !busy && !stale;
                if ui
                    .add_enabled(
                        can,
                        egui::Button::new(RichText::new(t("Apply")).color(egui::Color32::WHITE))
                            .fill(theme::ACCENT_DIM),
                    )
                    .clicked()
                {
                    apply = true;
                }
                let label = if detection.is_some() && stale {
                    t("Analyze Again")
                } else {
                    t("Analyze")
                };
                if ui.add_enabled(!busy, egui::Button::new(label)).clicked() {
                    analyze = true;
                }
            });
        });
    });
    if let Some(k) = toggle {
        if let Some(i) = form.keep.iter().position(|x| (x - k).abs() < 1e-9) {
            form.keep.remove(i);
        } else {
            form.keep.push(k);
        }
    }
    if analyze {
        form.start(s);
    }
    if apply && let Some(d) = &detection {
        let ranges: Vec<sil::Range> = active.iter().map(|c| c.range).collect();
        let action = form.settings.action;
        let n =
            s.ed.apply_cuts(d.origin, &ranges, action, form.settings.smooth_ms);
        s.silence_settings = form.settings.clone();
        match action {
            Action::Remove | Action::LeaveGap => s.ed.info(format!("Removed {n} stretches")),
            Action::SpeedUp(_) => s.ed.info(format!("Sped up {n} stretches")),
            Action::Markers => s.ed.info(format!("Marked {n} stretches")),
        }
        s.ed.silence_result = None;
        return false;
    }
    if close || esc {
        s.silence_settings = form.settings.clone();
        if let Some(j) = &s.ed.silencing {
            j.cancel();
        }
        s.ed.silence_result = None;
        return false;
    }
    true
}

/// The speech model of the words search: the helper's models (up to large-v3, on the graphics
/// card), with a download and a delete button; without the helper the built-in Whisper is used.
fn speech_model_row(s: &mut State, ui: &mut Ui, settings: &mut SilenceSettings) {
    use op_application::whisper_models as wm;
    ui.horizontal_wrapped(|ui| {
        ui.label("음성 인식 모델");
        if op_application::sentences::helper_files().is_none() {
            ui.label(RichText::new("음성 인식 도우미(helper)가 없어 내장 위스퍼(CPU, 최대 small)로 실행합니다").color(theme::WARN));
            return;
        }
        let shown = wm::info(&settings.whisper).map_or(settings.whisper.clone(), |m| m.label.to_string());
        egui::ComboBox::from_id_salt("sil-whisper").selected_text(shown).width(180.0).show_ui(ui, |ui| {
            for m in wm::MODELS {
                let mark = if wm::installed(m.id) { "✔" } else { "내려받기" };
                let text = format!("{}  ({:.1} GB)  {mark}", m.label, f64::from(m.megabytes) / 1000.0);
                if ui.selectable_label(settings.whisper == m.id, text).clicked() {
                    settings.whisper = m.id.to_string();
                }
            }
        });
        let downloading = s.ed.whisper_dl.as_ref().map(|d| (d.id, d.progress()));
        match downloading {
            Some((id, (done, total))) if id == settings.whisper => {
                let f = (done as f32 / total.max(1) as f32).min(1.0);
                ui.add(egui::ProgressBar::new(f).desired_width(120.0).text(format!("{:.0}%", f * 100.0)));
                if ui.small_button("취소").clicked()
                    && let Some(d) = &s.ed.whisper_dl
                {
                    d.cancel();
                }
            }
            _ if wm::installed(&settings.whisper) => {
                ui.label(RichText::new("✔ 설치됨").color(egui::Color32::from_rgb(0x2e, 0x9d, 0x4f)));
                if confirm_delete(ui, ("silence-model", settings.whisper.clone()))
                    && let Some(dir) = wm::dir_of(&settings.whisper)
                    && let Err(e) = s.ed.whisper_remove(&dir)
                {
                    s.ed.error(e);
                }
            }
            _ => {
                if let Some(m) = wm::info(&settings.whisper)
                    && ui
                        .add_enabled(downloading.is_none(), egui::Button::new(format!("내려받기 ({:.1} GB)", f64::from(m.megabytes) / 1000.0)))
                        .clicked()
                    && let Err(e) = s.ed.whisper_download(m.id)
                {
                    s.ed.error(e);
                }
            }
        }
        if let Some(m) = wm::info(&settings.whisper) {
            ui.label(RichText::new(format!("{} · NVIDIA 그래픽 카드(CUDA)로 실행", t(m.note))).size(11.0).color(theme::TEXT_DIM));
        }
    });
}

/// Everything the dialog sets except what to look at: shared with the batch dialog.
pub(super) fn settings_panel(ui: &mut Ui, settings: &mut SilenceSettings) {
    ui.checkbox(&mut settings.by_sound, t("Find Silences in the Sound"))
        .on_hover_text(t("Quiet stretches, measured on the audio."));
    ui.add_enabled_ui(settings.by_sound, |ui| {
        egui::Grid::new("sil-sound")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(t("Method"));
                let cur = settings.sound.method.index();
                egui::ComboBox::from_id_salt("sil-method")
                    .selected_text(t(Method::NAMES[cur]))
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in Method::NAMES.iter().enumerate() {
                            if ui.selectable_label(i == cur, t(name)).clicked() && i != cur {
                                settings.sound.method = Method::default_for(i);
                            }
                        }
                    })
                    .response
                    .on_hover_text(t(method_hint(cur)));
                ui.end_row();
                method_param(ui, &mut settings.sound.method);
                ui.label(t("Shortest Silence"));
                ui.add(
                    egui::Slider::new(&mut settings.sound.min_silence, 0.05..=3.0)
                        .suffix(" s")
                        .logarithmic(true),
                )
                .on_hover_text(t("Shorter pauses stay."));
                ui.end_row();
                ui.label(t("Shortest Sound"));
                ui.add(egui::Slider::new(&mut settings.sound.min_sound, 0.0..=0.5).suffix(" s"))
                    .on_hover_text(t("Shorter sounds (clicks, breaths) count as silence."));
                ui.end_row();
                ui.label(t("Keep Before Cut"));
                let mut ms = settings.sound.pad_before * 1000.0;
                if ui
                    .add(egui::Slider::new(&mut ms, 0.0..=500.0).suffix(" ms"))
                    .on_hover_text(t("Air left after a word, before the cut."))
                    .changed()
                {
                    settings.sound.pad_before = ms / 1000.0;
                }
                ui.end_row();
                ui.label(t("Keep After Cut"));
                let mut ms = settings.sound.pad_after * 1000.0;
                if ui
                    .add(egui::Slider::new(&mut ms, 0.0..=500.0).suffix(" ms"))
                    .on_hover_text(t("Air left before the next word, after the cut."))
                    .changed()
                {
                    settings.sound.pad_after = ms / 1000.0;
                }
                ui.end_row();
                ui.label("");
                ui.checkbox(
                    &mut settings.sound.voice_only,
                    t("Voices Only"),
                )
                .on_hover_text(t(
                    "Music, hum and background noise count as silence (uses the voice detector).",
                ));
                ui.end_row();
            });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.checkbox(&mut settings.by_words, t("Find Words to Cut"))
        .on_hover_text(t(
            "Fillers, repeated words and repeated takes. Transcribes the audio on this computer; the speech model is downloaded once.",
        ));
    ui.add_enabled_ui(settings.by_words, |ui| {
        egui::Grid::new("sil-words")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label(t("Spoken Language"));
                let cur = CAPTION_LANGUAGES
                    .iter()
                    .position(|(c, _)| {
                        Some(c.to_string()).filter(|c| !c.is_empty())
                            == settings.language
                    })
                    .unwrap_or(0);
                egui::ComboBox::from_id_salt("sil-lang")
                    .selected_text(if cur == 0 {
                        t(CAPTION_LANGUAGES[0].1).to_string()
                    } else {
                        CAPTION_LANGUAGES[cur].1.to_string()
                    })
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for (i, (code, name)) in
                            CAPTION_LANGUAGES.iter().enumerate()
                        {
                            let label =
                                if i == 0 { t(name) } else { name };
                            if ui.selectable_label(i == cur, label).clicked() {
                                settings.language = Some(code.to_string())
                                    .filter(|c| !c.is_empty());
                            }
                        }
                    });
                ui.end_row();
                if op_application::sentences::helper_files().is_none() {
                    ui.label(t("Speech Model"));
                    egui::ComboBox::from_id_salt("sil-model")
                        .selected_text(settings.model.label())
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            for m in ModelSize::ALL {
                                ui.selectable_value(
                                    &mut settings.model,
                                    m,
                                    format!("{} ({} MB)", m.label(), m.megabytes()),
                                );
                            }
                        });
                    ui.end_row();
                }
                ui.label("");
                ui.checkbox(&mut settings.words.fillers, t("Filler Words"))
                    .on_hover_text(t("um, uh, eh, euh... in the spoken language."));
                ui.end_row();
                ui.label("");
                ui.add_enabled(
                    settings.words.fillers,
                    egui::Checkbox::new(
                        &mut settings.words.aggressive_fillers,
                        t("Also Crutch Phrases"),
                    ),
                )
                .on_hover_text(t(
                    "\"you know\", \"like\", \"o sea\", \"tipo\", \"du coup\"...",
                ));
                ui.end_row();
                ui.label(t("Other Words"));
                ui.add(
                    egui::TextEdit::singleline(&mut settings.words.custom)
                        .hint_text(t("words or phrases, separated by commas"))
                        .desired_width(190.0),
                );
                ui.end_row();
                ui.label("");
                ui.checkbox(
                    &mut settings.words.repeated_words,
                    t("Repeated Words"),
                )
                .on_hover_text(t("\"I I think\" becomes \"I think\"."));
                ui.end_row();
                ui.label("");
                ui.checkbox(
                    &mut settings.words.repeated_phrases,
                    t("Repeated Phrases"),
                )
                .on_hover_text(t(
                    "\"to the to the store\" becomes \"to the store\" (up to four words).",
                ));
                ui.end_row();
                ui.label("");
                ui.checkbox(
                    &mut settings.words.retakes,
                    t("Repeated Takes (Keep the Last)"),
                )
                .on_hover_text(t(
                    "A sentence said again, nearly the same, is cut and the last time stays.",
                ));
                ui.end_row();
                ui.label(t("Pauses Between Words"));
                ui.add(
                    egui::Slider::new(&mut settings.words.gap, 0.0..=2.0)
                        .suffix(" s")
                        .custom_formatter(|v, _| {
                            if v == 0.0 {
                                t("off").to_string()
                            } else {
                                format!("{v:.2} s")
                            }
                        }),
                )
                .on_hover_text(t(
                    "Cuts the dead air between words that lasts at least this long (0 uses only the sound).",
                ));
                ui.end_row();
                ui.label(t("Match Similarity"));
                let mut pct = settings.words.retake_similarity * 100.0;
                if ui
                    .add(egui::Slider::new(&mut pct, 40.0..=95.0).suffix(" %"))
                    .changed()
                {
                    settings.words.retake_similarity = pct / 100.0;
                }
                ui.end_row();
            });
    });
    ui.add_space(6.0);
    ui.separator();
    egui::Grid::new("sil-action")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            ui.label(t("Then"));
            let cur = settings.action.index();
            egui::ComboBox::from_id_salt("sil-action-combo")
                .selected_text(t(Action::NAMES[cur]))
                .width(190.0)
                .show_ui(ui, |ui| {
                    for (i, name) in Action::NAMES.iter().enumerate() {
                        if ui.selectable_label(i == cur, t(name)).clicked() && i != cur {
                            settings.action = match i {
                                0 => Action::Remove,
                                1 => Action::LeaveGap,
                                2 => Action::SpeedUp(400.0),
                                _ => Action::Markers,
                            };
                        }
                    }
                });
            ui.end_row();
            if let Action::SpeedUp(pct) = &mut settings.action {
                ui.label(t("Speed"));
                ui.add(
                    egui::DragValue::new(pct)
                        .range(110.0..=2000.0)
                        .speed(5.0)
                        .suffix(" %"),
                );
                ui.end_row();
            }
            if matches!(settings.action, Action::Remove) {
                ui.label(t("Audio Crossfade"));
                ui.add(egui::Slider::new(&mut settings.smooth_ms, 0.0..=60.0).suffix(" ms"))
                    .on_hover_text(t(
                        "A short fade where two pieces of audio join, so cuts do not click.",
                    ));
                ui.end_row();
            }
            ui.label("");
            ui.checkbox(&mut settings.snap, t("Cut at the Quietest Moment"))
                .on_hover_text(t("Word cuts move to the quietest moment next to them."));
            ui.end_row();
        });
}

fn method_hint(i: usize) -> &'static str {
    match i {
        0 => "Anything quieter than a fixed level is silence. Simple and predictable.",
        1 => {
            "Measured from the loudest part of the recording, so it works for quiet and loud recordings alike."
        }
        2 => {
            "Finds the recording's own background noise and cuts what is barely above it. Best default."
        }
        3 => {
            "Looks for the sound of a voice (the speech frequencies, not steady noise): keeps quiet words, drops hum and hiss."
        }
        _ => "A small neural network trained to hear voices: also ignores music and room noise.",
    }
}

/// The waveform with the stretches to cut shaded over it. Click a stretch to keep it.
fn preview(
    ui: &mut Ui,
    form: &mut SilenceForm,
    d: &Detection,
    cuts: &[sil::Cut],
    toggle: &mut Option<f64>,
) {
    let avail = ui.available_width();
    let dur = d.analysis.duration().max(0.1);
    let fit = (avail / dur as f32).max(1.0);
    let pps = if form.zoom <= 0.0 { fit } else { form.zoom };
    ui.horizontal(|ui| {
        ui.label(RichText::new(t("Zoom")).size(11.0).color(theme::TEXT_DIM));
        let mut z = pps;
        if ui
            .add(
                egui::Slider::new(&mut z, fit..=(fit.max(400.0)))
                    .logarithmic(true)
                    .show_value(false),
            )
            .changed()
        {
            form.zoom = if (z - fit).abs() < 0.5 { 0.0 } else { z };
        }
        ui.label(
            RichText::new(t("Click a colored stretch to keep it."))
                .size(11.0)
                .color(theme::TEXT_DIM),
        );
    });
    let width = (dur as f32 * pps).max(avail);
    egui::ScrollArea::horizontal()
        .id_salt("sil-preview")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let (resp, painter) =
                ui.allocate_painter(egui::vec2(width, 150.0), egui::Sense::click());
            let r = resp.rect;
            painter.rect_filled(r, 3.0, theme::PANEL_DARK);
            let x_of = |t: f64| r.min.x + (t as f32 * pps);
            // the stretches under the waveform
            for c in cuts {
                let excluded = form.excluded(c);
                let col = kind_color(c.kind);
                let rect = egui::Rect::from_min_max(
                    egui::pos2(x_of(c.range.start), r.min.y),
                    egui::pos2(x_of(c.range.end).max(x_of(c.range.start) + 1.0), r.max.y),
                );
                painter.rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(
                        col.r(),
                        col.g(),
                        col.b(),
                        if excluded { 22 } else { 85 },
                    ),
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0, if excluded { theme::LINE } else { col }),
                    egui::StrokeKind::Inside,
                );
            }
            // loudness, one bar per pixel
            let mid = r.center().y;
            let scale = (r.height() / 2.0 - 4.0) / 60.0;
            let step = ((1.0 / pps as f64) / sil::HOP).max(1.0) as usize;
            let mut i = 0;
            while i < d.analysis.db.len() {
                let end = (i + step).min(d.analysis.db.len());
                let level = d.analysis.db[i..end]
                    .iter()
                    .cloned()
                    .fold(-100.0f32, f32::max);
                let h = ((level + 60.0).max(0.0)) * scale;
                let x = x_of(i as f64 * sil::HOP);
                painter.line_segment(
                    [egui::pos2(x, mid - h), egui::pos2(x, mid + h)],
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 190, 160)),
                );
                i += step;
            }
            // the level the method draws its line at
            if form.settings.by_sound {
                let line = match form.settings.sound.method {
                    Method::Loudness { threshold_db } => Some(threshold_db),
                    Method::Relative { below_peak_db } => {
                        Some(d.analysis.peak_db() - below_peak_db)
                    }
                    Method::Adaptive { margin_db } => Some(d.analysis.noise_floor_db() + margin_db),
                    _ => None,
                };
                if let Some(db) = line {
                    let h = (db + 60.0).max(0.0) * scale;
                    for y in [mid - h, mid + h] {
                        painter.line_segment(
                            [egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)],
                            egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 110),
                            ),
                        );
                    }
                }
            }
            // words, when there is a transcript
            if form.settings.by_words
                && let Some(words) = &d.words
            {
                for w in words.iter() {
                    let (a, b) = (x_of(w.start), x_of(w.end));
                    if b - a > 14.0 {
                        painter.text(
                            egui::pos2(a + 1.0, r.max.y - 12.0),
                            egui::Align2::LEFT_TOP,
                            w.text.trim(),
                            egui::FontId::proportional(10.0),
                            theme::TEXT_DIM,
                        );
                    }
                }
            }
            // ruler
            let tick = if pps > 150.0 {
                1.0
            } else if pps > 40.0 {
                5.0
            } else if pps > 12.0 {
                15.0
            } else {
                60.0
            };
            let mut tt = 0.0;
            while tt < dur {
                let x = x_of(tt);
                painter.line_segment(
                    [egui::pos2(x, r.min.y), egui::pos2(x, r.min.y + 6.0)],
                    egui::Stroke::new(1.0, theme::TEXT_DIM),
                );
                painter.text(
                    egui::pos2(x + 2.0, r.min.y + 1.0),
                    egui::Align2::LEFT_TOP,
                    clock(tt),
                    egui::FontId::monospace(9.0),
                    theme::TEXT_DIM,
                );
                tt += tick;
            }
            if let Some(p) = resp.hover_pos() {
                let at = ((p.x - r.min.x) / pps) as f64;
                if let Some(c) = cuts
                    .iter()
                    .find(|c| c.range.start <= at && at <= c.range.end)
                {
                    let text = if c.text.is_empty() {
                        format!("{} {}", t(c.kind.label()), clock(c.range.len()))
                    } else {
                        format!("{}: {}", t(c.kind.label()), c.text)
                    };
                    resp.clone().on_hover_text(text);
                    if resp.clicked() {
                        *toggle = Some((c.range.start + c.range.end) / 2.0);
                    }
                }
            }
        });
}

/// The stretches as a list, each with a checkbox.
fn cut_list(ui: &mut Ui, form: &mut SilenceForm, cuts: &[sil::Cut], toggle: &mut Option<f64>) {
    egui::ScrollArea::vertical()
        .id_salt("sil-list")
        .max_height(230.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if cuts.is_empty() {
                ui.label(
                    RichText::new(t("Nothing to cut with these settings.")).color(theme::TEXT_DIM),
                );
            }
            for c in cuts {
                let mid = (c.range.start + c.range.end) / 2.0;
                ui.horizontal(|ui| {
                    let mut on = !form.excluded(c);
                    if ui.checkbox(&mut on, "").changed() {
                        *toggle = Some(mid);
                    }
                    ui.label(
                        RichText::new(format!(
                            "{}  {}",
                            clock(c.range.start),
                            clock(c.range.len())
                        ))
                        .monospace()
                        .size(11.0)
                        .color(theme::TEXT_DIM),
                    );
                    ui.label(
                        RichText::new(t(c.kind.label()))
                            .size(11.0)
                            .color(kind_color(c.kind)),
                    );
                    if !c.text.is_empty() {
                        ui.label(RichText::new(&c.text).size(11.5));
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(words: bool, neural: bool, model: ModelSize) -> Key {
        Key {
            scope: Scope::Sequence,
            neural,
            words,
            engine: model.id().to_string(),
            language: None,
        }
    }

    #[test]
    fn an_analysis_is_kept_unless_it_lacks_something_the_settings_need() {
        let with_words = key(true, false, ModelSize::Tiny);
        // turning the word search off keeps it
        assert!(with_words.covers(&key(false, false, ModelSize::Tiny)));
        // so does a model nobody uses without words
        assert!(with_words.covers(&key(false, false, ModelSize::Base)));
        // words that were never transcribed, another model or a neural curve need a new one
        assert!(!key(false, false, ModelSize::Tiny).covers(&with_words));
        assert!(!with_words.covers(&key(true, false, ModelSize::Base)));
        assert!(!with_words.covers(&key(true, true, ModelSize::Tiny)));
        assert!(key(true, true, ModelSize::Tiny).covers(&with_words));
    }
}
