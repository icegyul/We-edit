//! Export Media: presets and the export window.

use super::*;

pub(super) struct ExportPreset {
    name: &'static str,
    codec: VideoCodec,
    /// Target bitrate in Mbps; 0 uses constant quality.
    mbps: u32,
    height: Option<u32>,
    /// Exact width for presets with a fixed frame (with `height`); else the width follows the sequence's aspect.
    width: Option<u32>,
    audio: AudioCodec,
    video: bool,
    /// YouTube's recommendations: the bitrate by frame size and rate, audio AAC 384 kbps, 48 kHz.
    youtube: bool,
}

pub(super) const EXPORT_PRESETS: &[ExportPreset] = &[
    ExportPreset {
        name: "Match Source - High Quality (H.264)",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "YouTube 1080p (H.264)",
        codec: VideoCodec::H264,
        mbps: 16,
        height: Some(1080),
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: true,
    },
    ExportPreset {
        name: "YouTube 2160p 4K (H.264)",
        codec: VideoCodec::H264,
        mbps: 45,
        height: Some(2160),
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: true,
    },
    ExportPreset {
        name: "YouTube 1440p 2K (H.264)",
        codec: VideoCodec::H264,
        mbps: 24,
        height: Some(1440),
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: true,
    },
    ExportPreset {
        name: "YouTube 720p (H.264)",
        codec: VideoCodec::H264,
        mbps: 8,
        height: Some(720),
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: true,
    },
    ExportPreset {
        name: "YouTube Shorts / Reels 1080x1920 (H.264)",
        codec: VideoCodec::H264,
        mbps: 12,
        height: Some(1920),
        width: Some(1080),
        audio: AudioCodec::Aac,
        video: true,
        youtube: true,
    },
    ExportPreset {
        name: "Social Media 1080p (H.264)",
        codec: VideoCodec::H264,
        mbps: 10,
        height: Some(1080),
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Match Source - High Quality (HEVC)",
        codec: VideoCodec::Hevc,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Aac,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 422 Proxy",
        codec: VideoCodec::ProResProxy,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 422 LT",
        codec: VideoCodec::ProResLt,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 422",
        codec: VideoCodec::ProRes422,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 422 HQ",
        codec: VideoCodec::ProRes422Hq,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 4444 (with alpha)",
        codec: VideoCodec::ProRes4444,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Apple ProRes 4444 XQ (with alpha)",
        codec: VideoCodec::ProRes4444Xq,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "DNxHR HQ",
        codec: VideoCodec::DnxhrHq,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Sony XDCAM HD422 50 (MXF, compatible)",
        codec: VideoCodec::XdcamHd422,
        mbps: 0,
        height: Some(1080),
        width: Some(1920),
        audio: AudioCodec::Pcm24,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "PNG (QuickTime, with alpha)",
        codec: VideoCodec::Png,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm16,
        video: true,
        youtube: false,
    },
    ExportPreset {
        name: "Audio only - WAV",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Pcm24,
        video: false,
        youtube: false,
    },
    ExportPreset {
        name: "Audio only - MP3",
        codec: VideoCodec::H264,
        mbps: 0,
        height: None,
        width: None,
        audio: AudioCodec::Mp3,
        video: false,
        youtube: false,
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

/// YouTube's recommended video bitrate (Mbps, SDR) for a picture of `short` pixels on its
/// shorter side at `fps` frames per second: 4K 35-45 (53-68 above 30 fps), 1440p 16 (24),
/// 1080p 8 (12), 720p 5 (7.5), smaller 2.5 (4).
pub(super) fn youtube_mbps(short: u32, fps: f64) -> u32 {
    let fast = fps > 30.5;
    match short {
        2000.. => if fast { 68 } else { 45 },
        1300.. => if fast { 24 } else { 16 },
        1000.. => if fast { 12 } else { 8 },
        650.. => if fast { 8 } else { 5 },
        _ => if fast { 4 } else { 3 },
    }
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
    f.youtube = p.youtube;
    f.auto_bitrate = p.youtube;
    if p.youtube {
        // what YouTube recommends for the sound: AAC at 384 kbps, 48 kHz, stereo
        f.audio_bitrate = 384;
        f.audio_rate = 48000;
        f.channels = 2;
    }
    let (sw, sh) = f.seq_size;
    match p.height {
        Some(h) if p.width.is_some() => {
            f.height = h;
            f.width = p.width.unwrap_or(sw);
        }
        Some(h) if sh > 0 => {
            f.height = h;
            f.width = ((sw as f64 * h as f64 / sh as f64).round() as u32) & !1;
        }
        _ => {
            f.width = sw;
            f.height = sh;
        }
    }
    if f.youtube {
        f.bitrate = youtube_mbps(f.width.min(f.height), f.rate.as_f64());
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
    let mut queue = false;
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
                                ui.horizontal(|ui| {
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut f.bitrate)
                                                .range(1..=800)
                                                .suffix(" Mbps"),
                                        )
                                        .changed()
                                    {
                                        f.auto_bitrate = false;
                                    }
                                    if f.youtube {
                                        ui.label(
                                            RichText::new(if f.auto_bitrate {
                                                "YouTube 권장값(프레임 크기와 속도에 맞춤) · 숫자를 바꾸면 직접 정한 값"
                                            } else {
                                                "직접 정한 값"
                                            })
                                            .size(11.0)
                                            .color(theme::TEXT_DIM),
                                        );
                                        if !f.auto_bitrate && ui.small_button("권장값").clicked() {
                                            f.auto_bitrate = true;
                                        }
                                    }
                                });
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
                            ui.label("");
                            ui.checkbox(&mut f.preview, "내보내는 동안 화면 미리보기 (끄면 더 빠릅니다)");
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
                                    for b in [128u32, 160, 192, 256, 320, 384, 512] {
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
                    if ui
                        .add_enabled(valid, egui::Button::new("대기열에 추가"))
                        .on_hover_text("지금 시작하지 않고 내보내기 대기열에 쌓아 두었다가 한꺼번에 내보냅니다 (지금의 편집 상태로 저장됩니다)")
                        .clicked()
                    {
                        queue = true;
                    }
                });
            });
        });
    if f.youtube && f.auto_bitrate {
        f.bitrate = youtube_mbps(f.width.min(f.height), f.rate.as_f64());
    }
    if start || queue {
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
        s.ed.prefs.export_preview = f.preview;
        let options = ExportOptions {
            preview: f.preview,
            tuning: if f.youtube && matches!(f.codec, VideoCodec::H264 | VideoCodec::Hevc) {
                Tuning::YOUTUBE
            } else {
                Tuning::DELIVERY
            },
        };
        let title = s.ed.project.sequence(f.sequence).map(|q| q.name.clone()).unwrap_or_default();
        let snap = s.ed.snapshot();
        if queue {
            s.ed.export_queue.add(title, t(EXPORT_PRESETS[f.preset].name).to_string(), snap, settings, options);
            s.ed.info("내보내기 대기열에 추가했습니다: 내보내기 대기열 패널에서 시작하세요");
            s.command("uif.window.Export Queue");
            return false;
        }
        let job = ExportJob::start_with(snap, settings, options, s.gpu.clone(), s.ed.media.clone());
        s.ed.exports.push(job);
        s.ed.info(tf("Exporting {}", &[&path.display()]));
        return false;
    }
    open
}

// ------------------------------------------------------------------------ keyboard shortcuts

#[cfg(test)]
mod tests {
    use super::*;

    fn form(w: u32, h: u32, rate: Rate) -> ExportForm {
        ExportForm {
            sequence: SequenceId(1),
            path: PathBuf::from("x.mp4"),
            preset: 0,
            video: true,
            codec: VideoCodec::H264,
            width: w,
            height: h,
            rate,
            use_bitrate: false,
            bitrate: 20,
            quality: 18,
            hardware: true,
            audio: true,
            audio_codec: AudioCodec::Aac,
            audio_rate: 44100,
            channels: 1,
            audio_bitrate: 192,
            youtube: false,
            auto_bitrate: false,
            preview: false,
            in_out: false,
            has_in_out: false,
            seq_size: (w, h),
            seq_rate: rate,
        }
    }

    fn preset(name: &str) -> usize {
        EXPORT_PRESETS.iter().position(|p| p.name == name).unwrap()
    }

    #[test]
    fn the_youtube_presets_set_what_youtube_recommends() {
        // 4K at 29.97 fps: 45 Mbps, sound AAC 384 kbps 48 kHz stereo
        let mut f = form(3840, 2160, Rate::new(30000, 1001));
        apply_preset(&mut f, preset("YouTube 2160p 4K (H.264)"));
        assert!(f.use_bitrate && f.youtube && f.auto_bitrate);
        assert_eq!((f.width, f.height, f.bitrate), (3840, 2160, 45));
        assert_eq!((f.audio_bitrate, f.audio_rate, f.channels), (384, 48000, 2));
        assert_eq!(f.audio_codec, AudioCodec::Aac);
        // the table by size and rate
        assert_eq!(youtube_mbps(2160, 60.0), 68);
        assert_eq!(youtube_mbps(1440, 30.0), 16);
        assert_eq!(youtube_mbps(1080, 25.0), 8);
        assert_eq!(youtube_mbps(1080, 59.94), 12);
        assert_eq!(youtube_mbps(720, 30.0), 5);
        // 1080p of a 4K sequence
        let mut f = form(3840, 2160, Rate::FPS_25);
        apply_preset(&mut f, preset("YouTube 1080p (H.264)"));
        assert_eq!((f.width, f.height, f.bitrate), (1920, 1080, 8));
        // shorts: a vertical 1080 picture counts as 1080p
        let mut f = form(1920, 1080, Rate::FPS_25);
        apply_preset(&mut f, preset("YouTube Shorts / Reels 1080x1920 (H.264)"));
        assert_eq!((f.width, f.height, f.bitrate), (1080, 1920, 8));
        // a preset that is not YouTube's leaves the sound alone
        let mut f = form(1920, 1080, Rate::FPS_25);
        apply_preset(&mut f, preset("Social Media 1080p (H.264)"));
        assert!(!f.youtube && f.audio_bitrate == 192);
    }
}
