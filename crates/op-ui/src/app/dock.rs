//! Docked panels: tab titles and icons, layouts and the dock's look.

use super::*;

pub(crate) fn layout_path(dirs: &Dirs, w: Workspace) -> PathBuf {
    dirs.workspaces().join(format!(
        "{}.v{}.json",
        w.key(),
        crate::workspace::LAYOUT_VERSION
    ))
}

/// Built-in texts of the docking system in the interface language.
pub(crate) fn dock_translations() -> egui_dock::Translations {
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
pub(crate) fn panel_icon(p: Panel) -> Icon {
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
        Panel::Captions => Icon::List,
    }
}

pub(crate) fn load_layout(dirs: &Dirs, w: Workspace) -> Option<DockState<Panel>> {
    let text = std::fs::read_to_string(layout_path(dirs, w)).ok()?;
    let d = crate::workspace::from_json(&text)?;
    // a layout without the timeline is from an incompatible version
    d.find_tab(&Panel::Timeline).is_some().then_some(d)
}

pub(crate) struct Viewer<'a> {
    pub(crate) s: &'a mut State,
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
                font_id: egui::FontId::new(12.5, crate::fonts::strong()),
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

pub(crate) fn dock_style(style: &egui::Style) -> egui_dock::Style {
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
