//! Editing policy (docs/timeline-behavior.md).
//!
//! Every operation works on a checked-out copy of one sequence and returns the new clip
//! selection. Callers wrap operations in `Project::transact`, so an operation that fails
//! validation leaves the project unchanged. Behaviors the specification still lists as open are
//! implemented with the documented candidate rule and marked "provisional" in comments.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use op_core::catalog::EffectKind;
use op_core::*;

pub mod arrange;
pub mod edit;
pub mod nav;
pub mod snap;
mod stress;
pub mod transitions;
pub mod trim;

pub use arrange::*;
pub use edit::*;
pub use nav::*;
pub use snap::*;
pub use transitions::*;
pub use trim::*;

/// User options that change how edits propagate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditOptions {
    /// Selecting or editing a clip includes its linked A/V partners (TL-SEL-002).
    pub linked_selection: bool,
    /// Sequence markers move with ripple edits.
    pub ripple_markers: bool,
}

impl Default for EditOptions {
    fn default() -> Self {
        EditOptions {
            linked_selection: true,
            ripple_markers: true,
        }
    }
}

/// A checked-out sequence being edited.
pub(crate) struct Ed<'a> {
    pub p: &'a Project,
    pub seq: Sequence,
    pub ids: IdGen,
    pub opts: EditOptions,
}

impl<'a> Ed<'a> {
    pub fn new(p: &'a Project, sid: SequenceId, opts: EditOptions) -> EditResult<Ed<'a>> {
        let seq = p
            .sequence(sid)
            .ok_or_else(|| EditError::NotFound("sequence".into()))?
            .clone();
        Ok(Ed {
            p,
            seq,
            ids: p.ids.clone(),
            opts,
        })
    }

    pub fn finish(self) -> (Sequence, IdGen) {
        (self.seq, self.ids)
    }

    pub fn frame(&self) -> Dur {
        self.seq.settings.frame_duration()
    }

    pub fn track(&self, r: TrackRef) -> EditResult<&Track> {
        self.seq
            .track(r)
            .ok_or_else(|| EditError::NotFound(format!("track {r:?}")))
    }

    pub fn track_mut(&mut self, r: TrackRef) -> EditResult<&mut Track> {
        self.seq
            .track_mut(r)
            .ok_or_else(|| EditError::NotFound(format!("track {r:?}")))
    }

    /// Fails if the track is locked.
    pub fn writable(&self, r: TrackRef) -> EditResult {
        if self.track(r)?.locked {
            return Err(EditError::Locked(self.seq.track_name(r)));
        }
        Ok(())
    }

    pub fn clip(&self, id: ClipId) -> EditResult<(TrackRef, Clip)> {
        self.seq
            .find_clip(id)
            .map(|(r, c)| (r, c.clone()))
            .ok_or_else(|| EditError::NotFound(format!("{id}")))
    }

    /// Expands a set of clips with their linked partners when linked selection is on.
    pub fn with_links(&self, ids: &[ClipId]) -> Vec<ClipId> {
        let mut out: Vec<ClipId> = Vec::new();
        for &id in ids {
            let set = if self.opts.linked_selection {
                self.seq.linked(id)
            } else {
                vec![id]
            };
            for c in set {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }

    pub fn available(&self, c: &Clip) -> Option<SrcRange> {
        self.p.available(&c.source)
    }

    /// Unlocked tracks that follow ripple edits because their sync lock is on.
    pub fn sync_tracks(&self) -> Vec<TrackRef> {
        self.seq
            .all_tracks()
            .filter(|(_, t)| t.sync_lock && !t.locked)
            .map(|(r, _)| r)
            .collect()
    }

    pub fn all_refs(&self) -> Vec<TrackRef> {
        self.seq.all_tracks().map(|(r, _)| r).collect()
    }

    /// Builds a new clip with the fixed components of its kind.
    pub fn new_clip(
        &mut self,
        kind: TrackKind,
        source: ClipSource,
        name: &str,
        start: SeqTime,
        duration: Dur,
        source_in: SrcTime,
    ) -> Clip {
        let fixed = if kind == TrackKind::Video {
            EffectKind::VideoFixed
        } else {
            EffectKind::AudioFixed
        };
        Clip {
            id: self.ids.clip(),
            name: name.to_string(),
            kind,
            source,
            start,
            duration,
            source_in,
            speed: Speed::NORMAL,
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: default_components(fixed, &mut self.ids),
            gain_db: 0.0,
            scale_to_frame: false,
            channels: None,
        }
    }

    /// Adds a clip to a track (the range must already be free).
    pub fn place(&mut self, r: TrackRef, clip: Clip) -> EditResult<ClipId> {
        let id = clip.id;
        let t = self.track_mut(r)?;
        let pos = t.clips.partition_point(|c| c.start <= clip.start);
        t.clips.insert(pos, clip);
        Ok(id)
    }

    pub fn remove(&mut self, id: ClipId) -> EditResult<Clip> {
        let (r, _) = self.clip(id)?;
        let t = self.track_mut(r)?;
        let i = t.index_of(id).unwrap();
        Ok(t.clips.remove(i))
    }

    /// Splits clips at `t`. Right halves get new IDs; halves of linked clips split together stay
    /// linked to each other. Returns (left, right) pairs.
    pub fn split(&mut self, ids: &[ClipId], t: SeqTime) -> Vec<(ClipId, ClipId)> {
        let mut link_map: HashMap<LinkId, LinkId> = HashMap::new();
        let mut out = Vec::new();
        for &id in ids {
            let Ok((r, clip)) = self.clip(id) else {
                continue;
            };
            if !(clip.start < t && t < clip.end()) {
                continue;
            }
            let left_dur = t - clip.start;
            let right_dur = clip.end() - t;
            let mut right = clip.clone();
            right.id = self.ids.clip();
            right.start = t;
            right.duration = right_dur;
            for c in &mut right.components {
                c.id = self.ids.component();
            }
            right.link = clip
                .link
                .map(|l| *link_map.entry(l).or_insert_with(|| self.ids.link()));
            let mut left = clip.clone();
            left.duration = left_dur;
            if clip.hold.is_none() {
                if clip.reverse {
                    let right_src = clip.speed.to_source(right_dur);
                    left.source_in = clip.source_in + right_src;
                    right.source_in = clip.source_in;
                } else {
                    right.source_in = clip.source_in + clip.speed.to_source(left_dur);
                }
            }
            let old_end = clip.end();
            let right_id = right.id;
            let track = self.seq.track_mut(r).unwrap();
            let i = track.index_of(id).unwrap();
            track.clips[i] = left;
            track.clips.insert(i + 1, right);
            for tr in &mut track.transitions {
                if tr.from == Some(id) && tr.cut == old_end {
                    tr.from = Some(right_id);
                }
            }
            out.push((id, right_id));
        }
        out
    }

    /// Removes all material inside `range` on the given tracks, splitting clips at its edges.
    pub fn clear_range(&mut self, tracks: &[TrackRef], range: SeqRange) -> EditResult {
        if range.is_empty() {
            return Ok(());
        }
        for &r in tracks {
            self.writable(r)?;
        }
        let at_start: Vec<ClipId> = tracks
            .iter()
            .filter_map(|r| self.seq.track(*r))
            .flat_map(|t| {
                t.clips
                    .iter()
                    .filter(|c| c.start < range.start && c.end() > range.start)
                    .map(|c| c.id)
            })
            .collect();
        self.split(&at_start, range.start);
        let at_end: Vec<ClipId> = tracks
            .iter()
            .filter_map(|r| self.seq.track(*r))
            .flat_map(|t| {
                t.clips
                    .iter()
                    .filter(|c| c.start < range.end && c.end() > range.end)
                    .map(|c| c.id)
            })
            .collect();
        self.split(&at_end, range.end);
        for &r in tracks {
            let t = self.track_mut(r)?;
            t.clips
                .retain(|c| !(c.start >= range.start && c.end() <= range.end));
        }
        Ok(())
    }

    /// Shifts every clip starting at or after `from` on the given tracks by `delta`, and the
    /// sequence markers when rippling markers.
    pub fn shift_after(
        &mut self,
        tracks: &[TrackRef],
        from: SeqTime,
        delta: Dur,
        markers: bool,
    ) -> EditResult {
        if delta.0 == 0 {
            return Ok(());
        }
        for &r in tracks {
            let moves = self.track(r)?.clips.iter().any(|c| c.start >= from);
            if !moves {
                continue;
            }
            let t = self.track_mut(r)?;
            let moved: HashSet<ClipId> = t
                .clips
                .iter()
                .filter(|c| c.start >= from)
                .map(|c| c.id)
                .collect();
            for c in &mut t.clips {
                if moved.contains(&c.id) {
                    c.start += delta;
                }
            }
            for tr in &mut t.transitions {
                let from_moves = tr.from.is_none_or(|f| moved.contains(&f));
                let to_moves = tr.to.is_none_or(|x| moved.contains(&x));
                if from_moves && to_moves && tr.cut >= from {
                    tr.cut += delta;
                }
            }
            t.sort();
        }
        if markers && self.opts.ripple_markers {
            for m in &mut self.seq.markers {
                if m.start >= from {
                    m.start += delta;
                }
            }
        }
        Ok(())
    }

    /// Opens `dur` of space at `at` on the given tracks, splitting clips that cross it.
    pub fn insert_space(
        &mut self,
        tracks: &[TrackRef],
        at: SeqTime,
        dur: Dur,
        markers: bool,
    ) -> EditResult {
        let crossing: Vec<ClipId> = tracks
            .iter()
            .filter_map(|r| self.seq.track(*r))
            .flat_map(|t| {
                t.clips
                    .iter()
                    .filter(|c| c.start < at && c.end() > at)
                    .map(|c| c.id)
            })
            .collect();
        self.split(&crossing, at);
        self.shift_after(tracks, at, dur, markers)
    }

    /// Length of empty space starting at `at` on a track (0 when a clip covers `at`).
    pub fn free_after(&self, r: TrackRef, at: SeqTime) -> Dur {
        let Some(t) = self.seq.track(r) else {
            return Dur(i64::MAX / 4);
        };
        if t.clips.iter().any(|c| c.start <= at && c.end() > at) {
            return Dur::ZERO;
        }
        t.clips
            .iter()
            .filter(|c| c.start >= at)
            .map(|c| c.start - at)
            .min()
            .unwrap_or(Dur(i64::MAX / 4))
    }

    /// Closes up to `len` of empty time at `at` on the participating tracks. Returns the amount
    /// actually closed: the smallest free space across them, so no protected material shifts out
    /// of sync (provisional rule for TL-RDEL-001 blockers).
    pub fn close_gap(&mut self, tracks: &[TrackRef], at: SeqTime, len: Dur) -> EditResult<Dur> {
        let mut amount = len;
        for &r in tracks {
            amount = amount.min(self.free_after(r, at));
        }
        if amount.0 <= 0 {
            return Ok(Dur::ZERO);
        }
        self.shift_after(tracks, at + amount, -amount, true)?;
        Ok(amount)
    }
}

/// Runs an edit on a sequence and commits the edited copy into the project.
pub(crate) fn run<R>(
    p: &mut Project,
    sid: SequenceId,
    opts: EditOptions,
    f: impl FnOnce(&mut Ed) -> EditResult<R>,
) -> EditResult<R> {
    let (seq, ids, r) = {
        let mut ed = Ed::new(p, sid, opts)?;
        let r = f(&mut ed)?;
        let (seq, ids) = ed.finish();
        (seq, ids, r)
    };
    p.sequences.insert(sid, Arc::new(seq));
    p.ids = ids;
    Ok(r)
}

/// Splits clips at `t` (razor). With linked selection, partners are cut too.
pub fn razor(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    t: SeqTime,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    run(p, sid, opts, |ed| {
        let ids = ed.with_links(clips);
        for id in &ids {
            let (r, _) = ed.clip(*id)?;
            ed.writable(r)?;
        }
        let pairs = ed.split(&ids, t);
        if pairs.is_empty() {
            return Err(EditError::Nothing);
        }
        Ok(pairs.into_iter().map(|(_, r)| r).collect())
    })
}

/// Cuts every clip under `t` on the given tracks (Add Edit, or Shift+Razor on all tracks).
pub fn add_edit(
    p: &mut Project,
    sid: SequenceId,
    tracks: &[TrackRef],
    t: SeqTime,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    run(p, sid, opts, |ed| {
        let ids: Vec<ClipId> = tracks
            .iter()
            .filter(|r| ed.seq.track(**r).is_some_and(|t| !t.locked))
            .filter_map(|r| ed.seq.track(*r).and_then(|tr| tr.clip_at(t)).map(|c| c.id))
            .collect();
        let pairs = ed.split(&ids, t);
        if pairs.is_empty() {
            return Err(EditError::Nothing);
        }
        Ok(pairs.into_iter().map(|(_, r)| r).collect())
    })
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;

    pub struct Fixture {
        pub p: Project,
        pub seq: SequenceId,
        pub asset: AssetId,
        pub item: ItemId,
    }

    /// A 25 fps sequence and a 60 s A/V asset.
    pub fn fixture() -> Fixture {
        let mut p = Project::new("test");
        let asset = MediaAsset {
            id: AssetId(0),
            path: "clip.mov".into(),
            proxy: None,
            kind: MediaKind::Video,
            video: Some(VideoStream {
                index: 0,
                codec: "h264".into(),
                width: 1920,
                height: 1080,
                pixel_aspect: (1, 1),
                rate: Rate::FPS_25,
                frames: 25 * 60,
                pixel_format: "yuv420p".into(),
                bit_depth: 8,
                alpha: AlphaMode::None,
                color: ColorInfo::default(),
                field_order: FieldOrder::Progressive,
                start: Dur::ZERO,
                rotation: 0,
                timecode: None,
            }),
            audio: vec![AudioStream {
                index: 1,
                codec: "aac".into(),
                sample_rate: 48000,
                layout: ChannelLayout::Stereo,
                samples: 48000 * 60,
                start: Dur::ZERO,
            }],
            duration: Dur::from_seconds(60.0),
            interpretation: Interpretation::default(),
            file_size: 0,
            modified_unix: 0,
        };
        let (asset, item) = p.add_asset(p.root, asset);
        let (seq, _) = p.add_sequence(p.root, "Sequence 01", SequenceSettings::default());
        Fixture {
            p,
            seq,
            asset,
            item,
        }
    }

    pub fn s(sec: f64) -> SeqTime {
        SeqTime::from_seconds(sec)
    }

    pub fn d(sec: f64) -> Dur {
        Dur::from_seconds(sec)
    }

    pub fn src(sec: f64) -> SrcTime {
        SrcTime::from_seconds(sec)
    }

    impl Fixture {
        pub fn source(&self, from: f64, to: f64) -> SourceClip {
            SourceClip::from_asset(&self.p, self.item, SrcRange::new(src(from), src(to))).unwrap()
        }

        /// Overwrites the asset range at `at` on V1/A1.
        pub fn put(&mut self, from: f64, to: f64, at: f64) -> Vec<ClipId> {
            let spec = self.source(from, to);
            let (next, ids) = self
                .p
                .transact(|p| {
                    overwrite(
                        p,
                        self.seq,
                        &spec,
                        s(at),
                        &Patch::default(),
                        EditOptions::default(),
                    )
                })
                .unwrap();
            self.p = next;
            ids
        }

        pub fn seq(&self) -> &Sequence {
            self.p.sequence(self.seq).unwrap()
        }

        /// (start, end) in seconds of every clip on a track.
        pub fn layout(&self, r: TrackRef) -> Vec<(f64, f64)> {
            self.seq()
                .track(r)
                .unwrap()
                .clips
                .iter()
                .map(|c| (round(c.start.seconds()), round(c.end().seconds())))
                .collect()
        }

        pub fn apply<R>(&mut self, f: impl FnOnce(&mut Project) -> EditResult<R>) -> EditResult<R> {
            let (next, r) = self.p.transact(f)?;
            self.p = next;
            Ok(r)
        }
    }

    pub fn round(v: f64) -> f64 {
        (v * 1000.0).round() / 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;

    #[test]
    fn razor_cuts_linked_partners_and_relinks_halves() {
        let mut f = fixture();
        let ids = f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        let rights = f
            .apply(|p| razor(p, seq, &[ids[0]], s(4.0), EditOptions::default()))
            .unwrap();
        assert_eq!(rights.len(), 2);
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 4.0), (4.0, 10.0)]);
        assert_eq!(f.layout(TrackRef::audio(0)), vec![(0.0, 4.0), (4.0, 10.0)]);
        let s = f.seq();
        let rv = s.clip(rights[0]).unwrap();
        let ra = s.clip(rights[1]).unwrap();
        assert!(rv.link.is_some() && rv.link == ra.link);
        assert_ne!(rv.link, s.video[0].clips[0].link);
        assert_eq!(rv.source_in, src(4.0));
    }

    #[test]
    fn razor_on_reverse_clip_keeps_the_picture() {
        let mut f = fixture();
        let ids = f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        f.p.sequence_mut(seq)
            .unwrap()
            .clip_mut(ids[0])
            .unwrap()
            .reverse = true;
        let before = f.seq().clip(ids[0]).unwrap().to_source(s(6.0));
        let r = f
            .apply(|p| {
                razor(
                    p,
                    seq,
                    &[ids[0]],
                    s(4.0),
                    EditOptions {
                        linked_selection: false,
                        ..Default::default()
                    },
                )
            })
            .unwrap();
        let after = f.seq().clip(r[0]).unwrap().to_source(s(6.0));
        assert_eq!(before, after);
    }

    #[test]
    fn add_edit_skips_locked_tracks() {
        let mut f = fixture();
        f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        f.p.sequence_mut(seq)
            .unwrap()
            .track_mut(TrackRef::audio(0))
            .unwrap()
            .locked = true;
        f.apply(|p| {
            add_edit(
                p,
                seq,
                &[TrackRef::video(0), TrackRef::audio(0)],
                s(5.0),
                EditOptions::default(),
            )
        })
        .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)).len(), 2);
        assert_eq!(f.layout(TrackRef::audio(0)).len(), 1);
    }
}
