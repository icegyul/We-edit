//! Panels and workspaces. A workspace is a complete docking layout
//! (UI-WS-001); user changes are kept per workspace and can be reset.

use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Panel {
    Project,
    Source,
    Program,
    Timeline,
    EffectControls,
    Effects,
    History,
    Tools,
    Meters,
    AudioMixer,
    Markers,
    Info,
    Lumetri,
    Scopes,
    Graphics,
    Captions,
}

impl Panel {
    pub const ALL: [Panel; 16] = [
        Panel::Project,
        Panel::Source,
        Panel::Program,
        Panel::Timeline,
        Panel::EffectControls,
        Panel::Effects,
        Panel::History,
        Panel::Tools,
        Panel::Meters,
        Panel::AudioMixer,
        Panel::Markers,
        Panel::Info,
        Panel::Lumetri,
        Panel::Scopes,
        Panel::Graphics,
        Panel::Captions,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Panel::Project => "Project",
            Panel::Source => "Source",
            Panel::Program => "Program",
            Panel::Timeline => "Timeline",
            Panel::EffectControls => "Effect Controls",
            Panel::Effects => "Effects",
            Panel::History => "History",
            Panel::Tools => "Tools",
            Panel::Meters => "Audio Meters",
            Panel::AudioMixer => "Audio Track Mixer",
            Panel::Markers => "Markers",
            Panel::Info => "Info",
            Panel::Lumetri => "Lumetri Color",
            Panel::Scopes => "Lumetri Scopes",
            Panel::Graphics => "Essential Graphics",
            Panel::Captions => "Captions",
        }
    }

    /// The `uif.window.*` shortcut command that focuses this panel.
    pub fn from_window_command(cmd: &str) -> Option<Panel> {
        Some(match cmd {
            "uif.window.Projects" => Panel::Project,
            "uif.window.Source Monitors" => Panel::Source,
            "uif.window.Timelines" => Panel::Timeline,
            "uif.window.Program Monitors" => Panel::Program,
            "uif.window.Effect Controls" => Panel::EffectControls,
            "uif.window.Audio Mixers" => Panel::AudioMixer,
            "uif.window.Effects" => Panel::Effects,
            "uif.window.Audio Clip Mixer" => Panel::AudioMixer,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Workspace {
    Assembly,
    Editing,
    Color,
    Effects,
    Audio,
    Graphics,
}

impl Workspace {
    pub const ALL: [Workspace; 6] = [
        Workspace::Assembly,
        Workspace::Editing,
        Workspace::Color,
        Workspace::Effects,
        Workspace::Audio,
        Workspace::Graphics,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Workspace::Assembly => "Assembly",
            Workspace::Editing => "Editing",
            Workspace::Color => "Color",
            Workspace::Effects => "Effects",
            Workspace::Audio => "Audio",
            Workspace::Graphics => "Graphics",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Workspace::Assembly => "assembly",
            Workspace::Editing => "editing",
            Workspace::Color => "color",
            Workspace::Effects => "effects",
            Workspace::Audio => "audio",
            Workspace::Graphics => "graphics",
        }
    }

    /// The factory layout.
    pub fn layout(self) -> DockState<Panel> {
        use Panel::*;
        let mut s = DockState::new(vec![Timeline]);
        let t = s.main_surface_mut();
        match self {
            Workspace::Editing => {
                // monitors on top, project / tools / timeline / meters below
                let [bottom, top] = t.split_above(NodeIndex::root(), 0.5, vec![Program]);
                let [_program, _source] = t.split_left(
                    top,
                    0.5,
                    vec![Source, EffectControls, AudioMixer, Graphics, Captions],
                );
                let [timeline, _project] =
                    t.split_left(bottom, 0.3, vec![Project, Effects, Markers, History, Info]);
                let [timeline, _tools] = t.split_left(timeline, 0.02, vec![Tools]);
                t.split_right(timeline, 0.965, vec![Meters]);
            }
            Workspace::Assembly => {
                let [right, _project] = t.split_left(
                    NodeIndex::root(),
                    0.42,
                    vec![Project, Effects, Markers, History],
                );
                let [timeline, _program] = t.split_above(right, 0.5, vec![Program, Source]);
                let [timeline, _tools] = t.split_left(timeline, 0.02, vec![Tools]);
                t.split_right(timeline, 0.955, vec![Meters]);
            }
            Workspace::Color => {
                let [left, _lumetri] =
                    t.split_right(NodeIndex::root(), 0.76, vec![Lumetri, EffectControls]);
                let [bottom, top] = t.split_above(left, 0.52, vec![Program]);
                t.split_left(top, 0.45, vec![Scopes, Source]);
                let [timeline, _project] =
                    t.split_left(bottom, 0.25, vec![Project, Effects, History]);
                t.split_right(timeline, 0.955, vec![Meters]);
            }
            Workspace::Effects => {
                let [left, _fx] =
                    t.split_right(NodeIndex::root(), 0.78, vec![Effects, Lumetri, Graphics]);
                let [bottom, top] = t.split_above(left, 0.5, vec![Program]);
                t.split_left(top, 0.5, vec![EffectControls, Source]);
                let [timeline, _project] =
                    t.split_left(bottom, 0.25, vec![Project, History, Markers]);
                let [timeline, _tools] = t.split_left(timeline, 0.02, vec![Tools]);
                t.split_right(timeline, 0.955, vec![Meters]);
            }
            Workspace::Audio => {
                let [bottom, top] = t.split_above(NodeIndex::root(), 0.5, vec![Program]);
                t.split_left(top, 0.55, vec![AudioMixer, EffectControls, Source]);
                let [timeline, _project] =
                    t.split_left(bottom, 0.25, vec![Project, Effects, Markers]);
                t.split_right(timeline, 0.93, vec![Meters]);
            }
            Workspace::Graphics => {
                let [left, _g] = t.split_right(
                    NodeIndex::root(),
                    0.77,
                    vec![Graphics, Captions, EffectControls],
                );
                let [bottom, top] = t.split_above(left, 0.52, vec![Program]);
                t.split_left(top, 0.4, vec![Source, Project]);
                let [timeline, _tools] = t.split_left(bottom, 0.02, vec![Tools]);
                t.split_right(timeline, 0.955, vec![Meters]);
            }
        }
        // the tool strip is a slim column without a tab, like the tools of other editors
        if let Some((node, _)) = s.main_surface().find_tab(&Tools) {
            s.main_surface_mut()[node].set_tab_bar_hidden(true);
        }
        s
    }
}

/// Saved layouts carry this version in their file name; a new factory layout replaces layouts
/// saved by older versions.
pub const LAYOUT_VERSION: u32 = 3;

/// Layout JSON as saved: rectangles that were never laid out are NaN, which JSON writes as
/// null; they are restored as zero (egui_dock recomputes them on the next frame).
pub fn to_json(d: &DockState<Panel>) -> Option<String> {
    serde_json::to_string(d).ok()
}

pub fn from_json(text: &str) -> Option<DockState<Panel>> {
    fn fix(v: &mut serde_json::Value) {
        match v {
            serde_json::Value::Object(m) => {
                for (k, x) in m.iter_mut() {
                    if x.is_null() && (k == "x" || k == "y") {
                        *x = serde_json::Value::from(0.0);
                    } else {
                        fix(x);
                    }
                }
            }
            serde_json::Value::Array(a) => a.iter_mut().for_each(fix),
            _ => {}
        }
    }
    let mut v: serde_json::Value = serde_json::from_str(text).ok()?;
    fix(&mut v);
    serde_json::from_value(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workspace_has_the_core_panels() {
        for w in Workspace::ALL {
            let s = w.layout();
            let tabs: Vec<Panel> = s.iter_all_tabs().map(|(_, t)| *t).collect();
            for p in [Panel::Timeline, Panel::Program] {
                assert!(tabs.contains(&p), "{w:?} lacks {p:?}");
            }
            let json = to_json(&s).unwrap();
            let back = from_json(&json).expect("layout round trip");
            assert_eq!(back.iter_all_tabs().count(), tabs.len());
            if let Some((node, _)) = back.main_surface().find_tab(&Panel::Tools) {
                assert!(
                    back.main_surface()[node].is_tab_bar_hidden(),
                    "{w:?} tools tab bar"
                );
            }
        }
    }
}
