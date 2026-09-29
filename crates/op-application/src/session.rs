//! Session state: tools, selection, targets, monitors and transport. None of this is project
//! content (DM-TL-004); it is saved as view state next to the project.

use op_core::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Tool {
    #[default]
    Selection,
    TrackSelectForward,
    TrackSelectBackward,
    Ripple,
    Roll,
    RateStretch,
    Razor,
    Slip,
    Slide,
    Pen,
    Hand,
    Zoom,
    Type,
}

impl Tool {
    pub const ALL: [Tool; 13] = [
        Tool::Selection,
        Tool::TrackSelectForward,
        Tool::TrackSelectBackward,
        Tool::Ripple,
        Tool::Roll,
        Tool::RateStretch,
        Tool::Razor,
        Tool::Slip,
        Tool::Slide,
        Tool::Pen,
        Tool::Hand,
        Tool::Zoom,
        Tool::Type,
    ];

    pub fn command(self) -> &'static str {
        match self {
            Tool::Selection => "cmd.tools.01pointer",
            Tool::TrackSelectForward => "cmd.tools.02trackselectforward",
            Tool::TrackSelectBackward => "cmd.tools.02_5trackselectbackward",
            Tool::Ripple => "cmd.tools.03ripple",
            Tool::Roll => "cmd.tools.04roll",
            Tool::RateStretch => "cmd.tools.05ratestretch",
            Tool::Razor => "cmd.tools.06razor",
            Tool::Slip => "cmd.tools.07slip",
            Tool::Slide => "cmd.tools.08slide",
            Tool::Pen => "cmd.tools.09pen",
            Tool::Hand => "cmd.tools.10hand",
            Tool::Zoom => "cmd.tools.11zoom",
            Tool::Type => "cmd.tools.12text",
        }
    }

    pub fn from_command(cmd: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.command() == cmd)
    }

    pub fn key(self) -> &'static str {
        match self {
            Tool::Selection => "tool.selection",
            Tool::TrackSelectForward => "tool.track_select_forward",
            Tool::TrackSelectBackward => "tool.track_select_backward",
            Tool::Ripple => "tool.ripple",
            Tool::Roll => "tool.roll",
            Tool::RateStretch => "tool.rate_stretch",
            Tool::Razor => "tool.razor",
            Tool::Slip => "tool.slip",
            Tool::Slide => "tool.slide",
            Tool::Pen => "tool.pen",
            Tool::Hand => "tool.hand",
            Tool::Zoom => "tool.zoom",
            Tool::Type => "tool.type",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub clips: Vec<ClipId>,
    pub transitions: Vec<TransitionId>,
    /// An empty space selected on a track (for Ripple Delete).
    pub gap: Option<(TrackRef, SeqTime)>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.clips.is_empty() && self.transitions.is_empty() && self.gap.is_none()
    }

    pub fn clear(&mut self) {
        *self = Selection::default();
    }

    pub fn only(clips: Vec<ClipId>) -> Selection {
        Selection {
            clips,
            ..Default::default()
        }
    }
}

/// Per-sequence session state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeqView {
    pub video_targets: Vec<bool>,
    pub audio_targets: Vec<bool>,
    /// Source patching: destination track per source stream.
    pub video_patch: Option<usize>,
    pub audio_patch: Vec<Option<usize>>,
    /// Timeline zoom: pixels per second.
    pub zoom: f64,
    /// Timeline scroll: first visible time in seconds.
    pub scroll: f64,
    pub scroll_y: f32,
}

impl Default for SeqView {
    fn default() -> Self {
        SeqView {
            video_targets: vec![true],
            audio_targets: vec![true],
            video_patch: Some(0),
            audio_patch: vec![Some(0), Some(1)],
            zoom: 40.0,
            scroll: 0.0,
            scroll_y: 0.0,
        }
    }
}

impl SeqView {
    pub fn is_targeted(&self, r: TrackRef) -> bool {
        let v = match r.kind {
            TrackKind::Video => &self.video_targets,
            TrackKind::Audio => &self.audio_targets,
        };
        v.get(r.index).copied().unwrap_or(false)
    }

    pub fn set_targeted(&mut self, r: TrackRef, on: bool) {
        let v = match r.kind {
            TrackKind::Video => &mut self.video_targets,
            TrackKind::Audio => &mut self.audio_targets,
        };
        if v.len() <= r.index {
            v.resize(r.index + 1, false);
        }
        v[r.index] = on;
    }

    /// Targeted tracks that exist in the sequence.
    pub fn targets(&self, seq: &Sequence) -> Vec<TrackRef> {
        seq.all_tracks()
            .map(|(r, _)| r)
            .filter(|r| self.is_targeted(*r))
            .collect()
    }
}

/// Which monitor a transport command addresses.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Monitor {
    Source,
    #[default]
    Program,
}

/// Where keyboard focus is, for command routing (KBD-002).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Focus {
    #[default]
    Timeline,
    Program,
    Source,
    Project,
    EffectControls,
    Effects,
    History,
    Other,
}

impl Focus {
    pub fn context(self) -> &'static str {
        match self {
            Focus::Timeline => crate::keymap::TIMELINE,
            Focus::Program => crate::keymap::PROGRAM,
            Focus::Source => crate::keymap::SOURCE,
            Focus::Project => crate::keymap::PROJECT,
            Focus::EffectControls => crate::keymap::EFFECT_CONTROLS,
            Focus::Effects => crate::keymap::EFFECTS,
            Focus::History => crate::keymap::HISTORY,
            Focus::Other => crate::keymap::GLOBAL,
        }
    }

    pub fn monitor(self) -> Monitor {
        if self == Focus::Source {
            Monitor::Source
        } else {
            Monitor::Program
        }
    }
}
