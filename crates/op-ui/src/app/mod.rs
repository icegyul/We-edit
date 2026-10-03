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

mod dock;
mod files;
mod menus;
mod paste;
mod status;

use dock::*;
pub use menus::proxy_menu;
use menus::*;

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
    pub caption_translate: bool,
    pub caption_language: usize,
    performance_applied: Option<op_application::performance::ProfileSettings>,
    /// This computer, checked once at start (for the recommended performance profile).
    pub hardware: op_application::performance::Hardware,
    /// Effect pipelines are still being compiled in idle frames.
    warming: bool,
    pub loop_playback: bool,
    pub workspace: Workspace,
    pub program: monitor::MonitorView,
    pub source: monitor::MonitorView,
    pub tl: timeline::TimelineView,
    pub proj: project_panel::ProjectView,
    pub fx: effects_panel::EffectsView,
    pub ec: effect_controls::EcView,
    /// Captions panel: style edits go to every caption (else to the selected ones).
    pub captions_all: bool,
    /// Listening for AI assistants (Preferences > AI Assistants).
    pub assistants: Option<op_mcp::live::Server>,
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
        crate::fonts::install(&cc.egui_ctx, prefs.ui_font != "classic");
        egui_extras_install(&cc.egui_ctx);
        let recovery = autosave::begin_session(&dirs);
        let ed = Editor::new(dirs, prefs, true);
        let workspace = std::fs::read_to_string(ed.dirs.workspaces().join("current.txt"))
            .ok()
            .and_then(|k| Workspace::ALL.into_iter().find(|w| w.key() == k.trim()))
            .unwrap_or(Workspace::Editing);
        let dock = load_layout(&ed.dirs, workspace).unwrap_or_else(|| workspace.layout());
        // this computer, for the recommended performance profile
        let hardware = op_application::performance::Hardware::probe(
            &gpu.info.name,
            match gpu.info.device_type {
                wgpu::DeviceType::DiscreteGpu => op_application::performance::GpuKind::Discrete,
                wgpu::DeviceType::IntegratedGpu => op_application::performance::GpuKind::Integrated,
                wgpu::DeviceType::Cpu => op_application::performance::GpuKind::Software,
                _ => op_application::performance::GpuKind::Unknown,
            },
        );
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
            caption_language: crate::dialogs::default_caption_language(),
            caption_translate: false,
            performance_applied: None,
            hardware,
            warming: true,
            loop_playback: false,
            workspace,
            program: monitor::MonitorView::new(Monitor::Program),
            source: monitor::MonitorView::new(Monitor::Source),
            tl: timeline::TimelineView::default(),
            proj: project_panel::ProjectView::default(),
            fx: effects_panel::EffectsView::default(),
            ec: effect_controls::EcView::default(),
            captions_all: true,
            assistants: None,
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
        s.renderer.warm_up_core();
        s.open_or_import(opts.open);
        if let Some(p) = recovery {
            s.dialogs.push(Dialog::Recover(p));
        }
        // first start (and the first start after updating from a version without profiles):
        // offer the profile this computer handles comfortably
        if !s.ed.prefs.performance_setup_done {
            let choice = s.hardware.recommended();
            s.dialogs.push(Dialog::PerformanceSetup {
                manual: false,
                choice,
            });
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
            "op.file.linkmedia" => match Dialog::link_media(self) {
                Some(d) => self.dialogs.push(d),
                None => self.ed.info(t("All media files are linked").to_string()),
            },
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
            "cmd.edit.removeattributes" => {
                if self.ed.selection.clips.is_empty() {
                    self.ed.error(t("Select the clips first"));
                } else {
                    self.dialogs
                        .push(Dialog::RemoveAttributes(op_timeline::Attributes::default()));
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
        let cap = self.ed.prefs.performance().playback_fps_cap.unwrap_or(60);
        let max = Duration::from_secs_f64(1.0 / cap.max(1) as f64);
        let Some(seq) = self.ed.active_seq() else {
            return max;
        };
        let fd = seq.rate().frame_duration().seconds().max(1e-3);
        let speed = self.ed.transport.speed.abs().max(0.01);
        let t = self.ed.playhead().seconds();
        let next = ((t / fd).floor() + 1.0) * fd;
        Duration::from_secs_f64(((next - t) / speed).max(0.001)).min(max)
    }

    /// Interface animations follow the performance profile.
    fn apply_performance_mode(&mut self, ctx: &egui::Context) {
        let st = self.ed.prefs.performance();
        if self.performance_applied == Some(st) {
            return;
        }
        self.performance_applied = Some(st);
        ctx.all_styles_mut(|s| {
            s.animation_time = st.animation_time;
            s.scroll_animation = if st.smooth_scroll {
                egui::style::ScrollAnimation::default()
            } else {
                egui::style::ScrollAnimation::none()
            };
        });
        log::info!(
            "interface for the {} profile{}",
            self.ed.prefs.profile().label(),
            if self.ed.prefs.performance_custom.is_some() {
                " (custom)"
            } else {
                ""
            }
        );
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
            Panel::Captions => crate::captions_panel::captions(self, ui),
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
            // a tab wider than its narrow panel is cut by the tab bar: so is its underline,
            // which must never reach into the neighbouring panel
            let column = egui::Rect::from_x_y_ranges(rect.x_range(), tab.expand(2.0).y_range());
            ui.ctx()
                .layer_painter(ui.layer_id())
                .with_clip_rect(tab.expand(2.0).intersect(column))
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

    /// Starts or stops listening for assistants as the preference says, and runs their tool
    /// calls between two frames.
    fn serve_assistants(&mut self, ctx: &egui::Context) {
        if self.ed.prefs.assistant_control != self.assistants.is_some() {
            if self.ed.prefs.assistant_control {
                let wake = ctx.clone();
                match op_mcp::live::Server::start(&self.ed.dirs.config, move || {
                    wake.request_repaint()
                }) {
                    Ok(s) => self.assistants = Some(s),
                    Err(e) => {
                        self.ed.prefs.assistant_control = false;
                        self.ed.error(format!("Assistants cannot connect: {e}"));
                    }
                }
            } else {
                self.assistants = None;
            }
        }
        let requests = self
            .assistants
            .as_ref()
            .map(|s| s.pending())
            .unwrap_or_default();
        for req in requests {
            let opened = self.ed.path.clone();
            let result = op_mcp::tools::call_live(&mut self.ed, &req.name, &req.args);
            if self.ed.path != opened || req.name == "new_project" {
                self.after_open();
            }
            req.answer(result);
            ctx.request_repaint();
        }
    }

    fn begin_frame(&mut self, ctx: &egui::Context) {
        self.poll_opening();
        self.poll_pastes();
        self.minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        self.tab_rects.clear();
        let busy = self.ed.tick() || self.opening.is_some();
        self.serve_assistants(ctx);
        self.apply_performance_mode(ctx);
        let profile = self.ed.prefs.performance();
        // the remaining effect pipelines compile two per idle frame from Balanced up; the
        // lighter profiles compile each one on first use and never spend the time up front
        if self.warming && profile.warm_up && !self.ed.is_playing() && self.dialogs.is_empty() {
            self.warming = self.renderer.warm_up_step(2);
            if self.warming {
                ctx.request_repaint_after(Duration::from_millis(15));
            } else {
                log::info!("all effect pipelines are ready");
            }
        }
        if self.ed.is_playing() {
            // redraw when the next frame of the sequence is due, at most as often as the
            // profile allows, rather than at the display's refresh rate: a 144 Hz screen would
            // otherwise redraw the whole interface 144 times a second for a 30 fps video.
            // Maximum Quality redraws every display frame for the smoothest playhead.
            if profile.playback_fps_cap.is_none() {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(self.next_frame_delay());
            }
        } else if busy {
            ctx.request_repaint_after(profile.busy_repaint);
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
}

/// Font fallbacks: the default fonts cover Latin text; symbols used in menus come from them too.
fn egui_extras_install(ctx: &egui::Context) {
    ctx.options_mut(|o| {
        o.zoom_with_keyboard = false;
    });
}

// --------------------------------------------------------------------------------- docking

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
