//! Edit > Preferences, and the log window it opens.

use super::*;

pub(super) fn level_name(i: usize) -> &'static str {
    match i {
        0 => t("Errors only"),
        1 => t("Warnings and errors"),
        2 => t("Normal"),
        3 => t("Detailed"),
        _ => t("Everything (very detailed)"),
    }
}

pub(super) fn preferences(s: &mut State, ctx: &egui::Context, scale: &mut f32) -> bool {
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
                        RichText::new(t("AI Assistants"))
                            .strong()
                            .family(crate::fonts::strong())
                            .color(theme::TEXT_BRIGHT),
                    );
                    ui.end_row();
                    ui.label(t("Assistant Control"));
                    ui.checkbox(
                        &mut p.assistant_control,
                        t("Let AI assistants edit the open project"),
                    )
                    .on_hover_text(t(
                        "Claude Code, Codex and other assistants that use the Model Context Protocol can then import, cut, add effects, titles and captions, look at frames and export in this window. Every change is one undo step.",
                    ));
                    ui.end_row();
                    ui.label(t("Connect an Assistant"));
                    ui.vertical(|ui| {
                        let exe = std::env::current_exe()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|_| "OpenPremier".into());
                        for (name, cmd) in [
                            ("Claude Code", format!("claude mcp add openpremier -- \"{exe}\" --mcp")),
                            ("Codex", format!("codex mcp add openpremier -- \"{exe}\" --mcp")),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(name).size(11.5).color(theme::TEXT_DIM));
                                if ui.small_button(t("Copy Command")).on_hover_text(&cmd).clicked() {
                                    ui.ctx().copy_text(cmd.clone());
                                }
                            });
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(tf(
                                    "Other assistants: run \"{}\" with the argument --mcp.",
                                    &[&exe],
                                ))
                                .size(11.0)
                                .color(theme::TEXT_DIM),
                            )
                            .wrap(),
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
pub(super) fn log_window(ctx: &egui::Context, level: &mut usize, filter: &mut String) -> bool {
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

pub(super) fn res_label(d: u32) -> String {
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
