//! Edit > Keyboard Shortcuts: command names and the shortcut editor.

use super::*;

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

pub(super) fn context_name(c: &str) -> &'static str {
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

pub(super) fn implemented(cmd: &str) -> bool {
    op_application::commands::EDITOR_COMMANDS.contains(&cmd)
        || op_application::Tool::from_command(cmd).is_some()
        || command_name(cmd) != cmd
}

pub(super) fn shortcuts_dialog(s: &mut State, ctx: &egui::Context, f: &mut ShortcutsForm) -> bool {
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
