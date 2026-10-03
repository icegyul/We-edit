//! Read-only import of Premiere Pro 2024 project files.
//!
//! The file is a gzip container holding an XML identity/reference graph. This importer:
//!
//! * verifies the gzip container (checksum, size, no trailing data) and parses the XML under the
//!   bounded profile of `xml.rs`;
//! * indexes identities of the root serialization scope only: definitions that are direct
//!   children of `PremiereData`. References found inside nested serialization islands (for
//!   example project view state) are never resolved against the root scope (PR-GRAPH-001B), and
//!   duplicated root identities are reported and not followed;
//! * maps bins, media, sequences, tracks, clip track items, Motion and selected effect parameters
//!   and markers onto the canonical model; everything else is counted in the report, and unknown
//!   effects are kept verbatim as disabled components (DM-FX-003).
//!
//! The source file is never written (PR-SAVE-001). Mappings that rest on observation rather than
//! a validated contract are listed in the report as approximations.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use op_core::catalog::{self, EffectKind};
use op_core::*;

use crate::ProjectError;
use crate::xml::{self, Doc, Limits};

/// Premiere's observed time base (PR-TIME-001); an adapter fact only (DM-TIME-005).
pub const PREMIERE_TICKS_PER_SECOND: i64 = 254_016_000_000;
const TICK_RATIO: i64 = PREMIERE_TICKS_PER_SECOND / TICKS_PER_SECOND;

const MAX_COMPRESSED: usize = 1 << 30;
const MAX_XML: u64 = 1 << 30;
const MAX_OPAQUE: usize = 256 << 10;

const VIDEO_GUID: &str = "228cda18-3625-4d2d-951e-348879e4ed93";
const AUDIO_GUID: &str = "80b8e3d5-6dca-4195-aefb-cb5f407ab009";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    pub premiere_data_version: String,
    pub project_version: String,
    pub bins: usize,
    pub media: usize,
    pub sequences: usize,
    pub clips: usize,
    pub markers: usize,
    pub components_mapped: usize,
    /// Effects of other programs replaced by an equivalent of ours.
    pub components_equivalent: usize,
    pub components_opaque: usize,
    pub transitions: usize,
    /// Reason -> count of things not imported.
    pub skipped: BTreeMap<String, usize>,
    /// Approximations and diagnostics.
    pub warnings: Vec<String>,
}

impl ImportReport {
    fn skip(&mut self, reason: impl Into<String>) {
        *self.skipped.entry(reason.into()).or_default() += 1;
    }

    fn warn(&mut self, w: impl Into<String>) {
        let w = w.into();
        if !self.warnings.contains(&w) && self.warnings.len() < 200 {
            self.warnings.push(w);
        }
    }
}

/// Decompresses and verifies the container.
pub fn decompress(bytes: &[u8]) -> Result<String, ProjectError> {
    if bytes.len() > MAX_COMPRESSED {
        return Err(ProjectError::Format("the project file is too large".into()));
    }
    if bytes.len() < 18 || bytes[0] != 0x1F || bytes[1] != 0x8B {
        return Err(ProjectError::Format(
            "not a gzip-compressed Premiere project".into(),
        ));
    }
    let mut dec = flate2::bufread::GzDecoder::new(bytes);
    let mut out = Vec::new();
    (&mut dec)
        .take(MAX_XML + 1)
        .read_to_end(&mut out)
        .map_err(|e| ProjectError::Format(format!("damaged project container: {e}")))?;
    if out.len() as u64 > MAX_XML {
        return Err(ProjectError::Format("the project XML is too large".into()));
    }
    let rest = dec.into_inner();
    if !rest.is_empty() {
        return Err(ProjectError::Format(
            "unexpected data after the compressed project".into(),
        ));
    }
    let text = String::from_utf8(out)
        .map_err(|_| ProjectError::Format("the project XML is not UTF-8".into()))?;
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// Imports a `.prproj` file's bytes.
pub fn import(bytes: &[u8], name: &str) -> Result<(Project, ImportReport), ProjectError> {
    let text = decompress(bytes)?;
    let doc =
        xml::parse(&text, Limits::default()).map_err(|e| ProjectError::Format(e.to_string()))?;
    if doc.name(0) != "PremiereData" {
        return Err(ProjectError::Format(
            "the XML root is not PremiereData".into(),
        ));
    }
    let mut im = Importer::new(&doc, name);
    im.run()?;
    Ok((im.project, im.report))
}

struct Importer<'a> {
    doc: &'a Doc,
    ids: HashMap<String, usize>,
    uids: HashMap<String, usize>,
    ambiguous: HashSet<String>,
    project: Project,
    report: ImportReport,
    sequences: HashMap<usize, SequenceId>,
    assets: HashMap<usize, AssetId>,
    /// MasterClip node -> project item
    masters: HashMap<usize, ItemId>,
}

impl<'a> Importer<'a> {
    fn new(doc: &'a Doc, name: &str) -> Importer<'a> {
        let mut ids = HashMap::new();
        let mut uids = HashMap::new();
        let mut ambiguous = HashSet::new();
        for c in doc.children(0) {
            if let Some(id) = doc.attr(c, "ObjectID")
                && ids.insert(id.to_string(), c).is_some()
            {
                ambiguous.insert(format!("id:{id}"));
            }
            if let Some(uid) = doc.attr(c, "ObjectUID")
                && uids.insert(uid.to_string(), c).is_some()
            {
                ambiguous.insert(format!("uid:{uid}"));
            }
        }
        let mut report = ImportReport {
            premiere_data_version: doc.attr(0, "Version").unwrap_or("").into(),
            ..Default::default()
        };
        if !ambiguous.is_empty() {
            report.warn(format!(
                "{} duplicated identities in the root graph were not followed",
                ambiguous.len()
            ));
        }
        Importer {
            doc,
            ids,
            uids,
            ambiguous,
            project: Project::new(name),
            report,
            sequences: HashMap::new(),
            assets: HashMap::new(),
            masters: HashMap::new(),
        }
    }

    /// Whether `n` lies inside a nested serialization island (below its root-level definition,
    /// under another element that carries an identity).
    fn in_island(&self, n: usize) -> bool {
        let mut cur = self.doc.node(n).parent;
        let mut last = n;
        while let Some(p) = cur {
            if p == 0 {
                // `last` is the root-level definition; identities on it do not count
                return false;
            }
            if last != n
                && (self.doc.attr(last, "ObjectID").is_some()
                    || self.doc.attr(last, "ObjectUID").is_some())
            {
                return true;
            }
            last = p;
            cur = self.doc.node(p).parent;
        }
        false
    }

    /// Resolves an `ObjectRef` / `ObjectURef` element in the root scope.
    fn deref(&mut self, n: usize) -> Option<usize> {
        if self.in_island(n) {
            return None;
        }
        if let Some(r) = self.doc.attr(n, "ObjectRef") {
            if self.ambiguous.contains(&format!("id:{r}")) {
                return None;
            }
            let hit = self.ids.get(r).copied();
            if hit.is_none() {
                self.report.warn(format!(
                    "dangling reference {r} at {}",
                    self.doc.location(n)
                ));
            }
            return hit;
        }
        if let Some(r) = self.doc.attr(n, "ObjectURef") {
            if self.ambiguous.contains(&format!("uid:{r}")) {
                return None;
            }
            let hit = self.uids.get(r).copied();
            if hit.is_none() {
                self.report.warn(format!(
                    "dangling reference {r} at {}",
                    self.doc.location(n)
                ));
            }
            return hit;
        }
        None
    }

    fn deref_path(&mut self, n: usize, path: &str) -> Option<usize> {
        let p = self.doc.path(n, path)?;
        self.deref(p)
    }

    fn text(&self, n: usize, path: &str) -> Option<&'a str> {
        self.doc.text_at(n, path).filter(|t| !t.is_empty())
    }

    fn run(&mut self) -> Result<(), ProjectError> {
        let doc = self.doc;
        let project = doc
            .children(0)
            .find(|c| doc.name(*c) == "Project" && doc.attr(*c, "ObjectID").is_some())
            .ok_or_else(|| ProjectError::Format("no Project object".into()))?;
        self.report.project_version = doc.attr(project, "Version").unwrap_or("").into();
        let root_item = self
            .deref_path(project, "RootProjectItem")
            .ok_or_else(|| ProjectError::Format("no root bin".into()))?;
        let root_bin = self.project.root;
        self.walk_bin(root_item, root_bin, 0);
        let seqs: Vec<(usize, SequenceId)> = self.sequences.iter().map(|(k, v)| (*k, *v)).collect();
        for (node, sid) in seqs {
            self.fill_sequence(node, sid);
        }
        self.fit_assets();
        self.project.normalize();
        self.project.validate().map_err(ProjectError::Invalid)?;
        self.report.sequences = self.project.sequences.len();
        Ok(())
    }

    fn walk_bin(&mut self, bin_node: usize, bin: ItemId, depth: usize) {
        if depth > 64 {
            self.report.skip("bins nested too deeply");
            return;
        }
        let Some(items) = self.doc.path(bin_node, "ProjectItemContainer/Items") else {
            return;
        };
        let children: Vec<usize> = self.doc.children_named(items, "Item").collect();
        for it in children {
            let Some(node) = self.deref(it) else { continue };
            match self.doc.name(node) {
                "BinProjectItem" => {
                    let name = self
                        .text(node, "ProjectItem/Name")
                        .unwrap_or("Bin")
                        .to_string();
                    let id = self.project.add_bin(bin, name);
                    self.apply_label(node, id);
                    self.report.bins += 1;
                    self.walk_bin(node, id, depth + 1);
                }
                "ClipProjectItem" => self.clip_item(node, bin),
                other => self.report.skip(format!("project item {other}")),
            }
        }
    }

    fn apply_label(&mut self, node: usize, id: ItemId) {
        let label = self
            .text(
                node,
                "ProjectItem/Node/Properties/Column.PropertyText.Label",
            )
            .and_then(label_index);
        if let (Some(i), Some(item)) = (label, self.project.item_mut(id)) {
            item.label = Label::ALL[i % Label::ALL.len()];
        }
    }

    /// The sources a MasterClip's clips point to: (clip node, source node).
    fn master_sources(&mut self, master: usize) -> Vec<(usize, usize)> {
        let Some(clips) = self.doc.path(master, "Clips") else {
            return vec![];
        };
        let refs: Vec<usize> = self.doc.children_named(clips, "Clip").collect();
        let mut out = Vec::new();
        for r in refs {
            let Some(clip) = self.deref(r) else { continue };
            if let Some(src) = self.deref_path(clip, "Clip/Source") {
                out.push((clip, src));
            }
        }
        out
    }

    fn clip_item(&mut self, node: usize, bin: ItemId) {
        let name = self
            .text(node, "ProjectItem/Name")
            .unwrap_or("Clip")
            .to_string();
        let Some(master) = self.deref_path(node, "MasterClip") else {
            self.report.skip("project item without a master clip");
            return;
        };
        let sources = self.master_sources(master);
        // a sequence item's master clip plays a sequence source
        let seq_node = sources.iter().find_map(|(_, s)| {
            let n = self.doc.name(*s);
            (n == "VideoSequenceSource" || n == "AudioSequenceSource").then_some(*s)
        });
        if let Some(src) = seq_node {
            let Some(seq) = self.deref_path(src, "SequenceSource/Sequence") else {
                self.report.skip("sequence item with a missing sequence");
                return;
            };
            match self.sequences.get(&seq) {
                Some(_) => {}
                None => {
                    let settings = self.sequence_settings(seq);
                    let seq_name = self.text(seq, "Name").unwrap_or(&name).to_string();
                    let (sid, item) = self.project.add_sequence(bin, seq_name, settings);
                    self.sequences.insert(seq, sid);
                    self.masters.insert(master, item);
                    self.apply_label(node, item);
                    // sequence markers live on the master clip's clips
                    let markers = self.markers_of_master(master);
                    let marker_ids: Vec<MarkerId> =
                        markers.iter().map(|_| self.project.ids.marker()).collect();
                    if let Some(s) = self.project.sequence_mut(sid) {
                        for ((start, dur, name, comment), id) in markers.into_iter().zip(marker_ids)
                        {
                            let mut m = Marker::new(id, SeqTime::from_ticks(start));
                            m.duration = Dur(dur);
                            m.name = name;
                            m.comment = comment;
                            s.markers.push(m);
                        }
                    }
                }
            }
            return;
        }
        let media = sources.iter().find_map(|(_, s)| {
            let n = self.doc.name(*s);
            (n == "VideoMediaSource" || n == "AudioMediaSource").then_some(*s)
        });
        let Some(src) = media else {
            let kinds: Vec<&str> = sources.iter().map(|(_, s)| self.doc.name(*s)).collect();
            self.report.skip(format!(
                "project item source {}",
                kinds.first().copied().unwrap_or("none")
            ));
            return;
        };
        let Some(media_node) = self.deref_path(src, "MediaSource/Media") else {
            self.report.skip("media item without media");
            return;
        };
        let asset = self.asset(media_node);
        let item = self.project.add_item(
            bin,
            name,
            ItemKind::Media {
                asset,
                subclip: None,
            },
        );
        self.masters.insert(master, item);
        self.apply_label(node, item);
        let markers = self.markers_of_master(master);
        let marker_ids: Vec<MarkerId> = markers.iter().map(|_| self.project.ids.marker()).collect();
        if let Some(it) = self.project.item_mut(item) {
            for ((start, dur, name, comment), id) in markers.into_iter().zip(marker_ids) {
                let mut m = Marker::new(id, SrcTime::from_ticks(start));
                m.duration = Dur(dur);
                m.name = name;
                m.comment = comment;
                it.markers.push(m);
            }
        }
    }

    fn asset(&mut self, media: usize) -> AssetId {
        if let Some(a) = self.assets.get(&media) {
            return *a;
        }
        let path = self
            .text(media, "ActualMediaFilePath")
            .or_else(|| self.text(media, "FilePath"))
            .unwrap_or("")
            .to_string();
        let video = self
            .deref_path(media, "VideoStream")
            .map(|v| self.video_stream(v));
        let audio: Vec<AudioStream> = self
            .deref_path(media, "AudioStream")
            .map(|a| self.audio_stream(a))
            .into_iter()
            .collect();
        let still = video.as_ref().is_some_and(|v| v.1);
        let video = video.map(|v| v.0);
        let duration = video
            .as_ref()
            .map(|v| v.duration())
            .or_else(|| audio.first().map(|a| a.duration()))
            .unwrap_or(Dur::ZERO);
        let kind = if still {
            MediaKind::Still
        } else if video.is_some() {
            MediaKind::Video
        } else {
            MediaKind::Audio
        };
        let asset = MediaAsset {
            id: AssetId(0),
            path,
            proxy: None,
            kind,
            video,
            audio,
            duration,
            interpretation: Interpretation::default(),
            file_size: 0,
            modified_unix: 0,
        };
        // added without a project item; the caller creates the item
        let id = self.project.ids.asset();
        let mut asset = asset;
        asset.id = id;
        self.project.assets.insert(id, Arc::new(asset));
        self.assets.insert(media, id);
        self.report.media += 1;
        id
    }

    fn video_stream(&mut self, v: usize) -> (VideoStream, bool) {
        let still = self.text(v, "IsStill") == Some("true");
        let per_frame = self
            .text(v, "FrameRate")
            .and_then(|t| t.parse::<i64>().ok())
            .filter(|t| *t > 0);
        let rate = per_frame.map(rate_from_ticks).unwrap_or(Rate::FPS_25);
        let (w, h) = self
            .text(v, "FrameRect")
            .and_then(parse_rect)
            .unwrap_or((1920, 1080));
        let dur = self
            .text(v, "Duration")
            .and_then(|t| t.parse::<i64>().ok())
            .filter(|d| *d > 0);
        let frames = match (dur, per_frame) {
            (Some(d), Some(f)) if !still => (d / f).max(1),
            _ => 1,
        };
        let stream = VideoStream {
            index: 0,
            codec: String::new(),
            width: w,
            height: h,
            pixel_aspect: (1, 1),
            rate,
            frames,
            pixel_format: String::new(),
            bit_depth: 8,
            alpha: AlphaMode::None,
            color: ColorInfo::default(),
            field_order: FieldOrder::Progressive,
            start: Dur::ZERO,
            rotation: 0,
            timecode: None,
        };
        (stream, still)
    }

    fn audio_stream(&mut self, a: usize) -> AudioStream {
        let per_sample = self
            .text(a, "FrameRate")
            .and_then(|t| t.parse::<i64>().ok())
            .filter(|t| *t > 0);
        let sample_rate = per_sample
            .map(|p| (PREMIERE_TICKS_PER_SECOND / p) as u32)
            .unwrap_or(48000);
        let channels = self
            .text(a, "AudioChannelLayout")
            .and_then(|j| serde_json::from_str::<serde_json::Value>(j).ok())
            .and_then(|v| v.as_array().map(|a| a.len() as u16))
            .filter(|n| *n > 0)
            .unwrap_or(2);
        let samples = match (
            self.text(a, "Duration").and_then(|t| t.parse::<i64>().ok()),
            per_sample,
        ) {
            (Some(d), Some(p)) if d > 0 => d / p,
            _ => 0,
        };
        AudioStream {
            index: 0,
            codec: String::new(),
            sample_rate,
            layout: ChannelLayout::from_count(channels),
            samples,
            start: Dur::ZERO,
        }
    }

    fn sequence_settings(&mut self, seq: usize) -> SequenceSettings {
        let mut s = SequenceSettings {
            video_tracks: 1,
            audio_tracks: 1,
            ..SequenceSettings::default()
        };
        let groups: Vec<(String, usize)> = self.track_groups(seq);
        for (kind, g) in groups {
            if kind == VIDEO_GUID {
                if let Some(f) = self
                    .text(g, "TrackGroup/FrameRate")
                    .and_then(|t| t.parse::<i64>().ok())
                    .filter(|t| *t > 0)
                {
                    s.rate = rate_from_ticks(f);
                }
                if let Some((w, h)) = self.text(g, "FrameRect").and_then(parse_rect) {
                    s.width = w;
                    s.height = h;
                }
                if let Some(par) = self.text(g, "PixelAspectRatio").and_then(parse_pair) {
                    s.pixel_aspect = par;
                }
            } else if kind == AUDIO_GUID
                && let Some(f) = self
                    .text(g, "TrackGroup/FrameRate")
                    .and_then(|t| t.parse::<i64>().ok())
                    .filter(|t| *t > 0)
            {
                s.audio_rate = (PREMIERE_TICKS_PER_SECOND / f) as u32;
            }
        }
        s
    }

    fn track_groups(&mut self, seq: usize) -> Vec<(String, usize)> {
        let Some(tg) = self.doc.path(seq, "TrackGroups") else {
            return vec![];
        };
        let groups: Vec<usize> = self.doc.children_named(tg, "TrackGroup").collect();
        let mut out = Vec::new();
        for g in groups {
            let kind = self.text(g, "First").unwrap_or("").to_string();
            if let Some(node) = self.deref_path(g, "Second") {
                out.push((kind, node));
            }
        }
        out
    }

    fn fill_sequence(&mut self, seq_node: usize, sid: SequenceId) {
        let groups = self.track_groups(seq_node);
        let mut video: Vec<Arc<Track>> = Vec::new();
        let mut audio: Vec<Arc<Track>> = Vec::new();
        for (kind, g) in groups {
            let tk = match kind.as_str() {
                VIDEO_GUID => TrackKind::Video,
                AUDIO_GUID => TrackKind::Audio,
                _ => {
                    if self
                        .doc
                        .path(g, "TrackGroup/Tracks")
                        .is_some_and(|t| self.doc.children(t).next().is_some())
                    {
                        self.report.skip("caption/data tracks");
                    }
                    continue;
                }
            };
            let Some(tracks) = self.doc.path(g, "TrackGroup/Tracks") else {
                continue;
            };
            let mut refs: Vec<(i64, usize)> = self
                .doc
                .children_named(tracks, "Track")
                .map(|t| {
                    (
                        self.doc
                            .attr(t, "Index")
                            .and_then(|i| i.parse().ok())
                            .unwrap_or(0),
                        t,
                    )
                })
                .collect();
            refs.sort();
            for (_, r) in refs {
                let Some(tnode) = self.deref(r) else { continue };
                let track = self.track(tnode, tk, sid);
                match tk {
                    TrackKind::Video => video.push(Arc::new(track)),
                    TrackKind::Audio => audio.push(Arc::new(track)),
                }
            }
        }
        let mut ids = self.project.ids.clone();
        if video.is_empty() {
            video.push(Arc::new(Track::new(ids.track(), TrackKind::Video)));
        }
        if audio.is_empty() {
            let mut t = Track::new(ids.track(), TrackKind::Audio);
            t.components = default_components(EffectKind::AudioFixed, &mut ids);
            audio.push(Arc::new(t));
        }
        self.project.ids = ids;
        if let Some(s) = self.project.sequence_mut(sid) {
            s.video = video;
            s.audio = audio;
        }
    }

    fn track(&mut self, tnode: usize, kind: TrackKind, sid: SequenceId) -> Track {
        let id = self.project.ids.track();
        let mut t = Track::new(id, kind);
        let flag = |s: &Self, p: &str| s.text(tnode, p) == Some("true");
        t.locked = flag(self, "ClipTrack/Track/IsLocked");
        t.sync_lock = self.text(tnode, "ClipTrack/Track/IsSyncLocked") != Some("false");
        let muted = flag(self, "ClipTrack/Track/IsMuted");
        match kind {
            // provisional: a muted video track is treated as output disabled
            TrackKind::Video => t.enabled = !muted,
            TrackKind::Audio => {
                t.muted = muted;
                let mut ids = self.project.ids.clone();
                t.components = default_components(EffectKind::AudioFixed, &mut ids);
                self.project.ids = ids;
            }
        }
        let transitions = self.transition_items(tnode);
        let Some(items) = self.doc.path(tnode, "ClipTrack/ClipItems/TrackItems") else {
            return t;
        };
        let refs: Vec<usize> = self.doc.children_named(items, "TrackItem").collect();
        for r in refs {
            let Some(node) = self.deref(r) else { continue };
            if let Some(clip) = self.clip(node, kind, sid) {
                let overlaps = t.clips.iter().any(|c| c.range().overlaps(&clip.range()));
                if overlaps {
                    self.report.skip("overlapping track items");
                    continue;
                }
                let pos = t.clips.partition_point(|c| c.start <= clip.start);
                t.clips.insert(pos, clip);
                self.report.clips += 1;
            }
        }
        self.attach_transitions(&mut t, kind, transitions);
        t
    }

    /// (start, end, identity) of the transition items of a track. Their serialization is not
    /// recognized; times and the component identity are looked up the way
    /// clip items store them, and items without them are skipped.
    fn transition_items(&mut self, tnode: usize) -> Vec<(i64, i64, Option<String>)> {
        let Some(items) = self.doc.path(tnode, "ClipTrack/TransitionItems/TrackItems") else {
            return vec![];
        };
        let refs: Vec<usize> = self.doc.children(items).collect();
        let mut out = Vec::new();
        for r in refs {
            let node = self.deref(r).unwrap_or(r);
            let Some(ti) = self.find_desc(node, "TrackItem") else {
                self.report.skip("transitions without times");
                continue;
            };
            let num =
                |s: &Self, name: &str| s.text(ti, name).and_then(|t| t.trim().parse::<i64>().ok());
            let (Some(start), Some(end)) = (num(self, "Start"), num(self, "End")) else {
                self.report.skip("transitions without times");
                continue;
            };
            if start < 0 || end <= start {
                self.report.skip("transitions without times");
                continue;
            }
            let identity = self.find_identity(node, 4);
            out.push((start, end, identity));
        }
        out
    }

    /// Adds transitions at the clip edges they cover, as the closest effect of ours.
    fn attach_transitions(
        &mut self,
        t: &mut Track,
        kind: TrackKind,
        items: Vec<(i64, i64, Option<String>)>,
    ) {
        let video = kind == TrackKind::Video;
        for (start, end, identity) in items {
            let s = SeqTime::from_ticks(to_ticks(start).0);
            let e = SeqTime::from_ticks(to_ticks(end).0);
            // the edit point inside the transition: an edge shared by two clips, else any edge
            let from = t.clips.iter().find(|c| c.end() >= s && c.end() <= e);
            let to = t.clips.iter().find(|c| c.start >= s && c.start <= e);
            let cut = match (from, to) {
                (Some(a), Some(b)) if a.end() == b.start => a.end(),
                (Some(a), None) => a.end(),
                (None, Some(b)) => b.start,
                (Some(a), Some(_)) => a.end(),
                (None, None) => {
                    self.report.skip("transitions away from clip edges");
                    continue;
                }
            };
            let from = t.clips.iter().find(|c| c.end() == cut).map(|c| c.id);
            let to = t.clips.iter().find(|c| c.start == cut).map(|c| c.id);
            if t.transitions.iter().any(|x| x.cut == cut) {
                continue;
            }
            let effect = match identity
                .as_deref()
                .and_then(|n| catalog::resolve_foreign(n, true, video))
            {
                Some((d, how)) => {
                    if how == catalog::ForeignMatch::Equivalent {
                        self.report.warn(format!(
                            "transition {} imported as {}",
                            identity.as_deref().unwrap_or_default(),
                            d.name
                        ));
                    }
                    d.id
                }
                None => {
                    self.report.warn(format!(
                        "transition {} imported as a {}",
                        identity.as_deref().unwrap_or("of an unknown kind"),
                        if video {
                            "Cross Dissolve"
                        } else {
                            "Constant Power"
                        }
                    ));
                    if video {
                        catalog::CROSS_DISSOLVE
                    } else {
                        catalog::CONSTANT_POWER
                    }
                }
            };
            t.transitions.push(Transition {
                id: self.project.ids.transition(),
                effect: effect.into(),
                cut,
                duration: e - s,
                alignment: Alignment::Custom(cut - s),
                from,
                to,
                params: vec![],
            });
            self.report.transitions += 1;
        }
    }

    /// The first descendant element called `name` (references are not followed).
    fn find_desc(&self, n: usize, name: &str) -> Option<usize> {
        let mut stack: Vec<usize> = self.doc.children(n).collect();
        let mut seen = 0;
        while let Some(c) = stack.pop() {
            seen += 1;
            if seen > 5000 {
                return None;
            }
            if self.doc.name(c) == name {
                return Some(c);
            }
            stack.extend(self.doc.children(c));
        }
        None
    }

    /// A component identity (`MatchName`, or a display name) in the subtree of `n` or in the
    /// objects it references, up to `depth` references away.
    fn find_identity(&mut self, n: usize, depth: usize) -> Option<String> {
        for tag in ["MatchName", "DisplayName"] {
            if let Some(m) = self.find_desc(n, tag) {
                let t = self.doc.text(m).trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        if depth == 0 {
            return None;
        }
        let mut refs = Vec::new();
        let mut stack: Vec<usize> = self.doc.children(n).collect();
        while let Some(c) = stack.pop() {
            if refs.len() > 32 {
                break;
            }
            if self.doc.attr(c, "ObjectRef").is_some() || self.doc.attr(c, "ObjectURef").is_some() {
                refs.push(c);
            } else {
                stack.extend(self.doc.children(c));
            }
        }
        for r in refs {
            if let Some(target) = self.deref(r)
                && let Some(found) = self.find_identity(target, depth - 1)
            {
                return Some(found);
            }
        }
        None
    }

    fn clip(&mut self, node: usize, kind: TrackKind, sid: SequenceId) -> Option<Clip> {
        let start = self
            .text(node, "ClipTrackItem/TrackItem/Start")
            .and_then(|t| t.parse::<i64>().ok())?;
        let end = self
            .text(node, "ClipTrackItem/TrackItem/End")
            .and_then(|t| t.parse::<i64>().ok())?;
        if start < 0 || end <= start {
            self.report.skip("track items with sentinel or empty times");
            return None;
        }
        let sub = self.deref_path(node, "ClipTrackItem/SubClip")?;
        let name = self.text(sub, "Name").unwrap_or("").to_string();
        let master_item = self
            .deref_path(sub, "MasterClip")
            .and_then(|m| self.masters.get(&m).copied());
        let Some(clip_node) = self.deref_path(sub, "Clip") else {
            self.report.skip("track items without a clip");
            return None;
        };
        let src_node = self.deref_path(clip_node, "Clip/Source")?;
        let source = match self.doc.name(src_node) {
            "VideoMediaSource" | "AudioMediaSource" => {
                let media = self.deref_path(src_node, "MediaSource/Media")?;
                let asset = self.asset(media);
                ClipSource::Asset {
                    asset,
                    item: master_item,
                    stream: 0,
                }
            }
            "VideoSequenceSource" | "AudioSequenceSource" => {
                let seq = self.deref_path(src_node, "SequenceSource/Sequence")?;
                match self.sequences.get(&seq) {
                    Some(s) if *s != sid => ClipSource::Sequence {
                        sequence: *s,
                        item: master_item,
                    },
                    _ => {
                        self.report.skip("nested sequences without a project item");
                        return None;
                    }
                }
            }
            other => {
                self.report.skip(format!("clips from {other}"));
                return None;
            }
        };
        let (start_t, r1) = to_ticks(start);
        let (end_t, r2) = to_ticks(end);
        let in_p = self
            .text(clip_node, "Clip/InPoint")
            .and_then(|t| t.parse::<i64>().ok())
            .unwrap_or(0)
            .max(0);
        let out_p = self
            .text(clip_node, "Clip/OutPoint")
            .and_then(|t| t.parse::<i64>().ok());
        let (in_t, r3) = to_ticks(in_p);
        if r1 || r2 || r3 {
            self.report
                .warn("some times were not exact in the editor's time base and were rounded");
        }
        let duration = Dur(end_t - start_t);
        let mut ids = self.project.ids.clone();
        let fixed = if kind == TrackKind::Video {
            EffectKind::VideoFixed
        } else {
            EffectKind::AudioFixed
        };
        let mut clip = Clip {
            id: ids.clip(),
            name,
            kind,
            source,
            start: SeqTime::from_ticks(start_t),
            duration,
            source_in: SrcTime::from_ticks(in_t),
            speed: Speed::NORMAL,
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: default_components(fixed, &mut ids),
            gain_db: 0.0,
            scale_to_frame: false,
            channels: (kind == TrackKind::Audio).then_some(ChannelLayout::Stereo),
        };
        self.project.ids = ids;
        if let Some(out) = out_p.filter(|o| *o > in_p) {
            let (out_t, _) = to_ticks(out);
            let src = Dur(out_t - in_t);
            if src != duration && duration.0 > 0 {
                clip.speed = Speed::from_durations(src, duration);
                self.report
                    .warn("clip speed was derived from source and timeline durations");
            }
        }
        // reverse playback (Clip Speed/Duration > Reverse Speed)
        if self.text(clip_node, "Clip/PlayBackwards") == Some("true") {
            clip.reverse = true;
            self.report
                .warn("reversed clips were imported; their source range is not yet verified");
        }
        if let Some(chain) = self.deref_path(node, "ClipTrackItem/ComponentOwner/Components") {
            self.components(chain, &mut clip);
        }
        if clip.name.is_empty() {
            clip.name = self.project.source_name(&clip.source);
        }
        Some(clip)
    }

    fn components(&mut self, chain: usize, clip: &mut Clip) {
        let Some(list) = self.doc.path(chain, "ComponentChain/Components") else {
            return;
        };
        let refs: Vec<usize> = self.doc.children_named(list, "Component").collect();
        for r in refs {
            let Some(comp) = self.deref(r) else { continue };
            let Some(match_name) = self.text(comp, "MatchName").map(str::to_string) else {
                self.report.skip("components without a match name");
                continue;
            };
            let bypass = self.text(comp, "Component/Bypass") == Some("true");
            let params = self.params(comp);
            match catalog::find_match_name(&match_name) {
                Some(def)
                    if def.kind == EffectKind::VideoFixed
                        && clip.kind == TrackKind::Video
                        && def.id == catalog::MOTION =>
                {
                    if let Some(m) = clip.component_mut(catalog::MOTION) {
                        let mapped = apply_params(m, &params, MOTION_MAP);
                        m.enabled = !bypass;
                        if mapped > 0 {
                            self.report.components_mapped += 1;
                        }
                    }
                }
                Some(def)
                    if def.kind == EffectKind::VideoEffect
                        && clip.kind == TrackKind::Video
                        && param_map(def.id).is_some() =>
                {
                    let mut ids = self.project.ids.clone();
                    let mut c = Component::new(def, &mut ids);
                    self.project.ids = ids;
                    apply_params(&mut c, &params, param_map(def.id).unwrap());
                    c.enabled = !bypass;
                    clip.components.push(c);
                    self.report.components_mapped += 1;
                    if def.id == "op.video.lumetri" {
                        self.report
                            .warn("Lumetri Color basic values were imported; the look may differ");
                    }
                }
                Some(def) if def.id == catalog::OPACITY && clip.kind == TrackKind::Video => {
                    if let Some(o) = clip.component_mut(catalog::OPACITY) {
                        let mapped = apply_params(o, &params, OPACITY_MAP);
                        o.enabled = !bypass;
                        if mapped > 0 {
                            self.report.components_mapped += 1;
                        }
                    }
                }
                _ if let Some((def, map)) = self.equivalent(comp, &match_name, clip.kind) => {
                    let mut ids = self.project.ids.clone();
                    let mut c = Component::new(def, &mut ids);
                    self.project.ids = ids;
                    if let Some(map) = map {
                        apply_params(&mut c, &params, map);
                    }
                    c.enabled = !bypass;
                    clip.components.push(c);
                    self.report.components_equivalent += 1;
                    self.report.warn(format!(
                        "effect {match_name} imported as {}{}",
                        def.name,
                        if map.is_some() {
                            ""
                        } else {
                            " (with its default settings)"
                        }
                    ));
                }
                _ => {
                    let raw = self.doc.subtree_xml(comp, MAX_OPAQUE);
                    let id = self.project.ids.component();
                    clip.components.push(Component {
                        id,
                        effect: match_name.clone(),
                        enabled: false,
                        params: Vec::new(),
                        foreign: raw,
                    });
                    self.report.components_opaque += 1;
                    self.report
                        .warn(format!("effect {match_name} is kept but not rendered"));
                }
            }
        }
    }

    /// Our equivalent of a component we do not implement under its own identity, and the
    /// parameter layout to read when it is known.
    fn equivalent(
        &mut self,
        comp: usize,
        match_name: &str,
        kind: TrackKind,
    ) -> Option<(&'static catalog::EffectDef, Option<ParamMap>)> {
        let video = kind == TrackKind::Video;
        let found = catalog::resolve_foreign(match_name, false, video).or_else(|| {
            // third-party effects are often recognizable only by their display name
            let name = self
                .text(comp, "Component/DisplayName")
                .or_else(|| self.text(comp, "DisplayName"))?;
            catalog::resolve_foreign(name, false, video)
        })?;
        Some((found.0, foreign_param_map(match_name)))
    }

    /// (ParameterID, animated, values) of a component's parameters.
    fn params(&mut self, comp: usize) -> Vec<(u32, ParsedParam)> {
        let Some(list) = self.doc.path(comp, "Component/Params") else {
            return vec![];
        };
        let refs: Vec<usize> = self.doc.children_named(list, "Param").collect();
        let mut out = Vec::new();
        for r in refs {
            let Some(p) = self.deref(r) else { continue };
            let Some(pid) = self
                .text(p, "ParameterID")
                .and_then(|t| t.parse::<u32>().ok())
            else {
                continue;
            };
            let varying = self.text(p, "IsTimeVarying") == Some("true");
            let start = self
                .text(p, "StartKeyframe")
                .and_then(|t| t.split(',').nth(1))
                .map(str::to_string);
            let current = self.text(p, "CurrentValue").map(str::to_string);
            let mut keys = Vec::new();
            if varying && let Some(list) = self.text(p, "Keyframes") {
                for k in list
                    .split(';')
                    .filter(|k| !k.trim().is_empty())
                    .take(100_000)
                {
                    let mut f = k.split(',');
                    let (Some(t), Some(v)) = (f.next(), f.next()) else {
                        continue;
                    };
                    let Ok(t) = t.trim().parse::<i64>() else {
                        continue;
                    };
                    if t < 0 {
                        continue;
                    }
                    keys.push((to_ticks(t).0, v.trim().to_string()));
                }
                if !keys.is_empty() {
                    self.report.warn("keyframes were imported with linear interpolation; their timing is not yet verified");
                }
            }
            out.push((
                pid,
                ParsedParam {
                    value: current.or(start),
                    keys,
                },
            ));
        }
        out
    }

    fn markers_of_master(&mut self, master: usize) -> Vec<(i64, i64, String, String)> {
        let mut out = Vec::new();
        let sources = self.master_sources(master);
        let Some((clip, _)) = sources.first().copied() else {
            return out;
        };
        let Some(markers) = self.deref_path(clip, "Clip/MarkerOwner/Markers") else {
            return out;
        };
        let Some(list) = self.doc.path(markers, "Markers") else {
            return out;
        };
        let entries: Vec<usize> = self.doc.children_named(list, "Marker").collect();
        for e in entries {
            let Some(m) = self.deref_path(e, "Second") else {
                continue;
            };
            let Some(json) = self.text(m, "DVAMarker") else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
                self.report.skip("markers with unreadable data");
                continue;
            };
            let d = &v["DVAMarker"];
            let Some(start) = d["mStartTime"]["ticks"].as_i64().or_else(|| {
                d["mStartTime"]["ticks"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
            }) else {
                continue;
            };
            let dur = d["mDuration"]["ticks"]
                .as_i64()
                .or_else(|| {
                    d["mDuration"]["ticks"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(0);
            let name = d["mName"].as_str().unwrap_or("").to_string();
            let comment = d["mComment"].as_str().unwrap_or("").to_string();
            out.push((
                to_ticks(start.max(0)).0,
                to_ticks(dur.max(0)).0,
                name,
                comment,
            ));
            self.report.markers += 1;
        }
        out
    }

    /// Stretches imported asset durations over the ranges their clips use (durations in the file
    /// are not validated), so the imported timeline stays valid.
    fn fit_assets(&mut self) {
        let mut need: HashMap<AssetId, Dur> = HashMap::new();
        for s in self.project.sequences.values() {
            for (_, c) in s.clips() {
                if let ClipSource::Asset { asset, .. } = c.source {
                    let end = c.source_range().end.since_zero();
                    let e = need.entry(asset).or_insert(Dur::ZERO);
                    *e = (*e).max(end);
                }
            }
        }
        for (id, end) in need {
            let Some(a) = self.project.assets.get_mut(&id) else {
                continue;
            };
            let have = a.available().map(|r| r.end.since_zero());
            if have.is_some_and(|h| h < end) {
                let a = Arc::make_mut(a);
                a.duration = end;
                if let Some(v) = &mut a.video {
                    v.frames = v.rate.dur_to_frames_ceil(end).max(v.frames);
                }
                for s in &mut a.audio {
                    s.samples = s.rate().dur_to_frames_ceil(end).max(s.samples);
                }
                self.report
                    .warn("some media durations were extended to cover their clips");
            }
        }
    }
}

struct ParsedParam {
    value: Option<String>,
    keys: Vec<(i64, String)>,
}

/// ParameterID -> our parameter key, for components whose parameter order is known.
type ParamMap = &'static [(u32, &'static str)];

const MOTION_MAP: ParamMap = &[
    (1, "position"),
    (2, "scale"),
    (3, "scale_width"),
    (4, "uniform_scale"),
    (5, "rotation"),
    (6, "anchor"),
];

const LUMETRI_MAP: ParamMap = &[
    (11, "temperature"),
    (12, "tint"),
    (15, "exposure"),
    (16, "contrast"),
    (17, "highlights"),
    (18, "shadows"),
    (19, "whites"),
    (20, "blacks"),
    (25, "saturation"),
    (32, "look_intensity"),
    (34, "faded_film"),
    (35, "sharpen"),
    (36, "vibrance"),
    (37, "creative_saturation"),
    (39, "tint_balance"),
    (86, "vignette_amount"),
    (87, "vignette_midpoint"),
    (88, "vignette_roundness"),
    (89, "vignette_feather"),
];

const MOSAIC_MAP: ParamMap = &[(1, "horizontal"), (2, "vertical")];
/// Opacity's first parameter; the blend mode is not mapped yet.
const OPACITY_MAP: ParamMap = &[(1, "opacity")];
/// Fast Blur's parameters, read into Gaussian Blur.
const FAST_BLUR_MAP: ParamMap = &[(1, "blurriness"), (2, "dimensions"), (3, "repeat_edge")];

/// Parameter layouts of foreign components imported as an equivalent effect.
fn foreign_param_map(match_name: &str) -> Option<ParamMap> {
    match match_name {
        "AE.ADBE Fast Blur" => Some(FAST_BLUR_MAP),
        _ => None,
    }
}
const TWIRL_MAP: ParamMap = &[(1, "angle"), (2, "radius"), (3, "center")];
const SOLARIZE_MAP: ParamMap = &[(1, "threshold")];

fn param_map(effect: &str) -> Option<ParamMap> {
    match effect {
        "op.video.lumetri" => Some(LUMETRI_MAP),
        "op.video.mosaic" => Some(MOSAIC_MAP),
        "op.video.twirl" => Some(TWIRL_MAP),
        "op.video.solarize" => Some(SOLARIZE_MAP),
        _ => None,
    }
}

/// Converts a textual value to the type of the existing parameter.
fn convert(template: &Value, text: &str) -> Option<Value> {
    let t = text.trim();
    Some(match template {
        Value::Bool(_) => Value::Bool(t == "true" || t == "1"),
        Value::Int(_) => Value::Int(t.parse::<f64>().ok()?.round() as i64),
        Value::Float(_) => Value::Float(
            t.trim_end_matches('.')
                .parse::<f64>()
                .or_else(|_| t.parse::<f64>())
                .ok()?,
        ),
        Value::Choice(_) => Value::Choice(t.parse::<f64>().ok()?.max(0.0) as u32),
        Value::Point(_) => {
            let (x, y) = t.split_once(':')?;
            Value::Point([x.trim().parse().ok()?, y.trim().parse().ok()?])
        }
        _ => return None,
    })
}

fn apply_params(c: &mut Component, params: &[(u32, ParsedParam)], map: ParamMap) -> usize {
    let def = c.def();
    let mut n = 0;
    for (pid, parsed) in params {
        let Some((_, key)) = map.iter().find(|(id, _)| id == pid) else {
            continue;
        };
        let spec = def.and_then(|d| d.param(key));
        let Some(p) = c.param_mut(key) else { continue };
        let template = p.value.clone();
        if let Some(v) = parsed.value.as_deref().and_then(|t| convert(&template, t)) {
            p.value = spec.map(|s| s.clamp(v.clone())).unwrap_or(v);
            n += 1;
        }
        if !parsed.keys.is_empty() && spec.is_some_and(|s| s.animatable) {
            let keys: Vec<Keyframe> = parsed
                .keys
                .iter()
                .filter_map(|(t, v)| {
                    convert(&template, v)
                        .map(|v| Keyframe::new(SrcTime::from_ticks(*t), v, Interp::Linear))
                })
                .collect();
            if !keys.is_empty() {
                p.keys = keys;
                p.animated = true;
                p.sort_keys();
            }
        }
    }
    n
}

/// Converts Premiere ticks to ours; the flag reports rounding.
fn to_ticks(premiere: i64) -> (i64, bool) {
    let q = premiere.div_euclid(TICK_RATIO);
    let r = premiere.rem_euclid(TICK_RATIO);
    if r * 2 >= TICK_RATIO {
        (q + 1, r != 0)
    } else {
        (q, r != 0)
    }
}

/// Frame rate from ticks per frame.
fn rate_from_ticks(per_frame: i64) -> Rate {
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(PREMIERE_TICKS_PER_SECOND, per_frame).max(1);
    let num = PREMIERE_TICKS_PER_SECOND / g;
    let den = per_frame / g;
    if num <= u32::MAX as i64 && den <= u32::MAX as i64 && den > 0 {
        Rate::new(num as u32, den as u32)
    } else {
        Rate::from_f64(PREMIERE_TICKS_PER_SECOND as f64 / per_frame as f64)
    }
}

fn parse_rect(t: &str) -> Option<(u32, u32)> {
    let v: Vec<i64> = t.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    if v.len() != 4 {
        return None;
    }
    let (w, h) = (v[2] - v[0], v[3] - v[1]);
    (w > 0 && h > 0 && w <= 32768 && h <= 32768).then_some((w as u32, h as u32))
}

fn parse_pair(t: &str) -> Option<(u32, u32)> {
    let (a, b) = t.split_once(',')?;
    let a: u32 = a.trim().parse().ok()?;
    let b: u32 = b.trim().parse().ok()?;
    (a > 0 && b > 0).then_some((a, b))
}

fn label_index(t: &str) -> Option<usize> {
    t.rsplit('.').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal synthetic project written for these tests (not derived from any shipped file):
    /// one bin, one media item, one sequence with a clip that has a Motion component, and an
    /// inline island that reuses a root ObjectID.
    const SYNTHETIC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PremiereData Version="3">
  <Project ObjectRef="1"/>
  <Project ObjectID="1" ClassID="p" Version="42">
    <Node><Properties><ProjectViewState.List ObjectID="30"><Entry ObjectID="10"/><Ref ObjectRef="10"/></ProjectViewState.List></Properties></Node>
    <RootProjectItem ObjectURef="root"/>
  </Project>
  <RootProjectItem ObjectUID="root" ClassID="r" Version="1">
    <ProjectItem><Name>Root Bin</Name></ProjectItem>
    <ProjectItemContainer><Items>
      <Item Index="0" ObjectURef="bin1"/>
      <Item Index="1" ObjectURef="seqitem"/>
    </Items></ProjectItemContainer>
  </RootProjectItem>
  <BinProjectItem ObjectUID="bin1" ClassID="b" Version="1">
    <ProjectItem><Node><Properties><Column.PropertyText.Label>BE.Prefs.LabelColors.3</Column.PropertyText.Label></Properties></Node><Name>Footage</Name></ProjectItem>
    <ProjectItemContainer><Items><Item Index="0" ObjectURef="clipitem"/></Items></ProjectItemContainer>
  </BinProjectItem>
  <ClipProjectItem ObjectUID="clipitem" ClassID="c" Version="1">
    <ProjectItem><Name>interview.mov</Name></ProjectItem>
    <MasterClip ObjectURef="mc1"/>
  </ClipProjectItem>
  <MasterClip ObjectUID="mc1" ClassID="m" Version="11"><Clips><Clip Index="0" ObjectRef="10"/></Clips></MasterClip>
  <VideoClip ObjectID="10" ClassID="vc" Version="11"><Clip><Source ObjectRef="11"/></Clip></VideoClip>
  <VideoMediaSource ObjectID="11" ClassID="vms" Version="2"><MediaSource><Media ObjectURef="media1"/></MediaSource></VideoMediaSource>
  <Media ObjectUID="media1" ClassID="md" Version="27">
    <VideoStream ObjectRef="12"/>
    <FilePath>C:\media\interview.mov</FilePath>
  </Media>
  <VideoStream ObjectID="12" ClassID="vs" Version="19">
    <FrameRate>10160640000</FrameRate><FrameRect>0,0,1920,1080</FrameRect><Duration>2540160000000</Duration>
  </VideoStream>
  <ClipProjectItem ObjectUID="seqitem" ClassID="c" Version="1">
    <ProjectItem><Name>Edit</Name></ProjectItem>
    <MasterClip ObjectURef="mc2"/>
  </ClipProjectItem>
  <MasterClip ObjectUID="mc2" ClassID="m" Version="11"><Clips><Clip Index="0" ObjectRef="20"/></Clips></MasterClip>
  <VideoClip ObjectID="20" ClassID="vc" Version="11"><Clip><MarkerOwner><Markers ObjectRef="25"/></MarkerOwner><Source ObjectRef="21"/></Clip></VideoClip>
  <Markers ObjectID="25" ClassID="mk" Version="4"><Markers><Marker Index="0"><First>g</First><Second ObjectRef="26"/></Marker></Markers></Markers>
  <Marker ObjectID="26" ClassID="m" Version="3"><DVAMarker>{"DVAMarker":{"mName":"Chapter","mStartTime":{"ticks":254016000000},"mType":"Comment"}}</DVAMarker></Marker>
  <VideoSequenceSource ObjectID="21" ClassID="vss" Version="3"><SequenceSource><Sequence ObjectURef="seq1"/></SequenceSource></VideoSequenceSource>
  <Sequence ObjectUID="seq1" ClassID="s" Version="11">
    <TrackGroups>
      <TrackGroup Index="0"><First>228cda18-3625-4d2d-951e-348879e4ed93</First><Second ObjectRef="40"/></TrackGroup>
      <TrackGroup Index="1"><First>80b8e3d5-6dca-4195-aefb-cb5f407ab009</First><Second ObjectRef="41"/></TrackGroup>
    </TrackGroups>
    <Name>Edit</Name>
  </Sequence>
  <VideoTrackGroup ObjectID="40" ClassID="vtg" Version="13">
    <TrackGroup><Tracks><Track Index="0" ObjectURef="vt1"/></Tracks><FrameRate>10594584000</FrameRate></TrackGroup>
    <FrameRect>0,0,1280,720</FrameRect><PixelAspectRatio>1,1</PixelAspectRatio>
  </VideoTrackGroup>
  <AudioTrackGroup ObjectID="41" ClassID="atg" Version="6">
    <TrackGroup><Tracks/><FrameRate>5292000</FrameRate></TrackGroup>
  </AudioTrackGroup>
  <VideoClipTrack ObjectUID="vt1" ClassID="vct" Version="1">
    <ClipTrack><Track><IsLocked>false</IsLocked><IsSyncLocked>true</IsSyncLocked></Track>
      <ClipItems><TrackItems><TrackItem Index="0" ObjectRef="50"/></TrackItems></ClipItems>
    </ClipTrack>
  </VideoClipTrack>
  <VideoClipTrackItem ObjectID="50" ClassID="vcti" Version="6">
    <ClipTrackItem>
      <ComponentOwner><Components ObjectRef="60"/></ComponentOwner>
      <TrackItem><Start>254016000000</Start><End>762048000000</End></TrackItem>
      <SubClip ObjectRef="51"/>
    </ClipTrackItem>
  </VideoClipTrackItem>
  <SubClip ObjectID="51" ClassID="sc" Version="5"><Clip ObjectRef="52"/><MasterClip ObjectURef="mc1"/><Name>interview.mov</Name></SubClip>
  <VideoClip ObjectID="52" ClassID="vc" Version="11"><Clip><Source ObjectRef="11"/><InPoint>508032000000</InPoint><OutPoint>1016064000000</OutPoint><PlayBackwards>true</PlayBackwards></Clip></VideoClip>
  <VideoComponentChain ObjectID="60" ClassID="vcc" Version="3"><ComponentChain><Components><Component Index="0" ObjectRef="61"/><Component Index="1" ObjectRef="62"/></Components></ComponentChain></VideoComponentChain>
  <VideoFilterComponent ObjectID="61" ClassID="vfc" Version="9">
    <Component><Params><Param Index="0" ObjectRef="70"/><Param Index="1" ObjectRef="71"/></Params></Component>
    <MatchName>AE.ADBE Motion</MatchName>
  </VideoFilterComponent>
  <PointComponentParam ObjectID="70" ClassID="pcp" Version="1"><StartKeyframe>-91445760000000000,0.25:0.75,0,0,0,0,0,0</StartKeyframe><ParameterID>1</ParameterID></PointComponentParam>
  <VideoComponentParam ObjectID="71" ClassID="vcp" Version="1"><StartKeyframe>-91445760000000000,50.,0,0,0,0,0,0</StartKeyframe><ParameterID>2</ParameterID></VideoComponentParam>
  <VideoFilterComponent ObjectID="62" ClassID="vfc" Version="9"><Component><Params/></Component><MatchName>AE.ADBE Unknown Effect</MatchName></VideoFilterComponent>
</PremiereData>"#;

    fn gz(text: &str) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(text.as_bytes()).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn imports_the_synthetic_graph() {
        let (p, report) = import(&gz(SYNTHETIC), "demo").unwrap();
        assert_eq!(
            (report.bins, report.media, report.sequences, report.clips),
            (1, 1, 1, 1)
        );
        let seq = p.sequences.values().next().unwrap();
        assert_eq!(seq.name, "Edit");
        assert_eq!(seq.settings.rate, Rate::FPS_23_976);
        assert_eq!((seq.settings.width, seq.settings.height), (1280, 720));
        assert_eq!(seq.settings.audio_rate, 48000);
        let clip = &seq.video[0].clips[0];
        assert_eq!(clip.start, SeqTime::from_seconds(1.0));
        assert_eq!(clip.duration, Dur::from_seconds(2.0));
        assert_eq!(clip.source_in, SrcTime::from_seconds(2.0));
        assert!(clip.reverse);
        let motion = clip.component(catalog::MOTION).unwrap();
        assert_eq!(
            motion.param("position").unwrap().value,
            Value::Point([0.25, 0.75])
        );
        assert_eq!(motion.param("scale").unwrap().value, Value::Float(50.0));
        let opaque = clip
            .components
            .iter()
            .find(|c| c.effect == "AE.ADBE Unknown Effect")
            .unwrap();
        assert!(!opaque.enabled && opaque.foreign.as_ref().unwrap().contains("Unknown"));
        assert_eq!(seq.markers.len(), 1);
        assert_eq!(seq.markers[0].name, "Chapter");
        let bin = p
            .walk()
            .iter()
            .map(|(_, i)| p.item(*i).unwrap().clone())
            .find(|i| i.is_bin())
            .unwrap();
        assert_eq!(bin.name, "Footage");
        assert_eq!(bin.label, Label::ALL[3]);
        // the clip references the project item of its master clip
        assert!(matches!(
            clip.source,
            ClipSource::Asset { item: Some(_), .. }
        ));
    }

    /// Transition items are mapped by the component identity they reference and attached to
    /// the clip edge they cover; unimplemented effects become our equivalent. The transition
    /// layout here is synthetic: no fixture with populated transitions exists yet.
    #[test]
    fn transitions_and_equivalent_effects() {
        let xml = SYNTHETIC
            .replace(
                "<ClipItems>",
                r#"<TransitionItems><TrackItems><TrackItem Index="0" ObjectRef="90"/></TrackItems></TransitionItems><ClipItems>"#,
            )
            .replace(
                "</PremiereData>",
                r#"<VideoTransitionTrackItem ObjectID="90" ClassID="vtti" Version="1"><TransitionTrackItem><TrackItem><Start>635040000000</Start><End>762048000000</End></TrackItem><Component ObjectRef="91"/></TransitionTrackItem></VideoTransitionTrackItem>
<VideoTransitionComponent ObjectID="91" ClassID="vtc" Version="1"><MatchName>PR.ADBE Dip To Black</MatchName></VideoTransitionComponent>
</PremiereData>"#,
            )
            .replace(
                "<MatchName>AE.ADBE Unknown Effect</MatchName>",
                "<MatchName>AE.ADBE Deep Glow</MatchName>",
            );
        let (p, report) = import(&gz(&xml), "demo").unwrap();
        assert_eq!(report.transitions, 1, "{report:?}");
        assert_eq!(report.components_equivalent, 1, "{report:?}");
        let seq = &p.sequences.values().next().unwrap();
        let track = &seq.video[0];
        let tr = &track.transitions[0];
        assert_eq!(tr.effect, "op.tr.dip_to_black");
        let clip = &track.clips[0];
        assert_eq!(tr.cut, clip.end());
        assert_eq!(tr.from, Some(clip.id));
        assert_eq!(tr.duration, Dur::from_seconds(0.5));
        assert!(clip.component("op.video.radiant_glow").is_some());
    }

    #[test]
    fn island_identities_do_not_leak_into_the_root_scope() {
        let doc = xml::parse(SYNTHETIC, Limits::default()).unwrap();
        let mut im = Importer::new(&doc, "x");
        // the reference inside ProjectViewState.List is inside an island and is not followed
        let island_ref = (0..doc.nodes.len())
            .find(|i| doc.name(*i) == "Ref")
            .unwrap();
        assert!(im.in_island(island_ref));
        assert_eq!(im.deref(island_ref), None);
        // the root-level VideoClip 10 is still resolvable from a root-level MasterClip
        let mc = (0..doc.nodes.len())
            .find(|i| doc.attr(*i, "ObjectUID") == Some("mc1"))
            .unwrap();
        let clip_ref = doc.path(mc, "Clips/Clip").unwrap();
        assert_eq!(doc.name(im.deref(clip_ref).unwrap()), "VideoClip");
    }

    #[test]
    fn damaged_containers_are_refused() {
        let mut bytes = gz(SYNTHETIC);
        assert!(import(b"<PremiereData/>", "x").is_err());
        let n = bytes.len();
        bytes[n - 6] ^= 0xFF; // checksum
        assert!(decompress(&bytes).is_err());
        let mut trailing = gz(SYNTHETIC);
        trailing.extend_from_slice(b"junk");
        assert!(decompress(&trailing).is_err());
    }

    #[test]
    fn premiere_ticks_convert_exactly() {
        assert_eq!(
            to_ticks(PREMIERE_TICKS_PER_SECOND),
            (TICKS_PER_SECOND, false)
        );
        assert_eq!(rate_from_ticks(10160640000), Rate::FPS_25);
        assert_eq!(rate_from_ticks(8475667200), Rate::FPS_29_97);
    }

    /// Runs against locally available Premiere projects when OPENPREMIER_PRPROJ_DIR is set.
    /// These files are never committed (docs/legal/clean-room.md).
    #[test]
    fn local_corpus_imports_without_errors() {
        let Some(dir) = std::env::var_os("OPENPREMIER_PRPROJ_DIR") else {
            return;
        };
        let mut n = 0;
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("prproj") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let (p, report) =
                import(&bytes, "corpus").unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            eprintln!(
                "{}: {:?} sequences, {} clips, skipped {:?}",
                path.display(),
                p.sequences.len(),
                report.clips,
                report.skipped
            );
            n += 1;
        }
        eprintln!("{n} projects imported");
    }
}
