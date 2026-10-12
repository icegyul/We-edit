//! The transcript panel (Window > 문장 편집): what the sequence says, word by word, written down
//! by Whisper. Click a word to go there, shift-click to select a stretch, then cut it out of the
//! timeline (every track, gap closed, one undo step) or make captions from it.

use egui::{Color32, RichText, Ui};
use op_application::Monitor;
use op_application::captions::CaptionStage;
use op_application::sentences::helper_files;
use op_core::catalog::CAPTION_STYLES;
use op_core::*;

use crate::app::State;
use crate::i18n::t;
use crate::theme;

/// Spoken languages offered (code for the helper, name shown).
const LANGUAGES: [(&str, &str); 6] = [
    ("", "자동 감지"),
    ("ko", "한국어"),
    ("en", "English"),
    ("ja", "日本語"),
    ("zh", "中文"),
    ("es", "Español"),
];

#[derive(Default)]
pub struct SentencesView {
    language: usize,
    /// First and last selected word, counted over the whole transcript.
    sel: Option<(usize, usize)>,
    anchor: Option<usize>,
}

fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0);
    format!("{:02}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

pub fn show(s: &mut State, ui: &mut Ui) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| body(s, ui));
}

fn body(s: &mut State, ui: &mut Ui) {
    let running = s.ed.sentencing.as_ref().map(|j| j.progress());
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("sentence-language")
            .selected_text(LANGUAGES[s.sentences.language].1)
            .width(100.0)
            .show_ui(ui, |ui| {
                for (i, (_, name)) in LANGUAGES.iter().enumerate() {
                    ui.selectable_value(&mut s.sentences.language, i, *name);
                }
            });
        let can = running.is_none() && s.ed.active.is_some();
        if ui.add_enabled(can, egui::Button::new("받아쓰기")).clicked() {
            start(s, false);
        }
        if ui
            .add_enabled(can, egui::Button::new("받아쓰고 자막 만들기"))
            .on_hover_text("Whisper가 받아쓴 단어 시간으로 자막을 만듭니다")
            .clicked()
        {
            start(s, true);
        }
        if running.is_some() && ui.button("취소").clicked()
            && let Some(job) = &s.ed.sentencing
        {
            job.cancel();
        }
    });
    if helper_files().is_none() {
        ui.label(RichText::new("받아쓰기 도우미(helper 폴더)를 찾을 수 없습니다.").color(theme::WARN));
    }
    if let Some(p) = running {
        let stage = match p.stage {
            CaptionStage::Transcribing | CaptionStage::Analyzing => "받아쓰는 중 (Whisper)",
            _ => "소리를 준비하는 중",
        };
        ui.add(egui::ProgressBar::new(p.fraction).text(stage));
        return;
    }
    ui.separator();
    if s.ed.active.is_none() {
        ui.label(RichText::new("먼저 시퀀스를 여세요").color(theme::TEXT_DIM));
        return;
    }
    let Some(tr) = s.ed.transcript.clone() else {
        ui.add(
            egui::Label::new(
                RichText::new(
                    "아직 받아쓴 내용이 없습니다. '받아쓰기'를 누르면 시퀀스의 말이 단어별로 나타납니다. 단어를 눌러 그 위치로 가고, Shift+클릭으로 구간을 골라 삭제하거나 자막으로 만들 수 있습니다.",
                )
                .color(theme::TEXT_DIM),
            )
            .wrap(),
        );
        return;
    };
    let current = s.ed.transcript_is_current();
    let flat: Vec<(f64, f64)> = tr.sentences.iter().flat_map(|x| x.words.iter().map(|w| (w.start, w.end))).collect();
    if s.sentences.sel.is_some_and(|(_, b)| b >= flat.len()) {
        s.sentences.sel = None;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("문장 {}개 · 단어 {}개 · 언어 {}", tr.sentences.len(), flat.len(), tr.language))
                .color(theme::TEXT_DIM),
        );
        if !current {
            ui.label(RichText::new("⚠ 받아쓴 뒤 시퀀스가 바뀌었습니다. 다시 받아쓰세요.").color(theme::WARN));
        }
    });
    // captions from the words, in the chosen design
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("자막").color(theme::TEXT_DIM));
        let style = (s.caption_options.style as usize).min(CAPTION_STYLES.len() - 1);
        egui::ComboBox::from_id_salt("sentence-caption-style")
            .selected_text(t(CAPTION_STYLES[style]))
            .width(120.0)
            .show_ui(ui, |ui| {
                for (i, name) in CAPTION_STYLES.iter().enumerate() {
                    if ui.selectable_label(i == style, t(name)).clicked() {
                        s.caption_options.style = i as u32;
                    }
                }
            });
        ui.add(egui::DragValue::new(&mut s.caption_options.max_words).range(1..=12).suffix(" 단어"));
        let all = ui.add_enabled(current, egui::Button::new("전체 자막 만들기")).clicked();
        let part = ui
            .add_enabled(current && s.sentences.sel.is_some(), egui::Button::new("선택한 부분 자막"))
            .clicked();
        if all || part {
            let range = if part { s.sentences.sel.map(|(a, b)| (flat[a].0, flat[b].1)) } else { None };
            let opts = s.caption_options.clone();
            match s.ed.captions_from_transcript(&opts, range) {
                Ok(n) => s.ed.info(format!("자막 {n}개를 만들었습니다")),
                Err(e) => s.ed.error(e),
            }
        }
    });
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        let sel = s.sentences.sel;
        ui.horizontal(|ui| {
            let label = match sel {
                Some((a, b)) => format!("선택한 단어 삭제 ({:.1}초)", flat[b].1 - flat[a].0),
                None => "선택한 단어 삭제".to_string(),
            };
            if ui.add_enabled(sel.is_some() && current, egui::Button::new(label)).clicked()
                && let Some((a, b)) = sel
            {
                match s.ed.delete_ranges(&[(flat[a].0, flat[b].1)]) {
                    Ok(_) => {
                        s.sentences.sel = None;
                        s.sentences.anchor = None;
                    }
                    Err(e) => s.ed.error(e),
                }
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("선택 해제")).clicked() {
                s.sentences.sel = None;
                s.sentences.anchor = None;
            }
        });
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // the outer layout stacks upward: the text reads downward
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                let shift = ui.input(|i| i.modifiers.shift);
                let mut index = 0usize;
                for sentence in &tr.sentences {
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(clock(sentence.start)).monospace().color(theme::VALUE));
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            for w in &sentence.words {
                                let on = s.sentences.sel.is_some_and(|(a, b)| index >= a && index <= b);
                                let mut text = RichText::new(&w.text).color(if on { Color32::WHITE } else { theme::TEXT });
                                if on {
                                    text = text.background_color(theme::ACCENT_DIM);
                                }
                                let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click()));
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    match s.sentences.anchor {
                                        Some(a) if shift => s.sentences.sel = Some((a.min(index), a.max(index))),
                                        _ => {
                                            s.sentences.anchor = Some(index);
                                            s.sentences.sel = Some((index, index));
                                        }
                                    }
                                    s.ed.set_monitor_time(Monitor::Program, SeqTime::ZERO + Dur::from_seconds(w.start));
                                }
                                index += 1;
                            }
                        });
                    });
                    ui.add_space(3.0);
                }
            });
        });
    });
}

/// Starts the transcription; with `captions` the captions follow as soon as it is ready.
fn start(s: &mut State, captions: bool) {
    s.sentences.sel = None;
    s.sentences.anchor = None;
    s.ed.captions_after = captions.then(|| s.caption_options.clone());
    if let Err(e) = s.ed.transcribe_sentences(LANGUAGES[s.sentences.language].0) {
        s.ed.captions_after = None;
        s.ed.error(e);
    }
}
