//! The application window: menus, workspaces, docking, status bar, dialogs and the frame loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui::{Color32, RichText, Stroke, StrokeKind, Ui, ViewportCommand};
use egui_dock::{DockArea, DockState, TabViewer};
use op_application::{Dirs, Editor, Focus, Monitor, Preferences, autosave};
use op_core::*;

use crate::dialogs::{Dialog, Then};
use crate::i18n::{self, t, tf, tn};
use crate::workspace::{Panel, Workspace};
use crate::{
    effect_controls, effects_panel, keys, monitor, panels, project_panel, theme, timeline, widgets,
};

/// Something being dragged between panels.
#[derive(Clone, Debug)]
pub enum Drag {
    Items(Vec<ItemId>),
    Effect(&'static str),
}

enum DockAction {
    Focus(Panel),
    Workspace(Workspace),
    Reset,
}

/// Everything the panels share. Panels are functions over this state.
pub(crate) struct State {
    pub ed: Editor,
    pub gpu: Arc<op_render::Gpu>,
    pub renderer: op_render::Renderer,
    pub rs: egui_wgpu::RenderState,
    pub ctx: egui::Context,
    pub focus: Focus,
    pub focused_panel: Option<Panel>,
    pub k_down: bool,
    pub loop_playback: bool,
    pub workspace: Workspace,
    pub program: monitor::MonitorView,
    pub source: monitor::MonitorView,
    pub tl: timeline::TimelineView,
    pub proj: project_panel::ProjectView,
    pub fx: effects_panel::EffectsView,
    pub ec: effect_controls::EcView,
    pub scopes: panels::ScopesView,
    pub meters: panels::MeterState,
    pub graphics: panels::GraphicsView,
    pub dialogs: Vec<Dialog>,
    pub drag: Option<Drag>,
    dock_action: Option<DockAction>,
    thumbs: HashMap<(AssetId, i64), egui::TextureHandle>,
    title: String,
    allow_close: bool,
    quit: bool,
    /// Panels drawn in the previous frame (monitors render scopes only when visible).
    pub visible: HashSet<Panel>,
    drawn: HashSet<Panel>,
    ui_scale: f32,
}

pub struct App {
    dock: DockState<Panel>,
    layouts: HashMap<Workspace, DockState<Panel>>,
    s: State,
}

fn focus_of(p: Panel) -> Option<Focus> {
    Some(match p {
        Panel::Timeline => Focus::Timeline,
        Panel::Program => Focus::Program,
        Panel::Source => Focus::Source,
        Panel::Project => Focus::Project,
        Panel::EffectControls => Focus::EffectControls,
        Panel::Effects => Focus::Effects,
        Panel::History => Focus::History,
        Panel::Tools | Panel::Meters | Panel::Info => return None,
        _ => Focus::Other,
    })
}

fn layout_path(dirs: &Dirs, w: Workspace) -> PathBuf {
    dirs.workspaces().join(format!("{}.json", w.key()))
}

fn load_layout(dirs: &Dirs, w: Workspace) -> Option<DockState<Panel>> {
    let text = std::fs::read_to_string(layout_path(dirs, w)).ok()?;
    let d = crate::workspace::from_json(&text)?;
    // a layout without the timeline is from an incompatible version
    d.find_tab(&Panel::Timeline).is_some().then_some(d)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: crate::Options) -> Result<App, String> {
        let rs = cc
            .wgpu_render_state
            .clone()
            .ok_or("the window has no GPU renderer")?;
        let gpu =
            op_render::Gpu::shared(rs.device.clone(), rs.queue.clone(), rs.adapter.get_info());
        log::info!("GPU: {}", gpu.description());
        let renderer = op_render::Renderer::new(gpu.clone());
        let dirs = match &opts.portable {
            Some(root) => Dirs::portable(root),
            None => Dirs::system(),
        };
        let prefs = Preferences::load(&dirs);
        let lang = if prefs.language.is_empty() {
            i18n::system()
        } else {
            i18n::Lang::from_code(&prefs.language).unwrap_or(i18n::Lang::En)
        };
        i18n::set(lang);
        let ui_scale = prefs.ui_scale;
        theme::apply(&cc.egui_ctx, ui_scale);
        egui_extras_install(&cc.egui_ctx);
        let recovery = autosave::begin_session(&dirs);
        let ed = Editor::new(dirs, prefs, true);
        let workspace = std::fs::read_to_string(ed.dirs.workspaces().join("current.txt"))
            .ok()
            .and_then(|k| Workspace::ALL.into_iter().find(|w| w.key() == k.trim()))
            .unwrap_or(Workspace::Editing);
        let dock = load_layout(&ed.dirs, workspace).unwrap_or_else(|| workspace.layout());
        let mut s = State {
            ed,
            gpu,
            renderer,
            rs,
            ctx: cc.egui_ctx.clone(),
            focus: Focus::Timeline,
            focused_panel: Some(Panel::Timeline),
            k_down: false,
            loop_playback: false,
            workspace,
            program: monitor::MonitorView::new(Monitor::Program),
            source: monitor::MonitorView::new(Monitor::Source),
            tl: timeline::TimelineView::default(),
            proj: project_panel::ProjectView::default(),
            fx: effects_panel::EffectsView::default(),
            ec: effect_controls::EcView::default(),
            scopes: panels::ScopesView::default(),
            meters: panels::MeterState::default(),
            graphics: panels::GraphicsView::default(),
            dialogs: Vec::new(),
            drag: None,
            dock_action: None,
            thumbs: HashMap::new(),
            title: String::new(),
            allow_close: false,
            quit: false,
            visible: HashSet::new(),
            drawn: HashSet::new(),
            ui_scale,
        };
        s.renderer.warm_up();
        s.open_or_import(opts.open);
        if let Some(p) = recovery {
            s.dialogs.push(Dialog::Recover(p));
        }
        Ok(App {
            dock,
            layouts: HashMap::new(),
            s,
        })
    }

    fn apply_dock_action(&mut self) {
        let Some(a) = self.s.dock_action.take() else {
            return;
        };
        match a {
            DockAction::Focus(p) => {
                match self.dock.find_tab(&p) {
                    Some(path) => {
                        let _ = self.dock.set_active_tab(path);
                        self.dock.set_focused_node_and_surface(path.node_path());
                    }
                    None => self.dock.push_to_focused_leaf(p),
                }
                if let Some(f) = focus_of(p) {
                    self.s.focus = f;
                }
                self.s.focused_panel = Some(p);
            }
            DockAction::Workspace(w) => {
                if w != self.s.workspace {
                    let cur = std::mem::replace(&mut self.dock, DockState::new(vec![]));
                    self.layouts.insert(self.s.workspace, cur);
                    self.dock = self
                        .layouts
                        .remove(&w)
                        .or_else(|| load_layout(&self.s.ed.dirs, w))
                        .unwrap_or_else(|| w.layout());
                    self.s.workspace = w;
                }
            }
            DockAction::Reset => {
                self.dock = self.s.workspace.layout();
                let _ = std::fs::remove_file(layout_path(&self.s.ed.dirs, self.s.workspace));
            }
        }
    }

    fn save_layouts(&mut self) {
        let dir = self.s.ed.dirs.workspaces();
        let _ = std::fs::create_dir_all(&dir);
        let mut all: Vec<(Workspace, &DockState<Panel>)> =
            self.layouts.iter().map(|(w, d)| (*w, d)).collect();
        all.push((self.s.workspace, &self.dock));
        for (w, d) in all {
            if let Some(text) = crate::workspace::to_json(d) {
                let _ = std::fs::write(layout_path(&self.s.ed.dirs, w), text);
            }
        }
        let _ = std::fs::write(dir.join("current.txt"), self.s.workspace.key());
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let s = &mut self.s;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t("File"), |ui| file_menu(ui, s));
            ui.menu_button(t("Edit"), |ui| edit_menu(ui, s));
            ui.menu_button(t("Clip"), |ui| clip_menu(ui, s));
            ui.menu_button(t("Sequence"), |ui| sequence_menu(ui, s));
            ui.menu_button(t("Markers"), |ui| markers_menu(ui, s));
            ui.menu_button(t("Graphics"), |ui| {
                item(ui, s, "New Text Layer", "cmd.graphics.add.text");
                item(ui, s, "New Rectangle", "cmd.graphics.add.shape.rectangle");
                item(ui, s, "New Ellipse", "cmd.graphics.add.shape.ellipse");
            });
            ui.menu_button(t("View"), |ui| view_menu(ui, s));
            let dock = &self.dock;
            ui.menu_button(t("Window"), |ui| {
                ui.menu_button(t("Workspaces"), |ui| {
                    for (i, w) in Workspace::ALL.into_iter().enumerate() {
                        let keys =
                            s.ed.keymap
                                .keys_for(&format!("cmd.window.user.workspace.{i}"))
                                .unwrap_or_default();
                        if ui
                            .add(
                                egui::Button::new(t(w.title()))
                                    .selected(s.workspace == w)
                                    .shortcut_text(keys),
                            )
                            .clicked()
                        {
                            s.dock_action = Some(DockAction::Workspace(w));
                            ui.close();
                        }
                    }
                    ui.separator();
                    item(
                        ui,
                        s,
                        "Reset to Saved Layout",
                        "cmd.window.workspace.revert",
                    );
                });
                ui.separator();
                for p in Panel::ALL {
                    let open = dock.find_tab(&p).is_some();
                    if ui
                        .add(egui::Button::new(t(p.title())).selected(open))
                        .clicked()
                    {
                        s.dock_action = Some(DockAction::Focus(p));
                        ui.close();
                    }
                }
            });
            ui.menu_button(t("Help"), |ui| {
                item(ui, s, "Keyboard Shortcuts...", "cmd.edit.keyboardshortcuts");
                item(ui, s, "About OpenPremier", "op.help.about");
            });
            // workspace tabs on the right, like the header of professional editors
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for w in Workspace::ALL.into_iter().rev() {
                    let on = s.workspace == w;
                    let text = RichText::new(t(w.title())).color(if on {
                        theme::ACCENT
                    } else {
                        theme::TEXT_DIM
                    });
                    if ui.add(egui::Button::new(text).frame(false)).clicked() {
                        s.dock_action = Some(DockAction::Workspace(w));
                    }
                }
            });
        });
    }
}

// ------------------------------------------------------------------------------------ menus

fn item(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str) {
    item_if(ui, s, label, cmd, true);
}

fn item_if(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str, enabled: bool) {
    let keys = s.ed.keymap.keys_for(cmd).unwrap_or_default();
    if ui
        .add_enabled(enabled, egui::Button::new(t(label)).shortcut_text(keys))
        .clicked()
    {
        ui.close();
        s.command(cmd);
    }
}

fn check(ui: &mut Ui, s: &mut State, label: &'static str, cmd: &str, on: bool) {
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

fn file_menu(ui: &mut Ui, s: &mut State) {
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

fn edit_menu(ui: &mut Ui, s: &mut State) {
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

fn clip_menu(ui: &mut Ui, s: &mut State) {
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
}

fn sequence_menu(ui: &mut Ui, s: &mut State) {
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

fn markers_menu(ui: &mut Ui, s: &mut State) {
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

fn view_menu(ui: &mut Ui, s: &mut State) {
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

impl State {
    /// Runs a command: UI commands here, editing commands in the editor.
    pub fn command(&mut self, cmd: &str) {
        match cmd {
            "cmd.file.new.project" => self.confirm(Then::NewProject),
            "cmd.file.openproject" => self.confirm(Then::Open(None)),
            "cmd.file.close" => self.confirm(Then::NewProject),
            "cmd.file.save" => {
                self.save();
            }
            "cmd.file.saveas" => {
                self.save_as();
            }
            "cmd.file.savecopy" => self.save_copy(),
            "cmd.file.import" => self.import_dialog(),
            "cmd.file.exit" => self.quit = true,
            "cmd.file.closepanel" => {
                if self.focus == Focus::Timeline
                    && let Some(sid) = self.ed.active
                {
                    self.ed.close_sequence(sid);
                }
            }
            "cmd.file.new.sequence" => {
                let settings = self.ed.prefs.default_sequence.clone();
                self.dialogs.push(Dialog::new_sequence(settings));
            }
            "cmd.file.export.movie" | "cmd.file.export.sendtoqueue" => {
                if let Some(d) = Dialog::export(self) {
                    self.dialogs.push(d);
                } else {
                    self.ed.error(t("Open a sequence first"));
                }
            }
            "cmd.export.frame" => self.export_frame(),
            "op.file.export.otio" => self.export_interchange("otio"),
            "op.file.export.fcpxml" => self.export_interchange("xml"),
            "op.file.export.edl" => self.export_interchange("edl"),
            "op.project.settings" => self.dialogs.push(Dialog::ProjectSettings),
            "op.new.colormatte" => self.dialogs.push(Dialog::ColorMatte {
                color: [0.1, 0.1, 0.1],
                name: t("Color Matte").to_string(),
            }),
            "op.new.blackvideo" => self.new_synthetic(Generator::BlackVideo, t("Black Video")),
            "op.new.transparentvideo" => {
                self.new_synthetic(Generator::TransparentVideo, t("Transparent Video"))
            }
            "op.new.barsandtone" => self.new_synthetic(Generator::BarsAndTone, t("Bars and Tone")),
            "op.new.adjustmentlayer" => {
                self.new_synthetic(Generator::AdjustmentLayer, t("Adjustment Layer"))
            }
            "cmd.edit.pasteattributes" => {
                if self.ed.clipboard.clips.is_empty() || self.ed.selection.clips.is_empty() {
                    self.ed
                        .error(t("Copy a clip and select the clips to paste to"));
                } else {
                    self.dialogs
                        .push(Dialog::PasteAttributes(op_timeline::Attributes::default()));
                }
            }
            "cmd.edit.copy" => {
                self.ed.copy();
                // keeps the clipboard shortcut working in the next paste (egui needs text)
                if !self.ed.clipboard.is_empty() {
                    self.clipboard_text();
                }
            }
            "cmd.edit.cut" => {
                self.ed.execute(cmd, self.focus);
                self.clipboard_text();
            }
            "cmd.edit.keyboardshortcuts" => self.dialogs.push(Dialog::shortcuts()),
            "op.edit.preferences" => self.dialogs.push(Dialog::Preferences),
            "op.help.about" | "cmd.help.contents" => self.dialogs.push(Dialog::About),
            "cmd.clip.speed" => {
                if let Some(d) = Dialog::speed(self) {
                    self.dialogs.push(d);
                }
            }
            "cmd.clip.audiooptions.gain" => {
                if let Some(d) = Dialog::gain(self) {
                    self.dialogs.push(d);
                }
            }
            "op.clip.nest" => {
                if !self.ed.selection.clips.is_empty() {
                    self.dialogs
                        .push(Dialog::Nest(t("Nested Sequence").to_string()));
                }
            }
            "op.clip.framehold" => self.ed.frame_hold(),
            "op.clip.scaletoframe" => self.toggle_scale_to_frame(),
            "op.clip.rename" => self.rename_selection(),
            "op.sequence.settings" => {
                if let Some(seq) = self.ed.active_seq() {
                    self.dialogs.push(Dialog::SequenceSettings {
                        name: seq.name.clone(),
                        settings: seq.settings.clone(),
                    });
                }
            }
            "op.sequence.linkedselection" => {
                self.ed.prefs.linked_selection = !self.ed.prefs.linked_selection
            }
            "op.markers.ripple" => self.ed.prefs.ripple_markers = !self.ed.prefs.ripple_markers,
            "op.sequence.addtracks" => self.dialogs.push(Dialog::AddTracks { video: 1, audio: 0 }),
            "op.sequence.deleteemptytracks" => {
                self.ed.seq_edit("Delete Tracks", |p, sid, _| {
                    op_timeline::delete_empty_tracks(p, sid)
                });
            }
            "cmd.toggle.audio.scrubbing" => {
                self.ed.prefs.audio_scrubbing = !self.ed.prefs.audio_scrubbing
            }
            "cmd.zoom.in" => timeline::zoom_by(self, 1.5),
            "cmd.zoom.out" => timeline::zoom_by(self, 1.0 / 1.5),
            "cmd.tlnav.zoomto.sequence" => timeline::zoom_to_sequence(self),
            "cmd.timeline.show.next.screen" => timeline::page(self, 1.0),
            "cmd.timeline.show.previous.screen" => timeline::page(self, -1.0),
            "cmd.timeline.increase.video.tracks.height" => {
                self.tl.video_height = (self.tl.video_height + 12.0).min(200.0)
            }
            "cmd.timeline.decrease.video.tracks.height" => {
                self.tl.video_height = (self.tl.video_height - 12.0).max(24.0)
            }
            "cmd.timeline.increase.audio.tracks.height" => {
                self.tl.audio_height = (self.tl.audio_height + 12.0).min(200.0)
            }
            "cmd.timeline.decrease.audio.tracks.height" => {
                self.tl.audio_height = (self.tl.audio_height - 12.0).max(24.0)
            }
            "cmd.timeline.expand.all.tracks" => {
                self.tl.video_height = 90.0;
                self.tl.audio_height = 90.0;
            }
            "cmd.timeline.minimize.all.tracks" => {
                self.tl.video_height = 26.0;
                self.tl.audio_height = 26.0;
            }
            "cmd.window.workspace.revert" => self.dock_action = Some(DockAction::Reset),
            "cmd.history.step.backward" => self.ed.undo(),
            "cmd.history.step.forward" => self.ed.redo(),
            "cmd.project.toggle.view" => self.proj.icons = !self.proj.icons,
            "cmd.project.openinsource" => {
                if let Some(i) = self.ed.items.first().copied() {
                    self.ed.load_source(i);
                    self.dock_action = Some(DockAction::Focus(Panel::Source));
                }
            }
            "cmd.edit.find" | "cmd.select.find.box" => {
                self.proj.focus_search = true;
                self.dock_action = Some(DockAction::Focus(Panel::Project));
            }
            "cmd.select.next.panel" | "cmd.select.previous.panel" => {
                let order = [
                    Panel::Project,
                    Panel::Source,
                    Panel::Program,
                    Panel::Timeline,
                    Panel::EffectControls,
                ];
                let cur = order
                    .iter()
                    .position(|p| Some(*p) == self.focused_panel)
                    .unwrap_or(3);
                let next = if cmd.ends_with("next.panel") {
                    (cur + 1) % order.len()
                } else {
                    (cur + order.len() - 1) % order.len()
                };
                self.dock_action = Some(DockAction::Focus(order[next]));
            }
            _ if cmd.starts_with("cmd.monitor.nudge.") => self.nudge_motion(cmd),
            _ if cmd.starts_with("cmd.window.user.workspace.") => {
                if let Some(w) = cmd
                    .rsplit('.')
                    .next()
                    .and_then(|i| i.parse::<usize>().ok())
                    .and_then(|i| Workspace::ALL.get(i))
                {
                    self.dock_action = Some(DockAction::Workspace(*w));
                }
            }
            _ => {
                if let Some(p) = Panel::from_window_command(cmd) {
                    self.dock_action = Some(DockAction::Focus(p));
                } else if !self.ed.execute(cmd, self.focus) {
                    log::debug!("command not available: {cmd}");
                } else if self.loop_playback && cmd.starts_with("cmd.transport.") {
                    self.apply_loop();
                }
            }
        }
    }

    /// Loop playback: the In/Out range (or everything) repeats while playing.
    fn apply_loop(&mut self) {
        let Some(m) = self.ed.transport.playing else {
            return;
        };
        if self.ed.transport.loop_from.is_some() || self.ed.transport.speed <= 0.0 {
            return;
        }
        let end = match m {
            Monitor::Program => {
                SeqTime::ZERO
                    + self
                        .ed
                        .active_seq()
                        .map(|q| q.duration())
                        .unwrap_or(Dur::ZERO)
            }
            Monitor::Source => SeqTime::ZERO + self.ed.source_duration(),
        };
        let from = self.ed.mark(m, true).unwrap_or(SeqTime::ZERO);
        let to = self.ed.mark(m, false).unwrap_or(end);
        if to > from {
            self.ed.transport.loop_from = Some(from);
            self.ed.transport.stop_at = Some(to);
        }
    }

    fn clipboard_text(&self) {
        let names: Vec<String> = self
            .ed
            .clipboard
            .clips
            .iter()
            .map(|(_, c)| c.name.clone())
            .collect();
        self.ctx.copy_text(names.join("\n"));
    }

    /// Escape: cancel the current drag, else clear the selection.
    pub fn escape(&mut self) {
        if self.tl.cancel() {
            return;
        }
        if self.drag.take().is_some() {
            return;
        }
        if self.ed.is_playing() {
            self.ed.stop();
            return;
        }
        self.ed.selection.clear();
    }

    // ------------------------------------------------------------------------------ files

    /// Asks to save changes first when there are any.
    pub fn confirm(&mut self, then: Then) {
        if self.ed.history.is_dirty() {
            self.dialogs.push(Dialog::Unsaved(then));
        } else {
            self.proceed(then);
        }
    }

    pub fn proceed(&mut self, then: Then) {
        match then {
            Then::NewProject => {
                self.ed.new_project(t("Untitled"));
                self.after_open();
            }
            Then::Open(None) => {
                let mut d = rfd::FileDialog::new()
                    .add_filter(t("All Projects"), &["opproj", "prproj", "xml", "otio"])
                    .add_filter("OpenPremier", &[op_project::native::EXTENSION])
                    .add_filter("Adobe Premiere Pro", &["prproj"])
                    .add_filter("Final Cut Pro XML", &["xml"])
                    .add_filter("OpenTimelineIO", &["otio"]);
                if let Some(dir) = self.ed.prefs.recent.first().and_then(|p| p.parent()) {
                    d = d.set_directory(dir);
                }
                if let Some(p) = d.pick_file() {
                    self.open_path(&p);
                }
            }
            Then::Open(Some(p)) => self.open_path(&p),
            Then::Quit => {
                self.allow_close = true;
                self.quit = true;
            }
        }
    }

    pub fn open_path(&mut self, p: &Path) {
        match self.ed.open(p) {
            Ok(()) => {
                self.after_open();
                if let Some(r) = self.ed.last_import_report.take() {
                    self.dialogs.push(Dialog::ImportReport(Box::new(r)));
                }
            }
            Err(e) => self.ed.error(format!("{}: {e}", p.display())),
        }
    }

    fn after_open(&mut self) {
        self.tl = self.tl.fresh();
        self.program.reset();
        self.source.reset();
        self.thumbs.clear();
    }

    /// Opens a project given on the command line or dropped on the window, or imports media.
    pub fn open_or_import(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        let is_project = |p: &PathBuf| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                matches!(
                    e.to_ascii_lowercase().as_str(),
                    "opproj" | "prproj" | "otio"
                )
            })
        };
        if paths.len() == 1 && is_project(&paths[0]) {
            let p = paths[0].clone();
            self.confirm(Then::Open(Some(p)));
            return;
        }
        // folders import their files, keeping one bin per folder
        let mut files = Vec::new();
        for p in paths {
            if p.is_dir() {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let parent = self.ed.bin;
                if let Some(bin) = self.ed.edit("New Bin", |pr| Ok(pr.add_bin(parent, name))) {
                    let mut inner: Vec<PathBuf> = std::fs::read_dir(&p)
                        .map(|r| {
                            r.flatten()
                                .map(|e| e.path())
                                .filter(|x| x.is_file())
                                .collect()
                        })
                        .unwrap_or_default();
                    inner.sort();
                    self.ed.import(inner, bin);
                }
            } else {
                files.push(p);
            }
        }
        if !files.is_empty() {
            let bin = self.ed.bin;
            self.ed.import(files, bin);
        }
    }

    pub fn save(&mut self) -> bool {
        if self.ed.path.is_none() {
            return self.save_as();
        }
        match self.ed.save() {
            Ok(()) => true,
            Err(e) => {
                self.ed.error(tf("Could not save: {}", &[&e]));
                false
            }
        }
    }

    pub fn save_as(&mut self) -> bool {
        let name = format!("{}.{}", self.ed.project.name, op_project::native::EXTENSION);
        let mut d = rfd::FileDialog::new()
            .add_filter("OpenPremier", &[op_project::native::EXTENSION])
            .set_file_name(name);
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else {
            return false;
        };
        match self.ed.save_as(&path) {
            Ok(()) => true,
            Err(e) => {
                self.ed.error(tf("Could not save: {}", &[&e]));
                false
            }
        }
    }

    fn save_copy(&mut self) {
        let name = format!(
            "{} {}.{}",
            self.ed.project.name,
            t("Copy"),
            op_project::native::EXTENSION
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("OpenPremier", &[op_project::native::EXTENSION])
            .set_file_name(name)
            .save_file()
        {
            match self.ed.save_copy(&path) {
                Ok(()) => self.ed.info(tf("Saved a copy to {}", &[&path.display()])),
                Err(e) => self.ed.error(tf("Could not save: {}", &[&e])),
            }
        }
    }

    pub fn import_dialog(&mut self) {
        let mut d = rfd::FileDialog::new()
            .add_filter(
                t("All Supported Media"),
                &[
                    "mp4", "mov", "mkv", "avi", "mxf", "m4v", "webm", "mts", "m2ts", "mpg", "mpeg",
                    "wmv", "flv", "3gp", "gif", "wav", "mp3", "aac", "m4a", "flac", "ogg", "opus",
                    "aif", "aiff", "wma", "png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp",
                    "exr", "dpx", "tga", "psd", "opproj", "prproj", "otio",
                ],
            )
            .add_filter(t("All Files"), &["*"]);
        if let Some(dir) = &self.ed.prefs.last_import_dir {
            d = d.set_directory(dir);
        }
        if let Some(files) = d.pick_files() {
            let bin = self.ed.bin;
            self.ed.import(files, bin);
        }
    }

    fn export_frame(&mut self) {
        let Some(sid) = self.ed.active else { return };
        let t0 = self.ed.playhead();
        let name = format!(
            "{}_{}.png",
            self.ed
                .active_seq()
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            t0.ticks() / (TICKS_PER_SECOND / 1000)
        );
        let mut d = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name(name);
        if let Some(dir) = &self.ed.prefs.last_export_dir {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else { return };
        let snap = self.ed.snapshot();
        match op_application::export::export_frame(
            &snap,
            sid,
            t0,
            &path,
            self.gpu.clone(),
            self.ed.media.clone(),
        ) {
            Ok(()) => self.ed.info(tf("Exported frame to {}", &[&path.display()])),
            Err(e) => self.ed.error(tf("Export failed: {}", &[&e])),
        }
    }

    fn export_interchange(&mut self, kind: &str) {
        let Some(sid) = self.ed.active else { return };
        let p = &self.ed.project;
        let (filter, text) = match kind {
            "otio" => ("OpenTimelineIO", op_project::otio::export(p, sid)),
            "xml" => ("Final Cut Pro XML", op_project::fcpxml::export(p, sid)),
            _ => ("EDL", op_project::edl::export(p, sid)),
        };
        let text = match text {
            Ok(t) => t,
            Err(e) => {
                self.ed.error(tf("Export failed: {}", &[&e]));
                return;
            }
        };
        let name = format!(
            "{}.{kind}",
            self.ed
                .active_seq()
                .map(|s| s.name.clone())
                .unwrap_or_default()
        );
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(filter, &[kind])
            .set_file_name(name)
            .save_file()
        {
            match std::fs::write(&path, text) {
                Ok(()) => self.ed.info(tf("Exported {}", &[&path.display()])),
                Err(e) => self.ed.error(tf("Export failed: {}", &[&e])),
            }
        }
    }

    pub fn new_synthetic(&mut self, generator: Generator, name: &str) {
        let bin = self.ed.bin;
        let d = self.ed.project.settings.still_duration;
        let name = name.to_string();
        if let Some(id) = self.ed.edit("New Item", move |p| {
            Ok(p.add_item(
                bin,
                name,
                ItemKind::Synthetic {
                    generator,
                    duration: d,
                },
            ))
        }) {
            self.ed.items = vec![id];
        }
    }

    fn toggle_scale_to_frame(&mut self) {
        let clips = self.ed.selection.clips.clone();
        self.ed.seq_edit("Scale to Frame Size", |p, sid, _| {
            let s = p.sequence_mut(sid).unwrap();
            let on = !clips
                .iter()
                .filter_map(|c| s.clip(*c))
                .any(|c| c.scale_to_frame);
            for c in &clips {
                if let Some(c) = s.clip_mut(*c)
                    && c.is_video()
                {
                    c.scale_to_frame = on;
                }
            }
            Ok(())
        });
    }

    fn rename_selection(&mut self) {
        if self.focus == Focus::Project {
            if let Some(i) = self.ed.items.first().copied() {
                self.proj.start_rename(&self.ed, i);
            }
        } else if let Some(c) = self.ed.selection.clips.first().copied()
            && let Some(clip) = self.ed.active_seq().and_then(|s| s.clip(c))
        {
            self.dialogs.push(Dialog::RenameClip(c, clip.name.clone()));
        }
    }

    /// Ctrl+arrows in the Program Monitor move the selected clip by one or five pixels.
    fn nudge_motion(&mut self, cmd: &str) {
        let step = if cmd.ends_with("five") { 5.0 } else { 1.0 };
        let (dx, dy) = if cmd.contains(".left.") {
            (-step, 0.0)
        } else if cmd.contains(".right.") {
            (step, 0.0)
        } else if cmd.contains(".up.") {
            (0.0, -step)
        } else {
            (0.0, step)
        };
        monitor::nudge_selected(self, dx, dy);
    }

    // ---------------------------------------------------------------------------- helpers

    /// Timecode format and start offset for a monitor.
    pub fn timecode(&self, m: Monitor) -> (TimecodeFormat, Dur) {
        let display = self.ed.prefs.time_display;
        match m {
            Monitor::Program => match self.ed.active_seq() {
                Some(s) => {
                    let mut f = TimecodeFormat::new(s.rate(), s.settings.drop_frame);
                    f.display = display;
                    f.audio_rate = Rate::fps(s.settings.audio_rate.max(1));
                    (f, s.settings.start_timecode)
                }
                None => (TimecodeFormat::new(Rate::FPS_25, false), Dur::ZERO),
            },
            Monitor::Source => {
                let mut f = TimecodeFormat::new(self.ed.source_rate(), false);
                f.display = display;
                (f, Dur::ZERO)
            }
        }
    }

    /// A cached thumbnail texture of a media frame (generated in the background).
    pub fn thumb(
        &mut self,
        ctx: &egui::Context,
        asset: AssetId,
        index: i64,
    ) -> Option<egui::TextureId> {
        if let Some(h) = self.thumbs.get(&(asset, index)) {
            return Some(h.id());
        }
        let a = self.ed.media.asset(asset)?;
        let img = self.ed.media.thumbnail(&a, index)?;
        if self.thumbs.len() > 600 {
            self.thumbs.clear();
        }
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width as usize, img.height as usize],
            &img.rgba,
        );
        let h = ctx.load_texture(
            format!("thumb-{}-{index}", asset.0),
            color,
            egui::TextureOptions::LINEAR,
        );
        let id = h.id();
        self.thumbs.insert((asset, index), h);
        Some(id)
    }

    fn panel_ui(&mut self, ui: &mut Ui, p: Panel) {
        self.drawn.insert(p);
        let rect = ui.max_rect();
        if ui.input(|i| i.pointer.any_pressed()) && ui.rect_contains_pointer(rect) {
            if let Some(f) = focus_of(p) {
                self.focus = f;
            }
            self.focused_panel = Some(p);
        }
        match p {
            Panel::Project => project_panel::show(self, ui),
            Panel::Source => monitor::show(self, ui, Monitor::Source),
            Panel::Program => monitor::show(self, ui, Monitor::Program),
            Panel::Timeline => timeline::show(self, ui),
            Panel::EffectControls => effect_controls::show(self, ui),
            Panel::Effects => effects_panel::show(self, ui),
            Panel::History => panels::history(self, ui),
            Panel::Tools => panels::tools(self, ui),
            Panel::Meters => panels::meters(self, ui),
            Panel::AudioMixer => panels::mixer(self, ui),
            Panel::Markers => panels::markers(self, ui),
            Panel::Info => panels::info(self, ui),
            Panel::Lumetri => panels::lumetri(self, ui),
            Panel::Scopes => panels::scopes(self, ui),
            Panel::Graphics => panels::graphics(self, ui),
        }
        if self.focused_panel == Some(p) && focus_of(p).is_some() {
            ui.painter().rect_stroke(
                rect,
                0.0,
                Stroke::new(1.0, theme::ACCENT_DIM),
                StrokeKind::Inside,
            );
        }
    }

    fn begin_frame(&mut self, ctx: &egui::Context) {
        let busy = self.ed.tick();
        if self.ed.is_playing() {
            ctx.request_repaint();
        } else if busy {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
        if self
            .ed
            .status
            .as_ref()
            .is_some_and(|s| s.at.elapsed() < Duration::from_secs(8))
        {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        let title = self.ed.title();
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() {
            self.open_or_import(dropped);
        }
        self.meters.update(&self.ed);
        if self.meters.active() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
        if (self.ed.prefs.ui_scale - self.ui_scale).abs() > 0.001 {
            self.ui_scale = self.ed.prefs.ui_scale;
            theme::apply(ctx, self.ui_scale);
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.ed.history.is_dirty() {
                ctx.send_viewport_cmd(ViewportCommand::CancelClose);
                if !self.dialogs.iter().any(|d| matches!(d, Dialog::Unsaved(_))) {
                    self.dialogs.push(Dialog::Unsaved(Then::Quit));
                }
            } else {
                self.allow_close = true;
            }
        }
        self.visible = std::mem::take(&mut self.drawn);
    }

    fn end_frame(&mut self, ctx: &egui::Context) {
        if let Some(d) = &self.drag {
            let text = match d {
                Drag::Items(items) => {
                    let first = items
                        .first()
                        .and_then(|i| self.ed.project.item(*i))
                        .map(|i| i.name.clone())
                        .unwrap_or_default();
                    if items.len() > 1 {
                        format!("{first} +{}", items.len() - 1)
                    } else {
                        first
                    }
                }
                Drag::Effect(id) => op_core::catalog::find(id)
                    .map(|d| tn(d.name))
                    .unwrap_or_default(),
            };
            if let Some(pos) = ctx.pointer_hover_pos() {
                egui::Area::new(egui::Id::new("drag-label"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(pos + egui::vec2(14.0, 10.0))
                    .interactable(false)
                    .show(ctx, |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(RichText::new(text).color(theme::TEXT_BRIGHT));
                        });
                    });
            }
        }
        if ctx.input(|i| i.pointer.any_released()) {
            self.drag = None;
            self.ed.seal();
        }
        if self.quit {
            if self.ed.history.is_dirty() && !self.allow_close {
                self.quit = false;
                if !self.dialogs.iter().any(|d| matches!(d, Dialog::Unsaved(_))) {
                    self.dialogs.push(Dialog::Unsaved(Then::Quit));
                }
            } else {
                self.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;
            if let Some(st) = &self.ed.status
                && st.at.elapsed() < Duration::from_secs(8)
            {
                let c = if st.error { theme::ERROR } else { theme::TEXT };
                ui.label(
                    RichText::new(translate_status(&st.text))
                        .color(c)
                        .size(12.0),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(self.gpu.description())
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );
                let mut done = Vec::new();
                for (i, job) in self.ed.exports.iter().enumerate() {
                    let p = job.progress.lock().clone();
                    if p.done {
                        continue;
                    }
                    if ui.small_button(t("Cancel")).clicked() {
                        done.push(i);
                    }
                    let name = job
                        .settings
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let eta = p.remaining.map(widgets::clock).unwrap_or_default();
                    ui.add(
                        egui::ProgressBar::new(p.fraction())
                            .desired_width(160.0)
                            .text(format!("{:.0} %  {eta}", p.fraction() * 100.0)),
                    );
                    ui.label(RichText::new(tf("Exporting {}", &[&name])).size(12.0));
                    ui.ctx().request_repaint_after(Duration::from_millis(200));
                }
                for i in done {
                    self.ed.exports[i].cancel();
                }
                let conforming = self.ed.media.conforming();
                if conforming > 0 {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Conforming audio ({})", &[&conforming]))
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );
                }
                if self.ed.importing > 0 {
                    ui.spinner();
                    ui.label(
                        RichText::new(tf("Importing {} files", &[&self.ed.importing]))
                            .size(12.0)
                            .color(theme::TEXT_DIM),
                    );
                }
            });
        });
    }
}

/// Translates the editor's status messages (written in English by the application layer).
fn translate_status(text: &str) -> String {
    if i18n::current() == i18n::Lang::En {
        return text.to_string();
    }
    for (prefix, fmt) in [
        ("Undo ", "Undo {}"),
        ("Redo ", "Redo {}"),
        ("Saved ", "Saved {}"),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            let rest = if prefix == "Saved " {
                rest.to_string()
            } else {
                tn(rest)
            };
            return tf(fmt, &[&rest]);
        }
    }
    if let Some(n) = text
        .strip_prefix("Imported ")
        .and_then(|r| r.strip_suffix(" files"))
    {
        return tf("Imported {} files", &[&n]);
    }
    if let Some(n) = text.strip_suffix(" media files are offline") {
        return tf("{} media files are offline", &[&n]);
    }
    tn(text)
}

/// Font fallbacks: the default fonts cover Latin text; symbols used in menus come from them too.
fn egui_extras_install(ctx: &egui::Context) {
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
    });
}

// --------------------------------------------------------------------------------- docking

struct Viewer<'a> {
    s: &'a mut State,
}

impl TabViewer for Viewer<'_> {
    type Tab = Panel;

    fn id(&mut self, tab: &mut Panel) -> egui::Id {
        egui::Id::new(("panel", *tab))
    }

    fn title(&mut self, tab: &mut Panel) -> egui::WidgetText {
        let base = t(tab.title());
        let text = match tab {
            Panel::Program => match self.s.ed.active_seq() {
                Some(seq) => format!("{base}: {}", seq.name),
                None => base.to_string(),
            },
            Panel::Source => match self
                .s
                .ed
                .source
                .item
                .and_then(|i| self.s.ed.project.item(i))
            {
                Some(it) => format!("{base}: {}", it.name),
                None => base.to_string(),
            },
            Panel::Project => format!("{base}: {}", self.s.ed.project.name),
            _ => base.to_string(),
        };
        text.into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Panel) {
        self.s.panel_ui(ui, *tab);
    }

    fn scroll_bars(&self, _tab: &Panel) -> [bool; 2] {
        [false, false]
    }

    fn is_closeable(&self, tab: &Panel) -> bool {
        !matches!(tab, Panel::Tools | Panel::Meters)
    }
}

fn dock_style(style: &egui::Style) -> egui_dock::Style {
    let mut d = egui_dock::Style::from_egui(style);
    d.main_surface_border_stroke = Stroke::NONE;
    d.tab_bar.bg_fill = theme::PANEL_DARK;
    d.tab_bar.height = 26.0;
    d.tab_bar.hline_color = theme::LINE;
    d.tab.tab_body.bg_fill = theme::PANEL;
    d.tab.tab_body.stroke = Stroke::NONE;
    d.tab.tab_body.inner_margin = egui::Margin::same(0);
    for s in [
        &mut d.tab.active,
        &mut d.tab.focused,
        &mut d.tab.active_with_kb_focus,
        &mut d.tab.focused_with_kb_focus,
    ] {
        s.bg_fill = theme::PANEL;
        s.text_color = theme::TEXT_BRIGHT;
        s.outline_color = theme::PANEL;
    }
    for s in [&mut d.tab.inactive, &mut d.tab.inactive_with_kb_focus] {
        s.bg_fill = theme::PANEL_DARK;
        s.text_color = theme::TEXT_DIM;
        s.outline_color = theme::PANEL_DARK;
    }
    d.tab.hovered.bg_fill = theme::RAISED;
    d.tab.hovered.text_color = theme::TEXT_BRIGHT;
    d.separator.width = 3.0;
    d.separator.extra = 56.0;
    d.separator.color_idle = theme::BG;
    d.separator.color_hovered = theme::ACCENT_DIM;
    d.separator.color_dragged = theme::ACCENT;
    d
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.s.begin_frame(&ctx);
        if self.s.dialogs.is_empty() {
            keys::handle(&mut self.s, &ctx);
        }
        egui::Panel::top("menu")
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL_DARK)
                    .inner_margin(egui::Margin::symmetric(6, 2)),
            )
            .show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::PANEL_DARK)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| self.s.status_bar(ui));
        let style = dock_style(ui.style());
        egui::CentralPanel::no_frame().show(ui, |ui| {
            ui.painter().rect_filled(ui.max_rect(), 0.0, theme::BG);
            let mut viewer = Viewer { s: &mut self.s };
            DockArea::new(&mut self.dock)
                .style(style)
                .show_add_buttons(false)
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        crate::dialogs::show(&mut self.s, &ctx);
        self.apply_dock_action();
        self.s.end_frame(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(0x1d, 0x1d, 0x20).to_normalized_gamma_f32()
    }

    fn on_exit(&mut self) {
        self.s.ed.stop();
        self.save_layouts();
        let _ = self.s.ed.prefs.save(&self.s.ed.dirs);
        autosave::end_session(&self.s.ed.dirs);
    }
}
