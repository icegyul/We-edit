//! Insert, overwrite, lift, extract, delete and ripple delete (TL-OW, TL-IN, TL-CUT tables).

use op_core::*;

use crate::{Ed, EditOptions, run};

/// What a source (media, subclip, sequence or generator) contributes to a timeline edit.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceClip {
    pub item: Option<ItemId>,
    pub name: String,
    pub video: Option<ClipSource>,
    /// One entry per audio stream placed.
    pub audio: Vec<(ClipSource, ChannelLayout)>,
    /// Source In/Out.
    pub range: SrcRange,
    pub label: Label,
    pub scale_to_frame: bool,
}

impl SourceClip {
    /// A media item (or subclip) with an explicit source range.
    pub fn from_asset(p: &Project, item: ItemId, range: SrcRange) -> EditResult<SourceClip> {
        let it = p
            .item(item)
            .ok_or_else(|| EditError::NotFound("item".into()))?;
        let ItemKind::Media { asset, .. } = it.kind else {
            return Err(EditError::Invalid("not media".into()));
        };
        let a = p
            .asset(asset)
            .ok_or_else(|| EditError::NotFound("media".into()))?;
        let video = a.video.as_ref().map(|_| ClipSource::Asset {
            asset,
            item: Some(item),
            stream: 0,
        });
        let audio = a
            .audio
            .iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    ClipSource::Asset {
                        asset,
                        item: Some(item),
                        stream: i,
                    },
                    s.layout,
                )
            })
            .collect();
        Ok(SourceClip {
            item: Some(item),
            name: it.name.clone(),
            video,
            audio,
            range,
            label: it.label,
            scale_to_frame: p.settings.media_scaling == MediaScaling::ScaleToFrameSize,
        })
    }

    /// Any project item, using its In/Out marks (or its whole length) as the range.
    pub fn from_item(p: &Project, item: ItemId) -> EditResult<SourceClip> {
        let it = p
            .item(item)
            .ok_or_else(|| EditError::NotFound("item".into()))?;
        let whole = |len: Dur| SrcRange::with_duration(SrcTime::ZERO, len);
        let marks = |r: SrcRange| {
            let start = it.mark_in.unwrap_or(r.start).clamp(r.start, r.end);
            let end = it.mark_out.unwrap_or(r.end).clamp(start, r.end);
            SrcRange::new(start, end)
        };
        match &it.kind {
            ItemKind::Media { asset, subclip } => {
                let a = p
                    .asset(*asset)
                    .ok_or_else(|| EditError::NotFound("media".into()))?;
                let full = match (subclip, a.available()) {
                    (Some(s), _) => *s,
                    (None, Some(r)) => r,
                    (None, None) => whole(p.settings.still_duration),
                };
                let mut range = marks(full);
                if a.is_still() && it.mark_out.is_none() {
                    range.end = range.start + p.settings.still_duration;
                }
                SourceClip::from_asset(p, item, range)
            }
            ItemKind::Sequence { sequence } => {
                let s = p
                    .sequence(*sequence)
                    .ok_or_else(|| EditError::NotFound("sequence".into()))?;
                let len = s.duration().max(s.settings.frame_duration());
                let src = |_| ClipSource::Sequence {
                    sequence: *sequence,
                    item: Some(item),
                };
                let layout = match s.settings.master {
                    MasterLayout::Mono => ChannelLayout::Mono,
                    MasterLayout::Stereo => ChannelLayout::Stereo,
                    MasterLayout::Surround51 => ChannelLayout::Surround51,
                };
                Ok(SourceClip {
                    item: Some(item),
                    name: it.name.clone(),
                    video: Some(src(())),
                    audio: vec![(src(()), layout)],
                    range: marks(whole(len)),
                    label: it.label,
                    scale_to_frame: false,
                })
            }
            ItemKind::Synthetic {
                generator,
                duration,
            } => {
                let src = ClipSource::Generator { item };
                let audio = if generator.has_audio() {
                    vec![(src.clone(), ChannelLayout::Stereo)]
                } else {
                    vec![]
                };
                Ok(SourceClip {
                    item: Some(item),
                    name: it.name.clone(),
                    video: Some(src),
                    audio,
                    range: marks(whole(*duration)),
                    label: it.label,
                    scale_to_frame: false,
                })
            }
            ItemKind::Bin { .. } => Err(EditError::Invalid(
                "bins cannot be edited into a sequence".into(),
            )),
        }
    }

    pub fn duration(&self) -> Dur {
        self.range.duration()
    }
}

/// Source patching: which destination track receives the source video and each audio stream.
/// `None` means the stream is not placed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub video: Option<usize>,
    pub audio: Vec<Option<usize>>,
}

impl Default for Patch {
    fn default() -> Self {
        Patch {
            video: Some(0),
            audio: Vec::new(),
        }
    }
}

impl Patch {
    /// Destination for audio stream `i` (defaults to A1, A2, ... in order).
    pub fn audio_track(&self, i: usize) -> Option<usize> {
        self.audio.get(i).copied().unwrap_or(Some(i))
    }
}

/// Destination tracks of a source edit, creating missing tracks.
fn destinations(
    ed: &mut Ed,
    spec: &SourceClip,
    patch: &Patch,
) -> EditResult<Vec<(TrackRef, ClipSource, Option<ChannelLayout>)>> {
    let mut out = Vec::new();
    if let (Some(v), Some(src)) = (patch.video, &spec.video) {
        out.push((TrackRef::video(v), src.clone(), None));
    }
    for (i, (src, layout)) in spec.audio.iter().enumerate() {
        if let Some(a) = patch.audio_track(i) {
            out.push((TrackRef::audio(a), src.clone(), Some(*layout)));
        }
    }
    if out.is_empty() {
        return Err(EditError::Invalid("no source track is patched".into()));
    }
    for (r, _, _) in &out {
        ensure_track(ed, *r);
        ed.writable(*r)?;
    }
    Ok(out)
}

/// Adds tracks until `r` exists.
pub(crate) fn ensure_track(ed: &mut Ed, r: TrackRef) {
    while ed.seq.tracks(r.kind).len() <= r.index {
        let mut t = Track::new(ed.ids.track(), r.kind);
        if r.kind == TrackKind::Audio {
            t.components =
                default_components(op_core::catalog::EffectKind::AudioFixed, &mut ed.ids);
        }
        ed.seq.tracks_mut(r.kind).push(std::sync::Arc::new(t));
    }
}

fn place_source(
    ed: &mut Ed,
    spec: &SourceClip,
    dests: &[(TrackRef, ClipSource, Option<ChannelLayout>)],
    at: SeqTime,
) -> EditResult<Vec<ClipId>> {
    let link = (dests.len() > 1).then(|| ed.ids.link());
    let mut ids = Vec::new();
    for (r, src, layout) in dests {
        let mut c = ed.new_clip(
            r.kind,
            src.clone(),
            &spec.name,
            at,
            spec.duration(),
            spec.range.start,
        );
        c.link = link;
        c.label = spec.label;
        c.channels = *layout;
        c.scale_to_frame = spec.scale_to_frame && r.kind == TrackKind::Video;
        if r.kind == TrackKind::Video && ed.p.settings.media_scaling == MediaScaling::SetToFrameSize
        {
            let (w, h) = ed.p.source_size(src, &ed.seq.settings);
            let fit = (ed.seq.settings.width as f64 / w.max(1) as f64)
                .min(ed.seq.settings.height as f64 / h.max(1) as f64);
            if let Some(m) = c
                .component_mut(op_core::catalog::MOTION)
                .and_then(|m| m.param_mut("scale"))
            {
                m.value = Value::Float(fit * 100.0);
            }
        }
        ids.push(ed.place(*r, c)?);
    }
    Ok(ids)
}

/// Overwrite edit: covered material is replaced (TL-OW-001, TL-OW-002).
pub fn overwrite(
    p: &mut Project,
    sid: SequenceId,
    spec: &SourceClip,
    at: SeqTime,
    patch: &Patch,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    if spec.duration().0 <= 0 {
        return Err(EditError::Nothing);
    }
    guard_nesting(p, sid, spec)?;
    run(p, sid, opts, |ed| {
        let dests = destinations(ed, spec, patch)?;
        let tracks: Vec<TrackRef> = dests.iter().map(|d| d.0).collect();
        ed.clear_range(&tracks, SeqRange::with_duration(at, spec.duration()))?;
        place_source(ed, spec, &dests, at)
    })
}

/// Insert edit: destination tracks, plus unlocked sync-locked tracks, open space (TL-IN-002).
pub fn insert(
    p: &mut Project,
    sid: SequenceId,
    spec: &SourceClip,
    at: SeqTime,
    patch: &Patch,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    if spec.duration().0 <= 0 {
        return Err(EditError::Nothing);
    }
    guard_nesting(p, sid, spec)?;
    run(p, sid, opts, |ed| {
        let dests = destinations(ed, spec, patch)?;
        let mut tracks: Vec<TrackRef> = dests.iter().map(|d| d.0).collect();
        for r in ed.sync_tracks() {
            if !tracks.contains(&r) {
                tracks.push(r);
            }
        }
        ed.insert_space(&tracks, at, spec.duration(), true)?;
        place_source(ed, spec, &dests, at)
    })
}

fn guard_nesting(p: &Project, sid: SequenceId, spec: &SourceClip) -> EditResult {
    if let Some(ClipSource::Sequence { sequence, .. }) = &spec.video
        && p.nests(*sequence, sid)
    {
        return Err(EditError::Recursive);
    }
    Ok(())
}

/// Lift: removes the In/Out range from the targeted tracks and leaves a gap (TL-LIFT-001).
pub fn lift(
    p: &mut Project,
    sid: SequenceId,
    range: SeqRange,
    targets: &[TrackRef],
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let tracks: Vec<TrackRef> = targets
            .iter()
            .copied()
            .filter(|r| ed.seq.track(*r).is_some_and(|t| !t.locked))
            .collect();
        if tracks.is_empty() || range.is_empty() {
            return Err(EditError::Nothing);
        }
        ed.clear_range(&tracks, range)
    })
}

/// Extract: removes the range from the targeted tracks and closes the gap on them and on
/// sync-locked tracks. When a sync-locked, untargeted track has material in the range, the gap
/// closes only as far as every participating track allows (provisional, TL-EXT-001).
pub fn extract(
    p: &mut Project,
    sid: SequenceId,
    range: SeqRange,
    targets: &[TrackRef],
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let tracks: Vec<TrackRef> = targets
            .iter()
            .copied()
            .filter(|r| ed.seq.track(*r).is_some_and(|t| !t.locked))
            .collect();
        if tracks.is_empty() || range.is_empty() {
            return Err(EditError::Nothing);
        }
        ed.clear_range(&tracks, range)?;
        let mut all = tracks.clone();
        for r in ed.sync_tracks() {
            if !all.contains(&r) {
                all.push(r);
            }
        }
        ed.close_gap(&all, range.start, range.duration())?;
        Ok(())
    })
}

/// Clear: removes clips (with linked partners when linked selection is on) and transitions,
/// leaving gaps (TL-CLEAR-001).
pub fn delete(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    transitions: &[TransitionId],
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let ids = ed.with_links(clips);
        if ids.is_empty() && transitions.is_empty() {
            return Err(EditError::Nothing);
        }
        for id in &ids {
            let (r, _) = ed.clip(*id)?;
            ed.writable(r)?;
        }
        for id in ids {
            ed.remove(id)?;
        }
        remove_transitions(ed, transitions)
    })
}

pub(crate) fn remove_transitions(ed: &mut Ed, transitions: &[TransitionId]) -> EditResult {
    if transitions.is_empty() {
        return Ok(());
    }
    for r in ed.all_refs() {
        if ed
            .track(r)?
            .transitions
            .iter()
            .any(|t| transitions.contains(&t.id))
        {
            ed.writable(r)?;
            ed.track_mut(r)?
                .transitions
                .retain(|t| !transitions.contains(&t.id));
        }
    }
    Ok(())
}

/// Ripple delete: removes clips and closes the time they occupied on their tracks and on
/// sync-locked tracks, per contiguous span, as far as every participating track allows.
/// Returns false when some span could not close completely.
pub fn ripple_delete(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    opts: EditOptions,
) -> EditResult<bool> {
    run(p, sid, opts, |ed| {
        let ids = ed.with_links(clips);
        if ids.is_empty() {
            return Err(EditError::Nothing);
        }
        let mut spans: Vec<SeqRange> = Vec::new();
        let mut direct: Vec<TrackRef> = Vec::new();
        for id in &ids {
            let (r, c) = ed.clip(*id)?;
            ed.writable(r)?;
            spans.push(c.range());
            if !direct.contains(&r) {
                direct.push(r);
            }
        }
        for id in &ids {
            ed.remove(*id)?;
        }
        let merged = merge_ranges(spans);
        let mut tracks = direct.clone();
        for r in ed.sync_tracks() {
            if !tracks.contains(&r) {
                tracks.push(r);
            }
        }
        let mut complete = true;
        // last span first so earlier positions stay valid
        for span in merged.iter().rev() {
            let closed = ed.close_gap(&tracks, span.start, span.duration())?;
            complete &= closed == span.duration();
        }
        Ok(complete)
    })
}

/// Removes the empty space at `t` on one track and on sync-locked tracks (Ripple Delete on a gap).
pub fn delete_gap(
    p: &mut Project,
    sid: SequenceId,
    track: TrackRef,
    t: SeqTime,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let gap = gap_at(ed.track(track)?, t).ok_or(EditError::Nothing)?;
        ed.writable(track)?;
        let mut tracks = vec![track];
        for r in ed.sync_tracks() {
            if !tracks.contains(&r) {
                tracks.push(r);
            }
        }
        let closed = ed.close_gap(&tracks, gap.start, gap.duration())?;
        if closed.0 == 0 {
            return Err(EditError::Invalid(
                "the gap is blocked on a sync-locked track".into(),
            ));
        }
        Ok(closed)
    })
}

/// The empty range between clips that contains `t` (not after the last clip).
pub fn gap_at(track: &Track, t: SeqTime) -> Option<SeqRange> {
    if track.clip_at(t).is_some() {
        return None;
    }
    let before = track
        .clips
        .iter()
        .filter(|c| c.end() <= t)
        .map(|c| c.end())
        .max()
        .unwrap_or(SeqTime::ZERO);
    let after = track
        .clips
        .iter()
        .filter(|c| c.start > t)
        .map(|c| c.start)
        .min()?;
    Some(SeqRange::new(before, after))
}

/// Q / W: ripple-trims from the previous edit to the playhead (`previous`) or from the playhead to
/// the next edit, on the targeted tracks.
pub fn ripple_trim_to_playhead(
    p: &mut Project,
    sid: SequenceId,
    targets: &[TrackRef],
    playhead: SeqTime,
    previous: bool,
    opts: EditOptions,
) -> EditResult {
    let seq = p.sequence(sid).ok_or(EditError::Nothing)?;
    let points = seq.edit_points(targets.iter().copied());
    let range = if previous {
        let e = points
            .iter()
            .copied()
            .filter(|e| *e < playhead)
            .max()
            .ok_or(EditError::Nothing)?;
        SeqRange::new(e, playhead)
    } else {
        let e = points
            .iter()
            .copied()
            .filter(|e| *e > playhead)
            .min()
            .ok_or(EditError::Nothing)?;
        SeqRange::new(playhead, e)
    };
    extract(p, sid, range, targets, opts)
}

/// Merges overlapping or touching ranges.
pub fn merge_ranges(mut v: Vec<SeqRange>) -> Vec<SeqRange> {
    v.sort_by_key(|r| r.start);
    let mut out: Vec<SeqRange> = Vec::new();
    for r in v {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditOptions;
    use crate::testutil::*;

    #[test]
    fn overwrite_splits_a_spanning_clip() {
        let mut f = fixture();
        f.put(0.0, 10.0, 0.0);
        f.put(20.0, 22.0, 4.0);
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 4.0), (4.0, 6.0), (6.0, 10.0)]
        );
        assert_eq!(
            f.layout(TrackRef::audio(0)),
            vec![(0.0, 4.0), (4.0, 6.0), (6.0, 10.0)]
        );
        let s = f.seq();
        // retained right segment continues the original media
        assert_eq!(s.video[0].clips[2].source_in, src(6.0));
        // new pair is linked, and each retained half keeps its partner
        assert_eq!(s.video[0].clips[1].link, s.audio[0].clips[1].link);
        assert_eq!(s.video[0].clips[2].link, s.audio[0].clips[2].link);
    }

    #[test]
    fn insert_shifts_sync_locked_tracks_only() {
        let mut f = fixture();
        f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        // V2 has a title; its sync lock is off, so it stays put
        let spec = f.source(0.0, 5.0);
        f.apply(|p| {
            overwrite(
                p,
                seq,
                &spec,
                s(8.0),
                &Patch {
                    video: Some(1),
                    audio: vec![None],
                },
                EditOptions::default(),
            )
        })
        .unwrap();
        f.p.sequence_mut(seq)
            .unwrap()
            .track_mut(TrackRef::video(1))
            .unwrap()
            .sync_lock = false;
        let spec = f.source(30.0, 32.0);
        f.apply(|p| {
            insert(
                p,
                seq,
                &spec,
                s(5.0),
                &Patch {
                    video: Some(0),
                    audio: vec![None],
                },
                EditOptions::default(),
            )
        })
        .unwrap();
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 5.0), (5.0, 7.0), (7.0, 12.0)]
        );
        // A1 is sync locked: split and shifted
        assert_eq!(f.layout(TrackRef::audio(0)), vec![(0.0, 5.0), (7.0, 12.0)]);
        assert_eq!(f.layout(TrackRef::video(1)), vec![(8.0, 13.0)]);
    }

    #[test]
    fn lift_and_extract() {
        let mut f = fixture();
        f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        let targets = [TrackRef::video(0), TrackRef::audio(0)];
        let mut g = fixture();
        g.put(0.0, 10.0, 0.0);
        f.apply(|p| {
            lift(
                p,
                seq,
                SeqRange::new(s(2.0), s(3.0)),
                &targets,
                EditOptions::default(),
            )
        })
        .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 2.0), (3.0, 10.0)]);
        let gseq = g.seq;
        g.apply(|p| {
            extract(
                p,
                gseq,
                SeqRange::new(s(2.0), s(3.0)),
                &targets,
                EditOptions::default(),
            )
        })
        .unwrap();
        assert_eq!(g.layout(TrackRef::video(0)), vec![(0.0, 2.0), (2.0, 9.0)]);
        assert_eq!(g.layout(TrackRef::audio(0)), vec![(0.0, 2.0), (2.0, 9.0)]);
    }

    #[test]
    fn ripple_delete_closes_gaps_and_respects_blockers() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        f.put(10.0, 15.0, 5.0);
        let seq = f.seq;
        let full = f
            .apply(|p| ripple_delete(p, seq, &[a[0]], EditOptions::default()))
            .unwrap();
        assert!(full);
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 5.0)]);
        assert_eq!(f.layout(TrackRef::audio(0)), vec![(0.0, 5.0)]);

        // a blocker on sync-locked V2 limits the ripple
        let mut g = fixture();
        let a = g.put(0.0, 5.0, 0.0);
        g.put(10.0, 15.0, 5.0);
        let gs = g.seq;
        let spec = g.source(40.0, 42.0);
        g.apply(|p| {
            overwrite(
                p,
                gs,
                &spec,
                s(3.0),
                &Patch {
                    video: Some(1),
                    audio: vec![None],
                },
                EditOptions::default(),
            )
        })
        .unwrap();
        let full = g
            .apply(|p| ripple_delete(p, gs, &[a[0]], EditOptions::default()))
            .unwrap();
        assert!(!full);
        // V2's clip starts 3 s in, so only 3 s close and both tracks stay in sync
        assert_eq!(g.layout(TrackRef::video(0)), vec![(2.0, 7.0)]);
        assert_eq!(g.layout(TrackRef::video(1)), vec![(0.0, 2.0)]);
    }

    #[test]
    fn delete_gap_and_q_w() {
        let mut f = fixture();
        f.put(0.0, 5.0, 0.0);
        f.put(10.0, 15.0, 8.0);
        let seq = f.seq;
        let closed = f
            .apply(|p| delete_gap(p, seq, TrackRef::video(0), s(6.0), EditOptions::default()))
            .unwrap();
        assert_eq!(closed, d(3.0));
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 5.0), (5.0, 10.0)]);

        let targets = [TrackRef::video(0), TrackRef::audio(0)];
        f.apply(|p| {
            ripple_trim_to_playhead(p, seq, &targets, s(7.0), true, EditOptions::default())
        })
        .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 5.0), (5.0, 8.0)]);
        assert_eq!(f.seq().video[0].clips[1].source_in, src(12.0));
    }

    #[test]
    fn stills_default_to_the_still_duration() {
        let mut f = fixture();
        let mut still = f.p.asset(f.asset).unwrap().clone();
        still.kind = MediaKind::Still;
        still.audio.clear();
        let (_, item) = f.p.add_asset(f.p.root, still);
        let spec = SourceClip::from_item(&f.p, item).unwrap();
        assert_eq!(spec.duration(), d(5.0));
    }

    #[test]
    fn a_sequence_cannot_be_edited_into_itself() {
        let mut f = fixture();
        let item = f.p.sequence_item(f.seq).unwrap();
        f.put(0.0, 1.0, 0.0);
        let spec = SourceClip::from_item(&f.p, item).unwrap();
        let seq = f.seq;
        let r = f.apply(|p| {
            overwrite(
                p,
                seq,
                &spec,
                s(0.0),
                &Patch::default(),
                EditOptions::default(),
            )
        });
        assert_eq!(r, Err(EditError::Recursive));
    }
}
