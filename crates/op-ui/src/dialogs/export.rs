//! Export Media: presets and the export window.

use super::*;

pub(super) struct ExportPreset {
    name: &'static str,
    codec: VideoCodec,
    /// Target bitrate in Mbps; 0 uses constant quality.
    mbps: u32,
    height: Option<u32>,
    audio: AudioCodec,
    video: bool,
}

pub(super) const EXPORT_PRESETS: &[ExportPreset] = &[
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

pub(super) fn audio_extension(c: AudioCodec) -> &'static str {
    match c {
        AudioCodec::Mp3 => "mp3",
        AudioCodec::Aac => "m4a",
        _ => "wav",
    }
}

/// The user's videos folder (Videos on Windows, Movies on macOS, the XDG folder on Linux in the
/// user's language), else the home folder.
pub(super) fn dirs_video() -> Option<PathBuf> {
    dirs::video_dir()
        .filter(|v| v.is_dir())
        .or_else(dirs::home_dir)
}

pub(super) fn apply_preset(f: &mut ExportForm, i: usize) {
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

pub(super) fn export_dialog(s: &mut State, ctx: &egui::Context, f: &mut ExportForm) -> bool {
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
