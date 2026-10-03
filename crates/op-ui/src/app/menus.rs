//! The menu bar's menus.

use super::*;

pub(crate) fn item(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str) {
    item_if(ui, s, label, cmd, true);
}

pub(crate) fn item_if(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str, enabled: bool) {
    let keys = s.ed.keymap.keys_for(cmd).unwrap_or_default();
    if ui
        .add_enabled(enabled, egui::Button::new(t(label)).shortcut_text(keys))
        .clicked()
    {
        ui.close();
        s.command(cmd);
    }
}

pub(crate) fn check(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str, on: bool) {
    let keys = s.ed.keymap.keys_for(cmd).unwrap_or_default();
    let text = if on {
        format!("\u{2714} {}", t(label))
    } else {
        format!("     {}", t(label))
    };
    if ui
        .add(egui::Button::new(text).shortcut_text(keys))
        .clicked()
    {
        ui.close();
        s.command(cmd);
    }
}

pub(crate) fn file_menu(ui: &mut Ui, s: &mut State) {
    ui.menu_button(t("New"), |ui| {
        item(ui, s, "Project...", "cmd.file.new.project");
        item(ui, s, "Sequence...", "cmd.file.new.sequence");
        item(ui, s, "Bin", "cmd.file.new.bin");
        ui.separator();
        item(ui, s, "Color Matte...", "op.new.colormatte");
        item(ui, s, "Black Video", "op.new.blackvideo");
        item(ui, s, "Transparent Video", "op.new.transparentvideo");
        item(ui, s, "Bars and Tone", "op.new.barsandtone");
        item(ui, s, "Adjustment Layer", "op.new.adjustmentlayer");
    });
    item(ui, s, "Open Project...", "cmd.file.openproject");
    ui.menu_button(t("Open Recent"), |ui| {
        let recent = s.ed.prefs.recent.clone();
        if recent.is_empty() {
            ui.label(RichText::new(t("No recent projects")).color(theme::TEXT_DIM));
        }
        for p in recent {
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if ui
                .button(name)
                .on_hover_text(p.display().to_string())
                .clicked()
            {
                ui.close();
                s.confirm(Then::Open(Some(p)));
            }
        }
    });
    item(ui, s, "Close Project", "cmd.file.close");
    ui.separator();
    item(ui, s, "Save", "cmd.file.save");
    item(ui, s, "Save As...", "cmd.file.saveas");
    item(ui, s, "Save a Copy...", "cmd.file.savecopy");
    ui.separator();
    item(ui, s, "Import...", "cmd.file.import");
    item(ui, s, "Link Media...", "op.file.linkmedia");
    ui.menu_button(t("Export"), |ui| {
        let has = s.ed.active.is_some();
        item_if(ui, s, "Media...", "cmd.file.export.movie", has);
        item_if(ui, s, "Frame...", "cmd.export.frame", has);
        ui.separator();
        item_if(ui, s, "OpenTimelineIO...", "op.file.export.otio", has);
        item_if(ui, s, "Final Cut Pro XML...", "op.file.export.fcpxml", has);
        item_if(ui, s, "EDL...", "op.file.export.edl", has);
    });
    ui.separator();
    item(ui, s, "Project Settings...", "op.project.settings");
    ui.separator();
    item(ui, s, "Exit", "cmd.file.exit");
}

pub(crate) fn edit_menu(ui: &mut Ui, s: &mut State) {
    let undo =
        s.ed.history
            .undo_label()
            .map(|l| tf("Undo {}", &[&tn(l)]))
            .unwrap_or_else(|| t("Undo").to_string());
    let redo =
        s.ed.history
            .redo_label()
            .map(|l| tf("Redo {}", &[&tn(l)]))
            .unwrap_or_else(|| t("Redo").to_string());
    let k = s.ed.keymap.keys_for("cmd.edit.undo").unwrap_or_default();
    if ui
        .add_enabled(
            s.ed.history.can_undo(),
            egui::Button::new(undo).shortcut_text(k),
        )
        .clicked()
    {
        ui.close();
        s.command("cmd.edit.undo");
    }
    let k = s.ed.keymap.keys_for("cmd.edit.redo").unwrap_or_default();
    if ui
        .add_enabled(
            s.ed.history.can_redo(),
            egui::Button::new(redo).shortcut_text(k),
        )
        .clicked()
    {
        ui.close();
        s.command("cmd.edit.redo");
    }
    ui.separator();
    item(ui, s, "Cut", "cmd.edit.cut");
    item(ui, s, "Copy", "cmd.edit.copy");
    item(ui, s, "Paste", "cmd.edit.paste");
    item(ui, s, "Paste Insert", "cmd.edit.pasteinsert");
    item(ui, s, "Paste Attributes...", "cmd.edit.pasteattributes");
    item(ui, s, "Remove Attributes...", "cmd.edit.removeattributes");
    ui.separator();
    item(ui, s, "Clear", "cmd.edit.clear");
    item(ui, s, "Ripple Delete", "cmd.edit.rippledelete");
    item(ui, s, "Duplicate", "cmd.edit.duplicate");
    item(ui, s, "Select All", "cmd.edit.selectall");
    item(ui, s, "Deselect All", "cmd.edit.deselectall");
    ui.separator();
    item(ui, s, "Keyboard Shortcuts...", "cmd.edit.keyboardshortcuts");
    item(ui, s, "Preferences...", "op.edit.preferences");
}

pub(crate) fn clip_menu(ui: &mut Ui, s: &mut State) {
    let has = !s.ed.selection.clips.is_empty();
    item(ui, s, "Rename...", "op.clip.rename");
    item(ui, s, "Make Subclip", "cmd.clip.makesubclip");
    ui.separator();
    item_if(ui, s, "Speed/Duration...", "cmd.clip.speed", has);
    item_if(ui, s, "Audio Gain...", "cmd.clip.audiooptions.gain", has);
    ui.separator();
    item(ui, s, "Insert", "cmd.clip.insert");
    item(ui, s, "Overwrite", "cmd.clip.overlay");
    ui.separator();
    item_if(ui, s, "Enable", "cmd.clip.enable", has);
    item_if(ui, s, "Link / Unlink", "cmd.clip.linkaudioandvideo", has);
    item_if(ui, s, "Group", "cmd.clip.group", has);
    item_if(ui, s, "Ungroup", "cmd.clip.ungroup", has);
    ui.separator();
    item_if(ui, s, "Nest...", "op.clip.nest", has);
    item_if(ui, s, "Add Frame Hold", "op.clip.framehold", has);
    item_if(ui, s, "Scale to Frame Size", "op.clip.scaletoframe", has);
    ui.separator();
    proxy_menu(ui, s);
}

/// Graphics > Captions.
pub(crate) fn captions_menu(ui: &mut Ui, s: &mut State) {
    let has_seq = s.ed.active.is_some();
    let busy = s.ed.captioning.is_some();
    item_if(
        ui,
        s,
        "Transcribe and Create Captions...",
        "op.captions.transcribe",
        has_seq && !busy,
    );
    item_if(ui, s, "New Caption", "op.captions.new", has_seq);
    ui.separator();
    item_if(
        ui,
        s,
        "Import Captions File...",
        "op.captions.import",
        has_seq,
    );
    item_if(
        ui,
        s,
        "Export Captions File...",
        "op.captions.export",
        has_seq,
    );
    ui.separator();
    let caption_selected = s.ed.active_seq().is_some_and(|q| {
        q.video.iter().flat_map(|t| &t.clips).any(|c| {
            s.ed.selection.clips.contains(&c.id) && c.component(catalog::CAPTION).is_some()
        })
    });
    item_if(
        ui,
        s,
        "Apply Caption Style to All",
        "op.captions.apply_style",
        caption_selected,
    );
}

/// Clip > Proxy and the Project panel's Proxy submenu.
pub fn proxy_menu(ui: &mut Ui, s: &mut State) {
    let assets = s.selected_assets();
    let videos = assets
        .iter()
        .filter(|a| {
            s.ed.project
                .asset(**a)
                .is_some_and(|m| m.has_video() && !m.is_still())
        })
        .count();
    let attached = assets
        .iter()
        .filter(|a| s.ed.project.asset(**a).is_some_and(|m| m.proxy.is_some()))
        .count();
    ui.menu_button(t("Proxy"), |ui| {
        item_if(ui, s, "Create Proxies", "op.proxy.create", videos > 0);
        item_if(ui, s, "Remove Proxies", "op.proxy.remove", attached > 0);
        ui.separator();
        check(
            ui,
            s,
            "Enable Proxies",
            "op.view.toggle_proxies",
            s.ed.prefs.use_proxies,
        );
    });
}

pub(crate) fn sequence_menu(ui: &mut Ui, s: &mut State) {
    let has = s.ed.active.is_some();
    item_if(ui, s, "Sequence Settings...", "op.sequence.settings", has);
    ui.separator();
    item(
        ui,
        s,
        "Apply Video Transition",
        "cmd.sequence.applydefaultvideotransition",
    );
    item(
        ui,
        s,
        "Apply Audio Transition",
        "cmd.sequence.applydefaultaudiotransition",
    );
    item(
        ui,
        s,
        "Apply Default Transitions to Selection",
        "cmd.sequence.applydefaulttransitions",
    );
    ui.separator();
    item(ui, s, "Lift", "cmd.sequence.lift");
    item(ui, s, "Extract", "cmd.sequence.extract");
    item(ui, s, "Add Edit", "cmd.sequence.razorateditline");
    item(
        ui,
        s,
        "Add Edit to All Tracks",
        "cmd.sequence.razorateditline.all",
    );
    item(
        ui,
        s,
        "Extend Selected Edit to Playhead",
        "cmd.sequence.extendselectededittoplayhead",
    );
    item(
        ui,
        s,
        "Ripple Trim Previous Edit to Playhead",
        "cmd.sequence.rippletrimpreviousedittoplayhead",
    );
    item(
        ui,
        s,
        "Ripple Trim Next Edit to Playhead",
        "cmd.sequence.rippletrimnextedittoplayhead",
    );
    ui.separator();
    item(ui, s, "Match Frame", "cmd.sequence.matchframe");
    item(
        ui,
        s,
        "Reverse Match Frame",
        "cmd.sequence.reversematchframe",
    );
    item(ui, s, "Make Subsequence", "cmd.sequence.makesubsequence");
    ui.menu_button(t("Go to Gap"), |ui| {
        item(
            ui,
            s,
            "Next in Sequence",
            "cmd.sequence.findnextsequencegap",
        );
        item(
            ui,
            s,
            "Previous in Sequence",
            "cmd.sequence.findprevioussequencegap",
        );
    });
    ui.separator();
    let snap = s.ed.prefs.snapping;
    check(ui, s, "Snap in Timeline", "cmd.sequence.snap", snap);
    let linked = s.ed.prefs.linked_selection;
    check(
        ui,
        s,
        "Linked Selection",
        "op.sequence.linkedselection",
        linked,
    );
    ui.separator();
    item_if(ui, s, "Add Tracks...", "op.sequence.addtracks", has);
    item_if(
        ui,
        s,
        "Delete Empty Tracks",
        "op.sequence.deleteemptytracks",
        has,
    );
}

pub(crate) fn markers_menu(ui: &mut Ui, s: &mut State) {
    item(ui, s, "Mark In", "cmd.common.setin");
    item(ui, s, "Mark Out", "cmd.common.setout");
    item(
        ui,
        s,
        "Mark Clip",
        "cmd.marker.setsequenceinoutmarkeraroundtargetclip",
    );
    item(
        ui,
        s,
        "Mark Selection",
        "cmd.marker.setsequenceinoutmarkeraroundselection.out",
    );
    ui.separator();
    item(ui, s, "Go to In", "cmd.goto.in");
    item(ui, s, "Go to Out", "cmd.goto.out");
    ui.separator();
    item(ui, s, "Clear In", "cmd.clear.in");
    item(ui, s, "Clear Out", "cmd.clear.out");
    item(ui, s, "Clear In and Out", "cmd.clear.inandout");
    ui.separator();
    item(ui, s, "Add Marker", "cmd.set.marker");
    item(ui, s, "Go to Next Marker", "cmd.marker.gotomarker.next");
    item(
        ui,
        s,
        "Go to Previous Marker",
        "cmd.marker.gotomarker.previous",
    );
    item(
        ui,
        s,
        "Clear Selected Marker",
        "cmd.marker.clearmarker.current",
    );
    item(ui, s, "Clear All Markers", "cmd.marker.clearmarker.all");
    ui.separator();
    let ripple = s.ed.prefs.ripple_markers;
    check(
        ui,
        s,
        "Ripple Sequence Markers",
        "op.markers.ripple",
        ripple,
    );
}

pub(crate) fn view_menu(ui: &mut Ui, s: &mut State) {
    let on = s.ed.prefs.use_proxies;
    check(ui, s, "Enable Proxies", "op.view.toggle_proxies", on);
    ui.separator();
    ui.menu_button(t("Playback Resolution"), |ui| {
        for (d, label) in [(1u32, "Full"), (2, "1/2"), (4, "1/4"), (8, "1/8")] {
            if ui
                .add(egui::Button::new(t(label)).selected(s.ed.prefs.playback_resolution == d))
                .clicked()
            {
                s.ed.prefs.playback_resolution = d;
                ui.close();
            }
        }
    });
    ui.menu_button(t("Paused Resolution"), |ui| {
        for (d, label) in [(1u32, "Full"), (2, "1/2"), (4, "1/4"), (8, "1/8")] {
            if ui
                .add(egui::Button::new(t(label)).selected(s.ed.prefs.paused_resolution == d))
                .clicked()
            {
                s.ed.prefs.paused_resolution = d;
                s.ed.frame_generation += 1;
                ui.close();
            }
        }
    });
    ui.menu_button(t("Time Display"), |ui| {
        for (d, label) in [
            (TimeDisplay::Timecode, "Timecode"),
            (TimeDisplay::Frames, "Frames"),
            (TimeDisplay::Samples, "Audio Samples"),
        ] {
            if ui
                .add(egui::Button::new(t(label)).selected(s.ed.prefs.time_display == d))
                .clicked()
            {
                s.ed.prefs.time_display = d;
                s.ed.edit("Time Display", |p| {
                    p.settings.time_display = d;
                    Ok(())
                });
                ui.close();
            }
        }
    });
    let safe = s.program.safe_margins;
    if ui
        .add(egui::Button::new(t("Safe Margins")).selected(safe))
        .clicked()
    {
        s.program.safe_margins = !safe;
        s.source.safe_margins = !safe;
        ui.close();
    }
    let scrub = s.ed.prefs.audio_scrubbing;
    check(
        ui,
        s,
        "Audio Scrubbing",
        "cmd.toggle.audio.scrubbing",
        scrub,
    );
    ui.separator();
    item(ui, s, "Zoom In", "cmd.zoom.in");
    item(ui, s, "Zoom Out", "cmd.zoom.out");
    item(ui, s, "Zoom to Sequence", "cmd.tlnav.zoomto.sequence");
    ui.separator();
    ui.menu_button(t("Interface Scale"), |ui| {
        for v in [0.85f32, 1.0, 1.15, 1.25, 1.5, 1.75, 2.0] {
            if ui
                .add(
                    egui::Button::new(format!("{:.0} %", v * 100.0))
                        .selected((s.ed.prefs.ui_scale - v).abs() < 0.01),
                )
                .clicked()
            {
                s.ed.prefs.ui_scale = v;
                ui.close();
            }
        }
    });
}

// ------------------------------------------------------------------------------- the state
