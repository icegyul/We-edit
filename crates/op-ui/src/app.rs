//! The application window: menus, workspaces, docking, status bar, dialogs and the frame loop.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Stroke, StrokeKind, Ui, ViewportCommand};
use egui_dock::{DockArea, DockState, SurfaceIndex, TabViewer};
use op_application::{Dirs, Editor, Focus, LoadedProject, Monitor, Preferences, autosave};
use op_core::*;

use crate::dialogs::{Dialog, Then};
use crate::i18n::{self, t, tf, tn};
use crate::icons::{self, Icon};
use crate::workspace::{Panel, Workspace};
use crate::{
    effect_controls, effects_panel, keys, monitor, panels, project_panel, theme, timeline, widgets,
};

/// A project being read on a background thread.
pub struct Opening {
    pub path: PathBuf,
    rx: std::sync::mpsc::Receiver<Result<LoadedProject, String>>,
    pub started: Instant,
    /// A recovered copy: it must be saved under a new name, so it starts as unsaved.
    recovered: bool,
}

/// Export progress presentation: the frames shown in the Program Monitor and the entries at the
/// right of the status bar.
#[derive(Default)]
pub struct ExportUi {
    preview: Option<(u64, egui::TextureHandle)>,
    /// Finished exports whose status bar entry was closed or has expired.
    dismissed: HashSet<usize>,
    /// When each export was first seen finished (successful entries hide after a while).
    finished_at: HashMap<usize, Instant>,
}

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
    /// A paste event arrived during the current Ctrl+V press (the release must not paste again).
    pub paste_seen: bool,
    /// Pasted images waiting to be saved and imported.
    pub pastes: Vec<crate::paste::Job>,
    next_paste: u64,
    /// Settings of the last caption dialog (style, words per caption, model, language).
    pub caption_options: op_application::captions::CaptionOptions,
    pub caption_model: op_application::captions::ModelSize,
    pub caption_language: usize,
    performance_applied: Option<bool>,
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
    pub opening: Option<Opening>,
    pub export_ui: ExportUi,
    /// Tab button rectangles of this frame (for the active-tab underline).
    tab_rects: HashMap<Panel, egui::Rect>,
    dock_action: Option<DockAction>,
    thumbs: HashMap<(AssetId, i64), egui::TextureHandle>,
    title: String,
    allow_close: bool,
    quit: bool,
    /// Panels drawn in the previous frame (monitors render scopes only when visible).
    pub visible: HashSet<Panel>,
    drawn: HashSet<Panel>,
    ui_scale: f32,
    minimized: bool,
}

pub struct App {
    dock: DockState<Panel>,
    layouts: HashMap<Workspace, DockState<Panel>>,
    /// Floating panel windows already placed (new ones open centered).
    placed: HashSet<SurfaceIndex>,
    lang: i18n::Lang,
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
    dirs.workspaces().join(format!(
        "{}.v{}.json",
        w.key(),
        crate::workspace::LAYOUT_VERSION
    ))
}

/// Built-in texts of the docking system in the interface language.
fn dock_translations() -> egui_dock::Translations {
    let mut tr = egui_dock::Translations::english();
    tr.tab_context_menu.close_button = t("Close Panel").into();
    tr.tab_context_menu.eject_button = t("Undock Panel").into();
    tr.tab_context_menu.hide_tab_bar_button = t("Hide Tab Bar").into();
    tr.tab_context_menu.show_tab_bar_button = t("Show Tab Bar").into();
    tr.leaf.close_button_disabled_tooltip = t("This panel group cannot be closed.").into();
    tr.leaf.close_all_button = t("Close Window").into();
    tr.leaf.close_all_button_menu_hint = t("Right-click to close this window.").into();
    tr.leaf.close_all_button_modifier_hint = t("Hold Shift to close this window.").into();
    tr.leaf.close_all_button_modifier_menu_hint =
        t("Hold Shift or right-click to close this window.").into();
    tr.leaf.close_all_button_disabled_tooltip = t("This window cannot be closed.").into();
    tr.leaf.minimize_button = t("Minimize Window").into();
    tr.leaf.minimize_button_menu_hint = t("Right-click to minimize this window.").into();
    tr.leaf.minimize_button_modifier_hint = t("Hold Shift to minimize this window.").into();
    tr.leaf.minimize_button_modifier_menu_hint =
        t("Hold Shift or right-click to minimize this window.").into();
    tr
}

/// The icon shown before a panel's name in its tab.
fn panel_icon(p: Panel) -> Icon {
    match p {
        Panel::Project => Icon::Folder,
        Panel::Source => Icon::Film,
        Panel::Program => Icon::Play,
        Panel::Timeline => Icon::Sequence,
        Panel::EffectControls => Icon::Stopwatch,
        Panel::Effects => Icon::Fx,
        Panel::History => Icon::Reset,
        Panel::Tools => Icon::Selection,
        Panel::Meters | Panel::AudioMixer => Icon::Music,
        Panel::Markers => Icon::Marker,
        Panel::Info => Icon::List,
        Panel::Lumetri => Icon::Eye,
        Panel::Scopes => Icon::Grid,
        Panel::Graphics => Icon::Type,
    }
}

/// Opens a folder in the system file manager.
pub fn open_folder(path: &Path) {
    let _ = std::fs::create_dir_all(path);
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(path).spawn() {
        log::warn!("cannot open {}: {e}", path.display());
    }
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
        // a GPU validation error must not end the program: report it and carry on
        rs.device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
            log::error!("GPU error: {e}");
        }));
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
            paste_seen: false,
            pastes: Vec::new(),
            next_paste: 1,
            caption_options: Default::default(),
            caption_model: op_application::captions::ModelSize::Base,
            caption_language: 0,
            performance_applied: None,
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
            opening: None,
            export_ui: ExportUi::default(),
            tab_rects: HashMap::new(),
            dock_action: None,
            thumbs: HashMap::new(),
            title: String::new(),
            allow_close: false,
            quit: false,
            visible: HashSet::new(),
            drawn: HashSet::new(),
            ui_scale,
            minimized: false,
        };
        s.renderer.warm_up();
        s.open_or_import(opts.open);
        if let Some(p) = recovery {
            s.dialogs.push(Dialog::Recover(p));
        }
        let mut dock = dock;
        dock.translations = dock_translations();
        Ok(App {
            dock,
            layouts: HashMap::new(),
            placed: HashSet::new(),
            lang: i18n::current(),
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
                    self.dock.translations = dock_translations();
                    self.s.workspace = w;
                    log::debug!("workspace {}", w.key());
                }
            }
            DockAction::Reset => {
                self.dock = self.s.workspace.layout();
                self.dock.translations = dock_translations();
                let _ = std::fs::remove_file(layout_path(&self.s.ed.dirs, self.s.workspace));
            }
        }
    }

    /// Newly undocked panels open centered in the program window; the docking texts follow the
    /// interface language.
    fn tend_dock(&mut self, ctx: &egui::Context) {
        if i18n::current() != self.lang {
            self.lang = i18n::current();
            self.dock.translations = dock_translations();
        }
        let center = ctx.content_rect().center();
        let size = egui::vec2(560.0, 420.0);
        for (index, surface) in self.dock.iter_surfaces_mut_indexed() {
            if let egui_dock::Surface::Window(_, state) = surface
                && self.placed.insert(index)
            {
                state.set_size(size);
                state.set_position(center - size / 2.0);
            }
        }
        let alive: HashSet<SurfaceIndex> = self
            .dock
            .iter_surfaces_indexed()
            .filter(|(_, s)| matches!(s, egui_dock::Surface::Window(..)))
            .map(|(i, _)| i)
            .collect();
        self.placed.retain(|i| alive.contains(i));
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
                ui.separator();
                ui.menu_button(t("Captions"), |ui| captions_menu(ui, s));
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
                ui.separator();
                item(ui, s, "Show Log...", "op.help.log");
                item(ui, s, "Open Log Folder", "op.help.logfolder");
                ui.separator();
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
    ui.separator();
    proxy_menu(ui, s);
}

/// Graphics > Captions.
fn captions_menu(ui: &mut Ui, s: &mut State) {
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

impl State {
    /// Runs a command: UI commands here, editing commands in the editor.
    pub fn command(&mut self, cmd: &str) {
        // media in the system clipboard (copied images, files, image links) wins over clips
        // copied inside the program: copying clips replaces the system clipboard with text
        if cmd == "cmd.edit.paste" && self.paste_media() {
            return;
        }
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
            "op.captions.transcribe" => {
                if self.ed.active.is_none() {
                    self.ed.error(t("Open a sequence first"));
                } else {
                    let d = Dialog::captions(self);
                    self.dialogs.push(d);
                }
            }
            "op.captions.import" => self.import_captions(),
            "op.captions.export" => self.export_captions(),
            "op.view.toggle_proxies" => {
                let on = !self.ed.prefs.use_proxies;
                self.ed.set_use_proxies(on);
            }
            "op.proxy.create" => {
                let assets = self.selected_assets();
                self.ed.create_proxies(&assets);
            }
            "op.proxy.remove" => {
                let assets = self.selected_assets();
                self.ed.remove_proxies(&assets);
            }
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
            "op.edit.preferences" => self.dialogs.push(Dialog::preferences(self)),
            "op.help.log" => self.dialogs.push(Dialog::log()),
            "op.help.logfolder" => {
                if let Some(dir) = op_application::logging::folder() {
                    open_folder(&dir);
                }
            }
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
                } else {
                    log::debug!("command {cmd} ({:?})", self.focus);
                    if self.loop_playback && cmd.starts_with("cmd.transport.") {
                        self.apply_loop();
                    }
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

    /// Time until the playing sequence shows its next frame.
    fn next_frame_delay(&self) -> Duration {
        let max = Duration::from_secs_f64(1.0 / 60.0);
        let Some(seq) = self.ed.active_seq() else {
            return max;
        };
        let fd = seq.rate().frame_duration().seconds().max(1e-3);
        let speed = self.ed.transport.speed.abs().max(0.01);
        let t = self.ed.playhead().seconds();
        let next = ((t / fd).floor() + 1.0) * fd;
        Duration::from_secs_f64(((next - t) / speed).max(0.001)).min(max)
    }

    /// Animations follow the light mode preference.
    fn apply_performance_mode(&mut self, ctx: &egui::Context) {
        let on = self.ed.prefs.performance_mode;
        if self.performance_applied == Some(on) {
            return;
        }
        self.performance_applied = Some(on);
        ctx.all_styles_mut(|st| {
            st.animation_time = if on { 0.0 } else { 1.0 / 12.0 };
            st.scroll_animation = if on {
                egui::style::ScrollAnimation::none()
            } else {
                egui::style::ScrollAnimation::default()
            };
        });
        log::info!("performance mode {}", if on { "on" } else { "off" });
    }

    /// Pastes media from the system clipboard. Returns false when it holds none.
    fn paste_media(&mut self) -> bool {
        use crate::paste::{self, Found, Job, Payload};
        let Some(found) = paste::read() else {
            return false;
        };
        let place = self.ed.active.is_some()
            && !matches!(self.focus, Focus::Project | Focus::Source | Focus::Effects);
        let bin = self.ed.project.root;
        let (data, name, source) = match found {
            Found::Files(files) => {
                log::info!("pasting {} files", files.len());
                if place {
                    self.ed.place_after_import.extend(files.iter().cloned());
                }
                self.ed.import(files, bin);
                return true;
            }
            Found::Bitmap(b) => {
                let stamp = op_application::autosave::stamp(std::time::SystemTime::now());
                (
                    Arc::new(parking_lot::Mutex::new(Some(Ok(Payload::Bitmap(b))))),
                    format!("{} {stamp}", t("Pasted Image")),
                    t("Image from the clipboard").to_string(),
                )
            }
            Found::Url(url) => {
                let slot = Arc::new(parking_lot::Mutex::new(None));
                let (s2, u2) = (slot.clone(), url.clone());
                let _ = std::thread::Builder::new()
                    .name("paste-download".into())
                    .spawn(move || {
                        let r = paste::download(&u2);
                        if let Err(e) = &r {
                            log::warn!("pasted link not downloaded: {u2}: {e}");
                        }
                        *s2.lock() = Some(r);
                    });
                let name = paste::name_from_url(&url).unwrap_or_else(|| t("Pasted Image").into());
                (slot, name, url)
            }
        };
        let id = self.next_paste;
        self.next_paste += 1;
        let folder = self.paste_folder();
        let auto = self.ed.prefs.paste_always && self.ed.prefs.paste_folder.is_some();
        self.pastes.push(Job {
            id,
            name: name.clone(),
            source,
            data,
            folder: auto.then(|| folder.clone()),
            place,
            preview: None,
        });
        if !auto {
            self.dialogs.push(Dialog::PasteMedia {
                id,
                folder: folder.display().to_string(),
                name,
                always: false,
            });
        }
        true
    }

    /// Where pasted images go by default: the chosen folder, else next to the project, else
    /// the user's pictures.
    pub fn paste_folder(&self) -> PathBuf {
        if let Some(f) = &self.ed.prefs.paste_folder {
            return f.clone();
        }
        if let Some(dir) = self.ed.path.as_ref().and_then(|p| p.parent()) {
            return dir.join(t("Pasted Media"));
        }
        // Pictures on Windows and macOS, the XDG pictures folder on Linux
        dirs::picture_dir()
            .filter(|p| p.is_dir())
            .or_else(dirs::home_dir)
            .unwrap_or_default()
            .join("OpenPremier")
    }

    /// Saves and imports pasted images whose folder is known and whose data is ready.
    fn poll_pastes(&mut self) {
        let mut i = 0;
        while i < self.pastes.len() {
            let job = &self.pastes[i];
            let Some(folder) = job.folder.clone() else {
                i += 1;
                continue;
            };
            let Some(result) = job.data.lock().take() else {
                i += 1;
                continue;
            };
            let job = self.pastes.remove(i);
            match result.and_then(|p| crate::paste::save(&p, &folder, &job.name)) {
                Ok(path) => {
                    log::info!("pasted image saved to {}", path.display());
                    if job.place {
                        self.ed.place_after_import.push(path.clone());
                    }
                    let bin = self.ed.project.root;
                    self.ed.import(vec![path], bin);
                }
                Err(e) => self
                    .ed
                    .error(tf("The pasted image could not be saved: {}", &[&e])),
            }
        }
        if !self.pastes.is_empty() {
            self.ctx.request_repaint_after(Duration::from_millis(200));
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

    /// Reads a project on a background thread; the window stays responsive meanwhile.
    pub fn open_path(&mut self, p: &Path) {
        self.open_in_background(p, false);
    }

    /// Opens a recovered copy: it is shown as unsaved and must be saved under a name.
    pub fn open_recovered(&mut self, p: &Path) {
        self.open_in_background(p, true);
    }

    fn open_in_background(&mut self, p: &Path, recovered: bool) {
        if self.opening.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let path = p.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("open-project".into())
            .spawn(move || {
                let r =
                    op_application::media::guarded("open project", || Editor::load_project(&path));
                let _ = tx.send(r);
            });
        match spawned {
            Ok(_) => {
                self.opening = Some(Opening {
                    path: p.to_path_buf(),
                    rx,
                    started: Instant::now(),
                    recovered,
                })
            }
            Err(e) => self.ed.error(format!("{}: {e}", p.display())),
        }
    }

    fn poll_opening(&mut self) {
        let Some(o) = &self.opening else { return };
        let result = match o.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("the project could not be read".into())
            }
        };
        let o = self.opening.take().unwrap();
        match result {
            Ok(loaded) => {
                self.ed.install(loaded);
                self.after_open();
                if o.recovered {
                    // the copy is not the user's file: keep it unsaved until saved by name
                    self.ed.path = None;
                    self.ed.history.clear();
                    self.ed
                        .info(t("Recovered project opened; save it to keep it"));
                }
                if let Some(r) = self.ed.last_import_report.take() {
                    self.dialogs.push(Dialog::ImportReport(Box::new(r)));
                }
                log::info!("project ready in {} ms", o.started.elapsed().as_millis());
            }
            Err(e) => self.ed.error(format!("{}: {e}", o.path.display())),
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

    /// The export shown in the Program Monitor: the newest one still running.
    /// Graphics > Captions > Import Captions File: SRT or WebVTT into caption clips.
    fn import_captions(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title(t("Import Captions File..."))
            .add_filter(t("Captions"), &["srt", "vtt"])
            .pick_file()
        else {
            return;
        };
        let opts = self.caption_options.clone();
        match self.ed.import_captions(&path, &opts) {
            Ok(n) => self.ed.info(format!("Created {n} captions")),
            Err(e) => self.ed.error(e),
        }
    }

    /// Graphics > Captions > Export Captions File: SRT (or WebVTT by extension).
    fn export_captions(&mut self) {
        let name = self
            .ed
            .active_seq()
            .map(|q| q.name.clone())
            .unwrap_or_else(|| "Captions".into());
        let Some(path) = rfd::FileDialog::new()
            .set_title(t("Export Captions File..."))
            .set_file_name(format!("{name}.srt"))
            .add_filter("SubRip (.srt)", &["srt"])
            .add_filter("WebVTT (.vtt)", &["vtt"])
            .save_file()
        else {
            return;
        };
        match self.ed.export_captions(&path) {
            Ok(n) => self.ed.info(format!("Exported {n} captions")),
            Err(e) => self.ed.error(e),
        }
    }

    /// Media assets of the selected Project panel items, or of the selected clips when the
    /// timeline has the focus.
    pub fn selected_assets(&self) -> Vec<AssetId> {
        let mut out: Vec<AssetId> = Vec::new();
        if self.focus == Focus::Timeline
            && let Some(seq) = self.ed.active_seq()
        {
            for c in seq.video.iter().chain(&seq.audio).flat_map(|t| &t.clips) {
                if self.ed.selection.clips.contains(&c.id)
                    && let ClipSource::Asset { asset, .. } = c.source
                    && !out.contains(&asset)
                {
                    out.push(asset);
                }
            }
        }
        if out.is_empty() {
            for id in &self.ed.items {
                if let Some(ItemKind::Media { asset, .. }) =
                    self.ed.project.item(*id).map(|i| i.kind.clone())
                    && !out.contains(&asset)
                {
                    out.push(asset);
                }
            }
        }
        out
    }

    pub fn running_export(&self) -> Option<&op_application::ExportJob> {
        self.ed.exports.iter().rev().find(|j| !j.finished())
    }

    /// The running export's preview frame as a texture (updated as the export advances).
    pub fn export_preview(&mut self) -> Option<(egui::TextureId, egui::Vec2)> {
        let img = self.running_export()?.progress.lock().preview.clone()?;
        let key = Arc::as_ptr(&img) as usize as u64;
        let stale = self
            .export_ui
            .preview
            .as_ref()
            .is_none_or(|(k, _)| *k != key);
        if stale {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width as usize, img.height as usize],
                &img.rgba,
            );
            match &mut self.export_ui.preview {
                Some((k, h)) => {
                    h.set(color, egui::TextureOptions::LINEAR);
                    *k = key;
                }
                None => {
                    let h = self.ctx.load_texture(
                        "export-preview",
                        color,
                        egui::TextureOptions::LINEAR,
                    );
                    self.export_ui.preview = Some((key, h));
                }
            }
        }
        let (_, h) = self.export_ui.preview.as_ref()?;
        Some((h.id(), egui::vec2(img.width as f32, img.height as f32)))
    }

    /// An export is running: the Program Monitor shows the frames it renders.
    pub fn exporting(&self) -> bool {
        self.running_export().is_some()
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
        // a failure inside one panel is contained: the panel is drawn again next frame and the
        // program keeps running (the failure is logged with a crash report)
        let drawn = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match p {
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
        }));
        if drawn.is_err() {
            log::error!("the {:?} panel failed and was restored", p);
            self.tl.cancel();
            self.drag = None;
            self.ed.error(tf(
                "The {} panel had a problem and was restored. Your work is safe.",
                &[&t(p.title())],
            ));
        }
        // the active tab is underlined: blue in the focused panel, gray elsewhere
        if let Some(tab) = self.tab_rects.get(&p) {
            let focused = self.focused_panel == Some(p);
            let y = tab.max.y - 1.5;
            ui.ctx()
                .layer_painter(ui.layer_id())
                .with_clip_rect(tab.expand(2.0))
                .line_segment(
                    [
                        egui::pos2(tab.min.x + 6.0, y),
                        egui::pos2(tab.max.x - 6.0, y),
                    ],
                    Stroke::new(
                        2.0,
                        if focused {
                            theme::ACCENT
                        } else {
                            theme::TEXT_DIM
                        },
                    ),
                );
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
        self.poll_opening();
        self.poll_pastes();
        self.minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        self.tab_rects.clear();
        let busy = self.ed.tick() || self.opening.is_some();
        self.apply_performance_mode(ctx);
        if self.ed.is_playing() {
            // redraw when the next frame of the sequence is due (at most 60 times a second)
            // rather than at the display's refresh rate: a 144 Hz screen would otherwise
            // redraw the whole interface 144 times a second for a 30 fps video
            ctx.request_repaint_after(self.next_frame_delay());
        } else if busy {
            let every = if self.ed.prefs.performance_mode {
                200
            } else {
                60
            };
            ctx.request_repaint_after(Duration::from_millis(every));
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
        let exporting = self.ed.exports.iter().any(|j| !j.finished());
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && exporting {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if !self
                .dialogs
                .iter()
                .any(|d| matches!(d, Dialog::ExportRunning))
            {
                self.dialogs.push(Dialog::ExportRunning);
            }
        } else if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
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
                    .or_else(|| op_core::presets::find(id).map(|p| tn(p.name)))
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
        if self.quit && self.ed.exports.iter().any(|j| !j.finished()) {
            self.quit = false;
            if !self
                .dialogs
                .iter()
                .any(|d| matches!(d, Dialog::ExportRunning))
            {
                self.dialogs.push(Dialog::ExportRunning);
            }
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
                // exports sit in the bottom right corner, like background tasks
                self.export_status(ui);
                self.proxy_status(ui);
                self.caption_status(ui);
                ui.label(
                    RichText::new(self.gpu.description())
                        .color(theme::TEXT_DIM)
                        .size(11.0),
                );
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

    /// Automatic captions in progress: stage, progress and Cancel.
    fn caption_status(&mut self, ui: &mut Ui) {
        let Some(job) = &self.ed.captioning else {
            return;
        };
        let p = job.progress();
        if ui.small_button(t("Cancel")).clicked() {
            job.cancel();
        }
        let stage = match p.stage {
            op_application::captions::CaptionStage::Downloading => t("Downloading speech model"),
            op_application::captions::CaptionStage::MixingAudio => t("Preparing audio"),
            op_application::captions::CaptionStage::Transcribing => t("Transcribing"),
        };
        ui.add(
            egui::ProgressBar::new(p.fraction)
                .desired_width(140.0)
                .text(format!("{:.0} %", p.fraction * 100.0)),
        );
        ui.label(RichText::new(stage).size(12.0));
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    /// Proxy creation in progress: file, progress and Cancel.
    fn proxy_status(&mut self, ui: &mut Ui) {
        if !self.ed.proxies.busy() {
            return;
        }
        let p = self.ed.proxies.progress();
        if ui.small_button(t("Cancel")).clicked() {
            self.ed.proxies.cancel();
        }
        let n = (p.done + 1).min(p.total.max(1));
        ui.add(
            egui::ProgressBar::new(p.fraction)
                .desired_width(140.0)
                .text(format!("{:.0} %", p.fraction * 100.0)),
        )
        .on_hover_text(p.current.clone().unwrap_or_default());
        ui.label(RichText::new(tf("Creating proxies ({}/{})", &[&n, &p.total])).size(12.0));
        ui.ctx().request_repaint_after(Duration::from_millis(200));
    }

    /// One entry per export, right to left: progress with pause and cancel while it runs (its
    /// frames show in the Program Monitor), then the result until it expires or is closed.
    fn export_status(&mut self, ui: &mut Ui) {
        let mut cancel = Vec::new();
        let mut pause = Vec::new();
        let mut folder = None;
        let ui_state = &mut self.export_ui;
        for (i, job) in self.ed.exports.iter().enumerate().rev() {
            if ui_state.dismissed.contains(&i) {
                continue;
            }
            let p = job.progress.lock().clone();
            let name = job
                .settings
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if !p.done {
                if ui.small_button(t("Cancel")).clicked() {
                    cancel.push(i);
                }
                let label = if p.paused { t("Resume") } else { t("Pause") };
                if ui.small_button(label).clicked() {
                    pause.push((i, !p.paused));
                }
                let text = if p.preparing {
                    t("Preparing audio...").to_string()
                } else if p.paused {
                    format!("{}  {:.0} %", t("Paused"), p.fraction() * 100.0)
                } else {
                    let eta = p.remaining.map(widgets::clock).unwrap_or_default();
                    format!("{:.0} %  {eta}", p.fraction() * 100.0)
                };
                ui.add(
                    egui::ProgressBar::new(p.fraction())
                        .desired_width(170.0)
                        .text(text),
                )
                .on_hover_ui(|ui| export_details(ui, job, &p));
                ui.label(RichText::new(tf("Exporting {}", &[&name])).size(12.0));
                ui.ctx().request_repaint_after(Duration::from_millis(200));
                continue;
            }
            let since = *ui_state.finished_at.entry(i).or_insert_with(Instant::now);
            if let Some(e) = &p.error {
                if ui.small_button("x").on_hover_text(t("Close")).clicked() {
                    ui_state.dismissed.insert(i);
                }
                let mut details = translate_status(&format!("Export failed: {e}"));
                if e.starts_with("cannot write ") && cfg!(windows) {
                    details.push_str("\n\n");
                    details.push_str(t(
                        "Windows may be blocking this folder (Controlled folder access). Allow OpenPremier in Windows Security or choose another folder.",
                    ));
                }
                ui.label(
                    RichText::new(tf("Export of {} failed", &[&name]))
                        .size(12.0)
                        .color(theme::ERROR),
                )
                .on_hover_text(details);
            } else if p.cancelled || since.elapsed() > Duration::from_secs(15) {
                ui_state.dismissed.insert(i);
            } else {
                if ui.small_button(t("Open Folder")).clicked() {
                    folder = job.settings.path.parent().map(|d| d.to_path_buf());
                }
                ui.label(RichText::new(tf("Exported {}", &[&name])).size(12.0))
                    .on_hover_text(job.settings.path.display().to_string());
                ui.ctx().request_repaint_after(Duration::from_secs(1));
            }
        }
        for i in cancel {
            self.ed.exports[i].cancel();
        }
        for (i, paused) in pause {
            self.ed.exports[i].set_paused(paused);
        }
        if let Some(dir) = folder {
            open_folder(&dir);
        }
    }
}

/// Details of a running export, shown over its progress bar.
fn export_details(ui: &mut Ui, job: &op_application::ExportJob, p: &op_application::Progress) {
    ui.label(RichText::new(job.settings.path.display().to_string()).strong());
    ui.label(tf("Frame {} of {}", &[&p.frame, &p.total]));
    ui.label(tf("Elapsed: {}", &[&widgets::clock(p.elapsed)]));
    if let Some(r) = p.remaining {
        ui.label(tf("Remaining: {}", &[&widgets::clock(r)]));
    }
    if p.fps > 0.0 {
        ui.label(format!("{:.1} fps", p.fps));
    }
    if !p.encoder.is_empty() {
        ui.label(RichText::new(&p.encoder).color(theme::TEXT_DIM));
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
    // messages with one variable part, from the editing engine and the exporter
    const PATTERNS: [(&str, &str, &str); 14] = [
        ("track ", " is locked", "Track {} is locked"),
        ("clips would overlap on ", "", "Clips would overlap on {}"),
        ("not enough media: ", "", "Not enough media: {}"),
        ("not found: ", "", "Not found: {}"),
        (
            "Export failed: cannot write ",
            "",
            "Export failed: cannot write {}",
        ),
        ("Export failed: ", "", "Export failed: {}"),
        ("Created ", " captions", "Created {} captions"),
        ("Exported ", " captions", "Exported {} captions"),
        (
            "Style applied to ",
            " captions",
            "Style applied to {} captions",
        ),
        ("Captions not created: ", "", "Captions not created: {}"),
        ("Exported ", "", "Exported {}"),
        ("Proxies ready (", ")", "Proxies ready ({})"),
        ("Proxy not created: ", "", "Proxy not created: {}"),
        (
            "This project was saved by a newer version (format ",
            ").",
            "This project was saved by a newer version (format {}).",
        ),
    ];
    for (prefix, suffix, fmt) in PATTERNS {
        if let Some(rest) = text.strip_prefix(prefix)
            && let Some(mid) = rest.strip_suffix(suffix)
        {
            return tf(fmt, &[&tn(mid)]);
        }
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
        let mut job = egui::text::LayoutJob::default();
        job.append(
            &text,
            20.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(12.5),
                ..Default::default()
            },
        );
        job.into()
    }

    fn on_tab_button(&mut self, tab: &mut Panel, response: &egui::Response) {
        let r = response.rect;
        self.s.tab_rects.insert(*tab, r);
        let icon = egui::Rect::from_center_size(
            egui::pos2(r.min.x + 17.0, r.center().y),
            egui::vec2(14.0, 14.0),
        );
        let color = if response.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        icons::draw(
            &response
                .ctx
                .layer_painter(response.layer_id)
                .with_clip_rect(r),
            icon,
            panel_icon(*tab),
            color,
        );
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut Panel, _path: egui_dock::NodePath) {
        if ui.button(t("Reset to Saved Layout")).clicked() {
            self.s.command("cmd.window.workspace.revert");
            ui.close();
        }
        let _ = tab;
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
    // panel headers: flat, with room between the names and a quiet line under the bar
    d.tab_bar.bg_fill = theme::PANEL_DARK;
    d.tab_bar.height = 30.0;
    d.tab_bar.hline_color = theme::BG;
    d.tab_bar.inner_margin = egui::Margin::symmetric(6, 0);
    d.tab.spacing = 4.0;
    d.tab.minimum_width = Some(64.0);
    d.tab.hline_below_active_tab_name = false;
    d.tab.tab_body.bg_fill = theme::PANEL;
    d.tab.tab_body.stroke = Stroke::NONE;
    d.tab.tab_body.inner_margin = egui::Margin::same(0);
    d.tab.tab_body.hidden_tab_bar_drag_height = Some(8.0);
    for s in [
        &mut d.tab.active,
        &mut d.tab.focused,
        &mut d.tab.active_with_kb_focus,
        &mut d.tab.focused_with_kb_focus,
    ] {
        s.bg_fill = theme::PANEL_DARK;
        s.text_color = theme::TEXT_BRIGHT;
        s.outline_color = theme::PANEL_DARK;
        s.corner_radius = egui::CornerRadius::ZERO;
    }
    for s in [&mut d.tab.inactive, &mut d.tab.inactive_with_kb_focus] {
        s.bg_fill = theme::PANEL_DARK;
        s.text_color = theme::TEXT_DIM;
        s.outline_color = theme::PANEL_DARK;
        s.corner_radius = egui::CornerRadius::ZERO;
    }
    d.tab.hovered.bg_fill = theme::PANEL_DARK;
    d.tab.hovered.text_color = theme::TEXT;
    d.tab.hovered.outline_color = theme::PANEL_DARK;
    d.buttons.show_tab_bar_color = theme::LINE;
    d.buttons.show_tab_bar_active_color = theme::ACCENT;
    d.separator.width = 3.0;
    d.separator.extra = 40.0;
    d.separator.color_idle = theme::BG;
    d.separator.color_hovered = theme::ACCENT_DIM;
    d.separator.color_dragged = theme::ACCENT;
    d
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.s.begin_frame(&ctx);
        if self.s.minimized {
            // nothing is visible: keep working (playback, exports) but do not lay out panels
            // at zero size, which would disturb their remembered sizes
            self.s.end_frame(&ctx);
            return;
        }
        self.tend_dock(&ctx);
        if self.s.dialogs.is_empty() && self.s.opening.is_none() {
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
                .show_close_buttons(false)
                .hidable_tab_bars(true)
                .show_leaf_collapse_buttons(false)
                .show_leaf_close_all_buttons(false)
                .show_inside(ui, &mut viewer);
        });
        let shown = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::dialogs::show(&mut self.s, &ctx);
        }));
        if shown.is_err() {
            log::error!("a dialog failed and was closed");
            self.s.dialogs.clear();
            self.s.ed.error(t(
                "A dialog had a problem and was closed. Your work is safe.",
            ));
        }
        crate::dialogs::opening(&mut self.s, &ctx);
        self.apply_dock_action();
        self.s.end_frame(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::from_rgb(0x1d, 0x1d, 0x20).to_normalized_gamma_f32()
    }

    fn on_exit(&mut self) {
        log::info!("closing");
        self.s.ed.stop();
        for job in &self.s.ed.exports {
            if !job.finished() {
                job.cancel();
            }
        }
        self.save_layouts();
        let _ = self.s.ed.prefs.save(&self.s.ed.dirs);
        op_application::recovery::forget();
        autosave::end_session(&self.s.ed.dirs);
    }
}
