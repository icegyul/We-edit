//! The canonical project graph (docs/data-model.md). It is our own model, not a copy of any
//! other application's file structure (DM-001).
//!
//! Entities that change independently are held behind `Arc` so a project revision shares every
//! untouched item, asset, sequence and track with the previous revision; history and preview
//! states cost memory in proportion to what changed (docs/adr/0003-history.md).

use std::sync::Arc;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::catalog::{self, EffectDef, EffectKind};
use crate::color::{Label, MarkerColor, Rgba};
use crate::ids::*;
use crate::media::{ChannelLayout, MediaAsset};
use crate::params::{Param, Value};
use crate::time::*;
use crate::timecode::TimeDisplay;

pub const FORMAT_VERSION: u32 = 1;

// ------------------------------------------------------------------------------------ project

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub settings: ProjectSettings,
    pub root: ItemId,
    pub items: IndexMap<ItemId, Arc<ProjectItem>>,
    pub assets: IndexMap<AssetId, Arc<MediaAsset>>,
    pub sequences: IndexMap<SequenceId, Arc<Sequence>>,
    pub ids: IdGen,
    /// Data kept verbatim from an imported foreign project (DM-004, docs/prproj-spec.md 9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub foreign: Vec<ForeignPayload>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum MediaScaling {
    /// Media keeps its pixel size.
    #[default]
    None,
    /// Motion scale is set so the media fits the frame.
    SetToFrameSize,
    /// The media is rasterized to fit the frame.
    ScaleToFrameSize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub time_display: TimeDisplay,
    pub still_duration: Dur,
    pub video_transition: String,
    pub video_transition_duration: Dur,
    pub audio_transition: String,
    pub audio_transition_duration: Dur,
    pub media_scaling: MediaScaling,
    pub default_sequence: SequenceSettings,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        ProjectSettings {
            time_display: TimeDisplay::Timecode,
            still_duration: Dur::from_seconds(5.0),
            video_transition: catalog::CROSS_DISSOLVE.into(),
            video_transition_duration: Dur::from_seconds(1.0),
            audio_transition: catalog::CONSTANT_POWER.into(),
            audio_transition_duration: Dur::from_seconds(1.0),
            media_scaling: MediaScaling::None,
            default_sequence: SequenceSettings::default(),
        }
    }
}

/// A piece of foreign project data retained verbatim.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForeignPayload {
    /// Source format, e.g. "prproj".
    pub format: String,
    /// Compound owner identity: format scope path plus identity kind and raw value.
    pub owner: String,
    pub key: String,
    pub raw: String,
    /// Why the data could not be mapped.
    pub reason: String,
}

// ------------------------------------------------------------------------------ project items

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectItem {
    pub id: ItemId,
    pub name: String,
    pub parent: Option<ItemId>,
    #[serde(default)]
    pub label: Label,
    pub kind: ItemKind,
    /// Source In/Out marks shown in the Source Monitor.
    #[serde(default)]
    pub mark_in: Option<SrcTime>,
    #[serde(default)]
    pub mark_out: Option<SrcTime>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker<Src>>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ItemKind {
    Bin {
        children: Vec<ItemId>,
    },
    /// A media file, or a subclip of one.
    Media {
        asset: AssetId,
        subclip: Option<SrcRange>,
    },
    Sequence {
        sequence: SequenceId,
    },
    Synthetic {
        generator: Generator,
        duration: Dur,
    },
}

impl ProjectItem {
    pub fn is_bin(&self) -> bool {
        matches!(self.kind, ItemKind::Bin { .. })
    }
}

/// Synthetic sources created in the Project panel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Generator {
    ColorMatte { color: Rgba },
    BlackVideo,
    TransparentVideo,
    BarsAndTone,
    AdjustmentLayer,
}

impl Generator {
    pub fn key(&self) -> &'static str {
        match self {
            Generator::ColorMatte { .. } => "generator.color_matte",
            Generator::BlackVideo => "generator.black_video",
            Generator::TransparentVideo => "generator.transparent_video",
            Generator::BarsAndTone => "generator.bars_and_tone",
            Generator::AdjustmentLayer => "generator.adjustment_layer",
        }
    }

    pub fn has_audio(&self) -> bool {
        matches!(self, Generator::BarsAndTone)
    }
}

// ----------------------------------------------------------------------------------- markers

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum MarkerKind {
    #[default]
    Comment,
    Chapter,
    Segmentation,
    WebLink,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Marker<D> {
    pub id: MarkerId,
    pub start: Time<D>,
    #[serde(default)]
    pub duration: Dur,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub color: MarkerColor,
    #[serde(default)]
    pub kind: MarkerKind,
}

impl<D> Clone for Marker<D> {
    fn clone(&self) -> Self {
        Marker {
            id: self.id,
            start: self.start,
            duration: self.duration,
            name: self.name.clone(),
            comment: self.comment.clone(),
            color: self.color,
            kind: self.kind,
        }
    }
}

impl<D> PartialEq for Marker<D> {
    fn eq(&self, o: &Self) -> bool {
        self.id == o.id
            && self.start == o.start
            && self.duration == o.duration
            && self.name == o.name
            && self.comment == o.comment
            && self.color == o.color
            && self.kind == o.kind
    }
}

impl<D> Marker<D> {
    pub fn new(id: MarkerId, start: Time<D>) -> Self {
        Marker {
            id,
            start,
            duration: Dur::ZERO,
            name: String::new(),
            comment: String::new(),
            color: MarkerColor::Green,
            kind: MarkerKind::Comment,
        }
    }

    pub fn end(&self) -> Time<D> {
        self.start + self.duration
    }
}

// ---------------------------------------------------------------------------------- sequences

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum MasterLayout {
    #[default]
    Stereo,
    Surround51,
    Mono,
}

impl MasterLayout {
    pub fn channels(self) -> usize {
        match self {
            MasterLayout::Mono => 1,
            MasterLayout::Stereo => 2,
            MasterLayout::Surround51 => 6,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SequenceSettings {
    pub width: u32,
    pub height: u32,
    pub rate: Rate,
    pub pixel_aspect: (u32, u32),
    pub audio_rate: u32,
    pub master: MasterLayout,
    pub drop_frame: bool,
    /// Timecode shown at sequence time zero.
    pub start_timecode: Dur,
    /// Blend and dissolve in linear light instead of the working (video) encoding.
    #[serde(default)]
    pub linear_compositing: bool,
    pub video_tracks: usize,
    pub audio_tracks: usize,
}

impl Default for SequenceSettings {
    fn default() -> Self {
        SequenceSettings {
            width: 1920,
            height: 1080,
            rate: Rate::FPS_25,
            pixel_aspect: (1, 1),
            audio_rate: 48000,
            master: MasterLayout::Stereo,
            drop_frame: false,
            start_timecode: Dur::ZERO,
            linear_compositing: false,
            video_tracks: 3,
            audio_tracks: 3,
        }
    }
}

impl SequenceSettings {
    pub fn frame_duration(&self) -> Dur {
        self.rate.frame_duration()
    }

    pub fn aspect(&self) -> f64 {
        self.width as f64 * self.pixel_aspect.0 as f64
            / (self.height as f64 * self.pixel_aspect.1.max(1) as f64)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
}

/// Audio track channel formats.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum AudioTrackLayout {
    /// Accepts mono and stereo clips.
    #[default]
    Standard,
    Mono,
    Surround51,
    Adaptive,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub kind: TrackKind,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "yes")]
    pub sync_lock: bool,
    /// Video: output enabled (eye). Audio: always true; see `muted`.
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub layout: AudioTrackLayout,
    /// Sorted by start; never overlapping (DM-TL-001).
    pub clips: Vec<Clip>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<Transition>,
    /// Track-level mixer components (audio): Volume and Panner, then inserts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<Component>,
    /// Persisted view state: header height in logical pixels.
    #[serde(default)]
    pub height: f32,
}

fn yes() -> bool {
    true
}

impl Track {
    pub fn new(id: TrackId, kind: TrackKind) -> Track {
        Track {
            id,
            kind,
            name: String::new(),
            locked: false,
            sync_lock: true,
            enabled: true,
            muted: false,
            solo: false,
            layout: AudioTrackLayout::Standard,
            clips: Vec::new(),
            transitions: Vec::new(),
            components: Vec::new(),
            height: 0.0,
        }
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    pub fn index_of(&self, id: ClipId) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// The clip whose range contains `t`.
    pub fn clip_at(&self, t: SeqTime) -> Option<&Clip> {
        let i = self.clips.partition_point(|c| c.start <= t);
        i.checked_sub(1)
            .map(|i| &self.clips[i])
            .filter(|c| c.range().contains(t))
    }

    /// Clips intersecting a range, in order.
    pub fn clips_in(&self, r: SeqRange) -> impl Iterator<Item = &Clip> {
        self.clips.iter().filter(move |c| c.range().overlaps(&r))
    }

    pub fn end(&self) -> SeqTime {
        self.clips.last().map(|c| c.end()).unwrap_or(SeqTime::ZERO)
    }

    pub fn sort(&mut self) {
        self.clips.sort_by_key(|c| (c.start, c.id));
        self.transitions.sort_by_key(|t| (t.cut, t.id));
    }

    pub fn transition(&self, id: TransitionId) -> Option<&Transition> {
        self.transitions.iter().find(|t| t.id == id)
    }

    /// Transitions active at `t`.
    pub fn transition_at(&self, t: SeqTime) -> Option<&Transition> {
        self.transitions.iter().find(|tr| tr.range().contains(t))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sequence {
    pub id: SequenceId,
    pub name: String,
    pub settings: SequenceSettings,
    /// V1 first (bottom of the stack).
    pub video: Vec<Arc<Track>>,
    /// A1 first.
    pub audio: Vec<Arc<Track>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<Marker<Seq>>,
    #[serde(default)]
    pub mark_in: Option<SeqTime>,
    #[serde(default)]
    pub mark_out: Option<SeqTime>,
    #[serde(default)]
    pub playhead: SeqTime,
    /// Master bus components (Volume, then inserts).
    #[serde(default)]
    pub master: Vec<Component>,
}

/// Addresses a track inside a sequence.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct TrackRef {
    pub kind: TrackKind,
    pub index: usize,
}

impl TrackRef {
    pub fn video(index: usize) -> TrackRef {
        TrackRef {
            kind: TrackKind::Video,
            index,
        }
    }
    pub fn audio(index: usize) -> TrackRef {
        TrackRef {
            kind: TrackKind::Audio,
            index,
        }
    }
}

impl Sequence {
    pub fn new(
        id: SequenceId,
        name: impl Into<String>,
        settings: SequenceSettings,
        ids: &mut IdGen,
    ) -> Sequence {
        let video = (0..settings.video_tracks.max(1))
            .map(|_| Arc::new(Track::new(ids.track(), TrackKind::Video)))
            .collect();
        let audio = (0..settings.audio_tracks.max(1))
            .map(|_| {
                let mut t = Track::new(ids.track(), TrackKind::Audio);
                t.components = default_components(EffectKind::AudioFixed, ids);
                Arc::new(t)
            })
            .collect();
        let master = vec![Component::new(catalog::find(catalog::VOLUME).unwrap(), ids)];
        Sequence {
            id,
            name: name.into(),
            settings,
            video,
            audio,
            markers: Vec::new(),
            mark_in: None,
            mark_out: None,
            playhead: SeqTime::ZERO,
            master,
        }
    }

    pub fn rate(&self) -> Rate {
        self.settings.rate
    }

    pub fn tracks(&self, kind: TrackKind) -> &Vec<Arc<Track>> {
        match kind {
            TrackKind::Video => &self.video,
            TrackKind::Audio => &self.audio,
        }
    }

    pub fn tracks_mut(&mut self, kind: TrackKind) -> &mut Vec<Arc<Track>> {
        match kind {
            TrackKind::Video => &mut self.video,
            TrackKind::Audio => &mut self.audio,
        }
    }

    pub fn track(&self, r: TrackRef) -> Option<&Track> {
        self.tracks(r.kind).get(r.index).map(|t| t.as_ref())
    }

    /// Mutable access, cloning the track if a previous revision shares it.
    pub fn track_mut(&mut self, r: TrackRef) -> Option<&mut Track> {
        self.tracks_mut(r.kind).get_mut(r.index).map(Arc::make_mut)
    }

    pub fn track_ref(&self, id: TrackId) -> Option<TrackRef> {
        for kind in [TrackKind::Video, TrackKind::Audio] {
            if let Some(i) = self.tracks(kind).iter().position(|t| t.id == id) {
                return Some(TrackRef { kind, index: i });
            }
        }
        None
    }

    pub fn all_tracks(&self) -> impl Iterator<Item = (TrackRef, &Track)> {
        let v = self
            .video
            .iter()
            .enumerate()
            .map(|(i, t)| (TrackRef::video(i), t.as_ref()));
        let a = self
            .audio
            .iter()
            .enumerate()
            .map(|(i, t)| (TrackRef::audio(i), t.as_ref()));
        v.chain(a)
    }

    /// Finds a clip and the track it is on.
    pub fn find_clip(&self, id: ClipId) -> Option<(TrackRef, &Clip)> {
        self.all_tracks()
            .find_map(|(r, t)| t.clip(id).map(|c| (r, c)))
    }

    pub fn clip(&self, id: ClipId) -> Option<&Clip> {
        self.find_clip(id).map(|(_, c)| c)
    }

    pub fn clip_mut(&mut self, id: ClipId) -> Option<&mut Clip> {
        let (r, _) = self.find_clip(id)?;
        self.track_mut(r)?.clip_mut(id)
    }

    pub fn clips(&self) -> impl Iterator<Item = (TrackRef, &Clip)> {
        self.all_tracks()
            .flat_map(|(r, t)| t.clips.iter().map(move |c| (r, c)))
    }

    /// Clips sharing the link of `id`, including `id` itself.
    pub fn linked(&self, id: ClipId) -> Vec<ClipId> {
        let Some(clip) = self.clip(id) else {
            return vec![];
        };
        match clip.link {
            None => vec![id],
            Some(link) => self
                .clips()
                .filter(|(_, c)| c.link == Some(link))
                .map(|(_, c)| c.id)
                .collect(),
        }
    }

    /// Clips in the same user group, including `id`.
    pub fn grouped(&self, id: ClipId) -> Vec<ClipId> {
        let Some(clip) = self.clip(id) else {
            return vec![];
        };
        match clip.group {
            None => vec![id],
            Some(g) => self
                .clips()
                .filter(|(_, c)| c.group == Some(g))
                .map(|(_, c)| c.id)
                .collect(),
        }
    }

    /// End of the last clip.
    pub fn duration(&self) -> Dur {
        self.all_tracks()
            .map(|(_, t)| t.end())
            .max()
            .unwrap_or(SeqTime::ZERO)
            .since_zero()
    }

    /// Sorted, de-duplicated edit points of the given tracks (clip boundaries).
    pub fn edit_points(&self, tracks: impl IntoIterator<Item = TrackRef>) -> Vec<SeqTime> {
        let mut v = Vec::new();
        for r in tracks {
            if let Some(t) = self.track(r) {
                for c in &t.clips {
                    v.push(c.start);
                    v.push(c.end());
                }
            }
        }
        v.sort();
        v.dedup();
        v
    }

    pub fn track_name(&self, r: TrackRef) -> String {
        match self.track(r) {
            Some(t) if !t.name.is_empty() => t.name.clone(),
            _ => match r.kind {
                TrackKind::Video => format!("V{}", r.index + 1),
                TrackKind::Audio => format!("A{}", r.index + 1),
            },
        }
    }

    /// Sequences directly nested in this one.
    pub fn nested(&self) -> impl Iterator<Item = SequenceId> + '_ {
        self.clips().filter_map(|(_, c)| match c.source {
            ClipSource::Sequence { sequence, .. } => Some(sequence),
            _ => None,
        })
    }

    /// In/Out range, defaulting to the whole sequence.
    pub fn in_out(&self) -> SeqRange {
        let start = self.mark_in.unwrap_or(SeqTime::ZERO);
        let end = self.mark_out.unwrap_or(SeqTime::ZERO + self.duration());
        SeqRange::new(start, end.max(start))
    }
}

// -------------------------------------------------------------------------------------- clips

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClipSource {
    /// A stream of a media file. `stream` is the audio stream ordinal for audio clips.
    Asset {
        asset: AssetId,
        item: Option<ItemId>,
        stream: usize,
    },
    /// A nested sequence (DM-TL-005).
    Sequence {
        sequence: SequenceId,
        item: Option<ItemId>,
    },
    /// A synthetic project item.
    Generator { item: ItemId },
    /// A graphics clip whose picture is its Text/Shape components.
    Graphic,
}

impl ClipSource {
    pub fn item(&self) -> Option<ItemId> {
        match self {
            ClipSource::Asset { item, .. } | ClipSource::Sequence { item, .. } => *item,
            ClipSource::Generator { item } => Some(*item),
            ClipSource::Graphic => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub id: ClipId,
    pub name: String,
    pub kind: TrackKind,
    pub source: ClipSource,
    pub start: SeqTime,
    pub duration: Dur,
    /// Source time shown at the clip's first frame (for reverse clips, the source range start).
    pub source_in: SrcTime,
    #[serde(default)]
    pub speed: Speed,
    #[serde(default)]
    pub reverse: bool,
    /// Frame hold: the whole clip shows this source time.
    #[serde(default)]
    pub hold: Option<SrcTime>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub link: Option<LinkId>,
    #[serde(default)]
    pub group: Option<GroupId>,
    #[serde(default)]
    pub label: Label,
    /// Fixed components first, then standard effects in stack order (DM-FX-002).
    pub components: Vec<Component>,
    /// Audio gain applied before effects (dB).
    #[serde(default)]
    pub gain_db: f64,
    /// Rasterize to fit the sequence frame.
    #[serde(default)]
    pub scale_to_frame: bool,
    /// Audio channel layout this clip carries.
    #[serde(default)]
    pub channels: Option<ChannelLayout>,
}

impl Clip {
    pub fn end(&self) -> SeqTime {
        self.start + self.duration
    }

    pub fn range(&self) -> SeqRange {
        SeqRange::with_duration(self.start, self.duration)
    }

    pub fn source_duration(&self) -> Dur {
        self.speed.to_source(self.duration)
    }

    /// Source range used by the clip.
    pub fn source_range(&self) -> SrcRange {
        SrcRange::with_duration(self.source_in, self.source_duration())
    }

    /// Sequence time -> source time (DM-TL-002).
    pub fn to_source(&self, t: SeqTime) -> SrcTime {
        if let Some(h) = self.hold {
            return h;
        }
        let offset = self.speed.to_source(t - self.start);
        if self.reverse {
            self.source_in + self.source_duration() - offset - Dur(1)
        } else {
            self.source_in + offset
        }
    }

    /// Source time -> sequence time (for drawing keyframes).
    pub fn to_sequence(&self, s: SrcTime) -> SeqTime {
        if self.reverse {
            self.start
                + self
                    .speed
                    .to_sequence(self.source_in + self.source_duration() - s)
        } else {
            self.start + self.speed.to_sequence(s - self.source_in)
        }
    }

    pub fn component(&self, effect: &str) -> Option<&Component> {
        self.components.iter().find(|c| c.effect == effect)
    }

    pub fn component_mut(&mut self, effect: &str) -> Option<&mut Component> {
        self.components.iter_mut().find(|c| c.effect == effect)
    }

    pub fn component_by_id(&self, id: ComponentId) -> Option<&Component> {
        self.components.iter().find(|c| c.id == id)
    }

    pub fn component_by_id_mut(&mut self, id: ComponentId) -> Option<&mut Component> {
        self.components.iter_mut().find(|c| c.id == id)
    }

    pub fn is_video(&self) -> bool {
        self.kind == TrackKind::Video
    }
}

// --------------------------------------------------------------------------------- transitions

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, Serialize, Deserialize)]
pub enum Alignment {
    #[default]
    CenterAtCut,
    StartAtCut,
    EndAtCut,
    /// The transition starts this long before the cut.
    Custom(Dur),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub id: TransitionId,
    pub effect: String,
    /// The edit point this transition belongs to.
    pub cut: SeqTime,
    pub duration: Dur,
    pub alignment: Alignment,
    /// Outgoing clip (None for a transition at a clip's head).
    pub from: Option<ClipId>,
    /// Incoming clip (None for a transition at a clip's tail).
    pub to: Option<ClipId>,
    #[serde(default)]
    pub params: Vec<Param>,
}

impl Transition {
    /// How much of the transition lies before the cut.
    pub fn before_cut(&self) -> Dur {
        match self.alignment {
            Alignment::CenterAtCut => self.duration / 2,
            Alignment::StartAtCut => Dur::ZERO,
            Alignment::EndAtCut => self.duration,
            Alignment::Custom(d) => d.clamp(Dur::ZERO, self.duration),
        }
    }

    pub fn range(&self) -> SeqRange {
        let start = self.cut - self.before_cut();
        SeqRange::with_duration(start, self.duration)
    }

    /// Progress 0..1 at `t`.
    pub fn progress(&self, t: SeqTime) -> f64 {
        let r = self.range();
        if self.duration.0 <= 0 {
            return 1.0;
        }
        ((t - r.start).0 as f64 / self.duration.0 as f64).clamp(0.0, 1.0)
    }

    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.key == key)
    }
}

// ---------------------------------------------------------------------------------- components

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Component {
    pub id: ComponentId,
    /// Catalog ID, or a foreign identity when the effect is unknown (DM-FX-003).
    pub effect: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub params: Vec<Param>,
    /// Retained verbatim when the component came from a foreign project and is not understood.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreign: Option<String>,
}

impl Component {
    pub fn new(def: &EffectDef, ids: &mut IdGen) -> Component {
        Component {
            id: ids.component(),
            effect: def.id.to_string(),
            enabled: true,
            params: def
                .params
                .iter()
                .map(|p| Param::new(p.key, p.default_value()))
                .collect(),
            foreign: None,
        }
    }

    pub fn def(&self) -> Option<&'static EffectDef> {
        catalog::find(&self.effect)
    }

    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.key == key)
    }

    pub fn param_mut(&mut self, key: &str) -> Option<&mut Param> {
        self.params.iter_mut().find(|p| p.key == key)
    }

    pub fn value_at(&self, key: &str, t: SrcTime) -> Value {
        match self.param(key) {
            Some(p) => p.value_at(t),
            None => self
                .def()
                .and_then(|d| d.param(key))
                .map(|s| s.default_value())
                .unwrap_or(Value::Float(0.0)),
        }
    }

    pub fn f64_at(&self, key: &str, t: SrcTime) -> f64 {
        self.value_at(key, t).as_f64()
    }

    pub fn is_fixed(&self) -> bool {
        self.def()
            .is_some_and(|d| matches!(d.kind, EffectKind::VideoFixed | EffectKind::AudioFixed))
    }

    /// Adds parameters that a newer catalog defines and an older project lacks, in catalog order.
    pub fn complete_params(&mut self) {
        let Some(def) = self.def() else { return };
        for (i, spec) in def.params.iter().enumerate() {
            if self.param(spec.key).is_none() {
                let at = i.min(self.params.len());
                self.params
                    .insert(at, Param::new(spec.key, spec.default_value()));
            }
        }
    }
}

/// The fixed components every clip of a kind carries.
pub fn default_components(kind: EffectKind, ids: &mut IdGen) -> Vec<Component> {
    let list: &[&str] = match kind {
        EffectKind::VideoFixed => &[catalog::MOTION, catalog::OPACITY],
        EffectKind::AudioFixed => &[catalog::VOLUME, catalog::CHANNEL_VOLUME, catalog::PANNER],
        _ => &[],
    };
    list.iter()
        .map(|id| Component::new(catalog::find(id).unwrap(), ids))
        .collect()
}

// ----------------------------------------------------------------------------- project methods

impl Project {
    pub fn new(name: impl Into<String>) -> Project {
        let mut ids = IdGen::default();
        let root = ids.item();
        let mut items = IndexMap::new();
        items.insert(
            root,
            Arc::new(ProjectItem {
                id: root,
                name: String::new(),
                parent: None,
                label: Label::None,
                kind: ItemKind::Bin {
                    children: Vec::new(),
                },
                mark_in: None,
                mark_out: None,
                markers: Vec::new(),
                comment: String::new(),
            }),
        );
        Project {
            format_version: FORMAT_VERSION,
            name: name.into(),
            settings: ProjectSettings::default(),
            root,
            items,
            assets: IndexMap::new(),
            sequences: IndexMap::new(),
            ids,
            foreign: Vec::new(),
        }
    }

    pub fn item(&self, id: ItemId) -> Option<&ProjectItem> {
        self.items.get(&id).map(|a| a.as_ref())
    }

    pub fn item_mut(&mut self, id: ItemId) -> Option<&mut ProjectItem> {
        self.items.get_mut(&id).map(Arc::make_mut)
    }

    pub fn asset(&self, id: AssetId) -> Option<&MediaAsset> {
        self.assets.get(&id).map(|a| a.as_ref())
    }

    pub fn sequence(&self, id: SequenceId) -> Option<&Sequence> {
        self.sequences.get(&id).map(|a| a.as_ref())
    }

    pub fn sequence_mut(&mut self, id: SequenceId) -> Option<&mut Sequence> {
        self.sequences.get_mut(&id).map(Arc::make_mut)
    }

    /// Children of a bin, in stored order.
    pub fn children(&self, bin: ItemId) -> &[ItemId] {
        match self.item(bin).map(|i| &i.kind) {
            Some(ItemKind::Bin { children }) => children,
            _ => &[],
        }
    }

    /// Adds an item to a bin (the root when `parent` is not a bin).
    pub fn add_item(&mut self, parent: ItemId, name: impl Into<String>, kind: ItemKind) -> ItemId {
        let parent = if self.item(parent).is_some_and(|i| i.is_bin()) {
            parent
        } else {
            self.root
        };
        let id = self.ids.item();
        self.items.insert(
            id,
            Arc::new(ProjectItem {
                id,
                name: name.into(),
                parent: Some(parent),
                label: Label::None,
                kind,
                mark_in: None,
                mark_out: None,
                markers: Vec::new(),
                comment: String::new(),
            }),
        );
        if let Some(ItemKind::Bin { children }) = self.item_mut(parent).map(|i| &mut i.kind) {
            children.push(id);
        }
        id
    }

    pub fn add_bin(&mut self, parent: ItemId, name: impl Into<String>) -> ItemId {
        self.add_item(
            parent,
            name,
            ItemKind::Bin {
                children: Vec::new(),
            },
        )
    }

    pub fn add_asset(&mut self, parent: ItemId, mut asset: MediaAsset) -> (AssetId, ItemId) {
        let id = self.ids.asset();
        asset.id = id;
        let name = asset.file_name().to_string();
        self.assets.insert(id, Arc::new(asset));
        let item = self.add_item(
            parent,
            name,
            ItemKind::Media {
                asset: id,
                subclip: None,
            },
        );
        (id, item)
    }

    /// Creates an empty sequence and its project item.
    pub fn add_sequence(
        &mut self,
        parent: ItemId,
        name: impl Into<String>,
        settings: SequenceSettings,
    ) -> (SequenceId, ItemId) {
        let name = name.into();
        let id = self.ids.sequence();
        let seq = Sequence::new(id, name.clone(), settings, &mut self.ids);
        self.sequences.insert(id, Arc::new(seq));
        let item = self.add_item(parent, name, ItemKind::Sequence { sequence: id });
        (id, item)
    }

    /// Moves an item to another bin. Refuses to move a bin into itself or its descendants.
    pub fn move_item(&mut self, id: ItemId, to_bin: ItemId) -> bool {
        if id == self.root
            || !self.item(to_bin).is_some_and(|i| i.is_bin())
            || self.is_ancestor(id, to_bin)
        {
            return false;
        }
        let Some(old) = self.item(id).and_then(|i| i.parent) else {
            return false;
        };
        if old == to_bin {
            return true;
        }
        if let Some(ItemKind::Bin { children }) = self.item_mut(old).map(|i| &mut i.kind) {
            children.retain(|c| *c != id);
        }
        if let Some(ItemKind::Bin { children }) = self.item_mut(to_bin).map(|i| &mut i.kind) {
            children.push(id);
        }
        if let Some(item) = self.item_mut(id) {
            item.parent = Some(to_bin);
        }
        true
    }

    /// Whether `a` is `b` or one of its ancestors.
    pub fn is_ancestor(&self, a: ItemId, b: ItemId) -> bool {
        let mut cur = Some(b);
        let mut guard = 0;
        while let Some(c) = cur {
            if c == a {
                return true;
            }
            cur = self.item(c).and_then(|i| i.parent);
            guard += 1;
            if guard > 10_000 {
                return false;
            }
        }
        false
    }

    /// Removes an item (and a bin's contents). Sequences and assets that are no longer referenced
    /// by any item are removed too; clips using them are removed from every sequence.
    pub fn remove_item(&mut self, id: ItemId) {
        if id == self.root {
            return;
        }
        let mut stack = vec![id];
        let mut doomed = Vec::new();
        while let Some(i) = stack.pop() {
            if let Some(item) = self.item(i) {
                if let ItemKind::Bin { children } = &item.kind {
                    stack.extend(children.iter().copied());
                }
                doomed.push(i);
            }
        }
        if let Some(parent) = self.item(id).and_then(|i| i.parent)
            && let Some(ItemKind::Bin { children }) = self.item_mut(parent).map(|i| &mut i.kind)
        {
            children.retain(|c| *c != id);
        }
        let mut dead_assets = Vec::new();
        let mut dead_seqs = Vec::new();
        for i in &doomed {
            if let Some(item) = self.items.shift_remove(i) {
                match &item.kind {
                    ItemKind::Media { asset, .. } => dead_assets.push(*asset),
                    ItemKind::Sequence { sequence } => dead_seqs.push(*sequence),
                    _ => {}
                }
            }
        }
        dead_assets.retain(|a| {
            !self
                .items
                .values()
                .any(|i| matches!(i.kind, ItemKind::Media { asset, .. } if asset == *a))
        });
        for a in &dead_assets {
            self.assets.shift_remove(a);
        }
        for s in &dead_seqs {
            self.sequences.shift_remove(s);
        }
        // clips that referenced removed sources
        let gone_items: std::collections::HashSet<ItemId> = doomed.into_iter().collect();
        let seq_ids: Vec<SequenceId> = self.sequences.keys().copied().collect();
        for sid in seq_ids {
            let uses = |c: &Clip| match &c.source {
                ClipSource::Asset { asset, .. } => dead_assets.contains(asset),
                ClipSource::Sequence { sequence, .. } => dead_seqs.contains(sequence),
                ClipSource::Generator { item } => gone_items.contains(item),
                ClipSource::Graphic => false,
            };
            let seq = self.sequence(sid).unwrap();
            if !seq.clips().any(|(_, c)| uses(c)) {
                continue;
            }
            let seq = self.sequence_mut(sid).unwrap();
            for kind in [TrackKind::Video, TrackKind::Audio] {
                for t in seq.tracks_mut(kind) {
                    if t.clips.iter().any(&uses) {
                        let t = Arc::make_mut(t);
                        t.clips.retain(|c| !uses(c));
                    }
                }
            }
        }
    }

    /// Items whose parent chain reaches the root, in depth-first order.
    pub fn walk(&self) -> Vec<(usize, ItemId)> {
        let mut out = Vec::new();
        fn rec(p: &Project, bin: ItemId, depth: usize, out: &mut Vec<(usize, ItemId)>) {
            for &c in p.children(bin) {
                out.push((depth, c));
                if p.item(c).is_some_and(|i| i.is_bin()) && depth < 256 {
                    rec(p, c, depth + 1, out);
                }
            }
        }
        rec(self, self.root, 0, &mut out);
        out
    }

    /// Items referencing a sequence.
    pub fn sequence_item(&self, id: SequenceId) -> Option<ItemId> {
        self.items
            .values()
            .find(|i| matches!(i.kind, ItemKind::Sequence { sequence } if sequence == id))
            .map(|i| i.id)
    }

    pub fn asset_item(&self, id: AssetId) -> Option<ItemId> {
        self.items
            .values()
            .find(|i| matches!(i.kind, ItemKind::Media { asset, subclip: None } if asset == id))
            .map(|i| i.id)
    }

    /// The source range available to a clip, `None` when unbounded (stills, generators, graphics).
    pub fn available(&self, source: &ClipSource) -> Option<SrcRange> {
        match source {
            ClipSource::Asset { asset, .. } => self.asset(*asset).and_then(|a| a.available()),
            ClipSource::Sequence { sequence, .. } => self.sequence(*sequence).map(|s| {
                SrcRange::with_duration(
                    SrcTime::ZERO,
                    s.duration().max(s.settings.frame_duration()),
                )
            }),
            ClipSource::Generator { .. } | ClipSource::Graphic => None,
        }
    }

    /// Default frame size of a clip source before Motion (source pixels).
    pub fn source_size(&self, source: &ClipSource, seq: &SequenceSettings) -> (u32, u32) {
        match source {
            ClipSource::Asset { asset, .. } => self
                .asset(*asset)
                .and_then(|a| a.video.as_ref())
                .map(|v| v.display_size())
                .unwrap_or((seq.width, seq.height)),
            ClipSource::Sequence { sequence, .. } => self
                .sequence(*sequence)
                .map(|s| (s.settings.width, s.settings.height))
                .unwrap_or((seq.width, seq.height)),
            _ => (seq.width, seq.height),
        }
    }

    /// Display name for a clip source.
    pub fn source_name(&self, source: &ClipSource) -> String {
        match source {
            ClipSource::Asset { asset, item, .. } => item
                .and_then(|i| self.item(i))
                .map(|i| i.name.clone())
                .or_else(|| self.asset(*asset).map(|a| a.file_name().to_string()))
                .unwrap_or_default(),
            ClipSource::Sequence { sequence, .. } => self
                .sequence(*sequence)
                .map(|s| s.name.clone())
                .unwrap_or_default(),
            ClipSource::Generator { item } => {
                self.item(*item).map(|i| i.name.clone()).unwrap_or_default()
            }
            ClipSource::Graphic => "Graphic".into(),
        }
    }

    /// Whether `inner` is `outer` or nested (directly or indirectly) inside it.
    pub fn nests(&self, outer: SequenceId, inner: SequenceId) -> bool {
        let mut stack = vec![outer];
        let mut seen = std::collections::HashSet::new();
        while let Some(s) = stack.pop() {
            if s == inner {
                return true;
            }
            if !seen.insert(s) {
                continue;
            }
            if let Some(seq) = self.sequence(s) {
                stack.extend(seq.nested());
            }
        }
        false
    }

    /// Makes sure the ID generator is above every stored ID (after loading).
    pub fn refresh_ids(&mut self) {
        let mut max = 0u64;
        let mut see = |v: u64| max = max.max(v);
        for (id, item) in &self.items {
            see(id.0);
            for m in &item.markers {
                see(m.id.0);
            }
        }
        for id in self.assets.keys() {
            see(id.0);
        }
        for (id, seq) in &self.sequences {
            see(id.0);
            for m in &seq.markers {
                see(m.id.0);
            }
            for c in &seq.master {
                see(c.id.0);
            }
            for (_, t) in seq.all_tracks() {
                see(t.id.0);
                for c in &t.components {
                    see(c.id.0);
                }
                for tr in &t.transitions {
                    see(tr.id.0);
                }
                for c in &t.clips {
                    see(c.id.0);
                    if let Some(l) = c.link {
                        see(l.0);
                    }
                    if let Some(g) = c.group {
                        see(g.0);
                    }
                    for comp in &c.components {
                        see(comp.id.0);
                    }
                }
            }
        }
        self.ids.observe(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_and_bins() {
        let mut p = Project::new("t");
        let bin = p.add_bin(p.root, "Footage");
        let inner = p.add_bin(bin, "Inner");
        assert!(
            !p.move_item(bin, inner),
            "a bin cannot move into its descendant"
        );
        let (seq, seq_item) = p.add_sequence(inner, "Sequence 01", SequenceSettings::default());
        assert_eq!(p.walk().len(), 3);
        assert!(p.sequence(seq).is_some());
        p.remove_item(bin);
        assert!(p.sequence(seq).is_none());
        assert!(p.item(seq_item).is_none());
        assert_eq!(p.children(p.root).len(), 0);
    }

    #[test]
    fn sequences_share_untouched_tracks_between_revisions() {
        let mut p = Project::new("t");
        let (seq, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        let before = p.clone();
        p.sequence_mut(seq)
            .unwrap()
            .track_mut(TrackRef::video(0))
            .unwrap()
            .name = "Main".into();
        let a = &before.sequence(seq).unwrap().video;
        let b = &p.sequence(seq).unwrap().video;
        assert!(!Arc::ptr_eq(&a[0], &b[0]));
        assert!(Arc::ptr_eq(&a[1], &b[1]));
    }

    #[test]
    fn clip_time_mapping() {
        let mut ids = IdGen::default();
        let clip = Clip {
            id: ids.clip(),
            name: "c".into(),
            kind: TrackKind::Video,
            source: ClipSource::Graphic,
            start: SeqTime::from_seconds(10.0),
            duration: Dur::from_seconds(4.0),
            source_in: SrcTime::from_seconds(2.0),
            speed: Speed::new(2, 1),
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: vec![],
            gain_db: 0.0,
            scale_to_frame: false,
            channels: None,
        };
        assert_eq!(clip.source_range().end, SrcTime::from_seconds(10.0));
        assert_eq!(
            clip.to_source(SeqTime::from_seconds(11.0)),
            SrcTime::from_seconds(4.0)
        );
        assert_eq!(
            clip.to_sequence(SrcTime::from_seconds(4.0)),
            SeqTime::from_seconds(11.0)
        );
        let mut rev = clip.clone();
        rev.reverse = true;
        // first frame shows the end of the source range
        assert!(rev.to_source(rev.start) < SrcTime::from_seconds(10.0));
        assert!(rev.to_source(rev.start) > SrcTime::from_seconds(9.99));
        assert_eq!(rev.to_sequence(SrcTime::from_seconds(10.0)), rev.start);
    }

    #[test]
    fn transition_ranges() {
        let tr = Transition {
            id: TransitionId(1),
            effect: catalog::CROSS_DISSOLVE.into(),
            cut: SeqTime::from_seconds(10.0),
            duration: Dur::from_seconds(1.0),
            alignment: Alignment::CenterAtCut,
            from: None,
            to: None,
            params: vec![],
        };
        assert_eq!(tr.range().start, SeqTime::from_seconds(9.5));
        assert!((tr.progress(SeqTime::from_seconds(10.0)) - 0.5).abs() < 1e-12);
        let end = Transition {
            alignment: Alignment::EndAtCut,
            ..tr.clone()
        };
        assert_eq!(end.range().end, SeqTime::from_seconds(10.0));
    }

    #[test]
    fn project_round_trips_through_json() {
        let mut p = Project::new("t");
        p.add_sequence(p.root, "S", SequenceSettings::default());
        let text = serde_json::to_string(&p).unwrap();
        let back: Project = serde_json::from_str(&text).unwrap();
        assert_eq!(p, back);
    }
}
