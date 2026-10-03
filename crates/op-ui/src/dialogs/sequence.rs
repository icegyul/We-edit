//! New Sequence and Sequence Settings: presets and the settings form.

use super::*;

pub(super) struct Preset {
    name: &'static str,
    w: u32,
    h: u32,
    rate: Rate,
}

pub(super) const SEQ_PRESETS: &[Preset] = &[
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

pub(super) fn sequence_form(ui: &mut Ui, st: &mut SequenceSettings, new: bool) {
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
