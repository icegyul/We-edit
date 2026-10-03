//! Moving, duplicating, clipboard, nesting, links, groups, tracks and markers.

use std::collections::HashMap;
use std::sync::Arc;

use op_core::catalog::EffectKind;
use op_core::*;

use crate::edit::ensure_track;
use crate::{Ed, EditOptions, run};

/// A drag of clips with the Selection tool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveSpec {
    pub clips: Vec<ClipId>,
    pub delta: Dur,
    /// Tracks up (positive) or down; applied to video and audio alike.
    pub track_delta: i32,
    /// Insert at the destination instead of overwriting (Ctrl-drag).
    pub insert: bool,
    /// Leave the originals in place and move copies (Alt-drag).
    pub duplicate: bool,
}

/// Collects clips plus linked partners and group members.
fn expand(ed: &Ed, clips: &[ClipId]) -> Vec<ClipId> {
    let mut out = ed.with_links(clips);
    let mut i = 0;
    while i < out.len() {
        for g in ed.seq.grouped(out[i]) {
            for l in ed.with_links(&[g]) {
                if !out.contains(&l) {
                    out.push(l);
                }
            }
        }
        i += 1;
    }
    out
}

/// Gives clips new IDs; link and group IDs are remapped consistently.
fn fresh(ed: &mut Ed, clips: &mut [Clip]) -> HashMap<ClipId, ClipId> {
    let mut links: HashMap<LinkId, LinkId> = HashMap::new();
    let mut groups: HashMap<GroupId, GroupId> = HashMap::new();
    let mut map = HashMap::new();
    for c in clips {
        let new = ed.ids.clip();
        map.insert(c.id, new);
        c.id = new;
        c.link = c
            .link
            .map(|l| *links.entry(l).or_insert_with(|| ed.ids.link()));
        c.group = c
            .group
            .map(|g| *groups.entry(g).or_insert_with(|| ed.ids.group()));
        for comp in &mut c.components {
            comp.id = ed.ids.component();
        }
    }
    map
}

/// Moves (or duplicates) clips; destination material is overwritten, or pushed with `insert`.
pub fn move_clips(
    p: &mut Project,
    sid: SequenceId,
    spec: &MoveSpec,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    run(p, sid, opts, |ed| {
        let ids = expand(ed, &spec.clips);
        if ids.is_empty() {
            return Err(EditError::Nothing);
        }
        let mut moving: Vec<(TrackRef, Clip)> = Vec::new();
        for id in &ids {
            let (r, c) = ed.clip(*id)?;
            if !spec.duplicate {
                ed.writable(r)?;
            }
            moving.push((r, c));
        }
        // keep inside the sequence and the track stacks
        let earliest = moving.iter().map(|(_, c)| c.start).min().unwrap();
        let delta = spec.delta.max(-(earliest.since_zero()));
        let mut track_delta = spec.track_delta;
        for kind in [TrackKind::Video, TrackKind::Audio] {
            if let Some(lowest) = moving
                .iter()
                .filter(|(r, _)| r.kind == kind)
                .map(|(r, _)| r.index)
                .min()
            {
                track_delta = track_delta.max(-(lowest as i32));
            }
        }
        if delta.0 == 0 && track_delta == 0 && !spec.duplicate {
            return Err(EditError::Nothing);
        }
        // transitions that move with the clips
        let moved_ids: Vec<ClipId> = moving.iter().map(|(_, c)| c.id).collect();
        let mut carried: Vec<(TrackRef, Transition)> = Vec::new();
        for (r, t) in ed.seq.all_tracks() {
            for tr in &t.transitions {
                let ends = [tr.from, tr.to];
                if ends.iter().flatten().all(|c| moved_ids.contains(c)) {
                    carried.push((r, tr.clone()));
                }
            }
        }
        if !spec.duplicate {
            for id in &moved_ids {
                ed.remove(*id)?;
            }
            for (r, tr) in &carried {
                ed.track_mut(*r)?.transitions.retain(|t| t.id != tr.id);
            }
        }
        let mut clips: Vec<Clip> = moving.iter().map(|(_, c)| c.clone()).collect();
        let map = if spec.duplicate {
            fresh(ed, &mut clips)
        } else {
            moved_ids.iter().map(|i| (*i, *i)).collect()
        };
        let dest: Vec<TrackRef> = moving
            .iter()
            .map(|(r, _)| TrackRef {
                kind: r.kind,
                index: (r.index as i32 + track_delta) as usize,
            })
            .collect();
        for r in &dest {
            ensure_track(ed, *r);
            ed.writable(*r)?;
        }
        if spec.insert {
            let span_start = earliest + delta;
            let span_end = moving.iter().map(|(_, c)| c.end()).max().unwrap() + delta;
            let mut tracks: Vec<TrackRef> = dest.clone();
            for r in ed.sync_tracks() {
                if !tracks.contains(&r) {
                    tracks.push(r);
                }
            }
            tracks.sort_by_key(|r| (r.kind == TrackKind::Audio, r.index));
            tracks.dedup();
            ed.insert_space(&tracks, span_start, span_end - span_start, true)?;
        } else {
            for (r, c) in dest.iter().zip(&clips) {
                ed.clear_range(&[*r], c.range().shifted(delta))?;
            }
        }
        let mut out = Vec::new();
        for (r, mut c) in dest.iter().copied().zip(clips) {
            c.start += delta;
            out.push(c.id);
            ed.place(r, c)?;
        }
        for (r, mut tr) in carried {
            let r = TrackRef {
                kind: r.kind,
                index: (r.index as i32 + track_delta) as usize,
            };
            tr.cut += delta;
            tr.from = tr.from.and_then(|c| map.get(&c).copied());
            tr.to = tr.to.and_then(|c| map.get(&c).copied());
            if spec.duplicate {
                tr.id = ed.ids.transition();
            }
            ed.track_mut(r)?.transitions.push(tr);
        }
        Ok(out)
    })
}

/// Copied timeline material.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clipboard {
    /// Clips with their original tracks.
    pub clips: Vec<(TrackRef, Clip)>,
    pub transitions: Vec<(TrackRef, Transition)>,
}

impl Clipboard {
    pub fn copy(seq: &Sequence, clips: &[ClipId]) -> Clipboard {
        let mut cb = Clipboard::default();
        for id in clips {
            if let Some((r, c)) = seq.find_clip(*id) {
                cb.clips.push((r, c.clone()));
            }
        }
        for (r, t) in seq.all_tracks() {
            for tr in &t.transitions {
                if [tr.from, tr.to].iter().flatten().all(|c| clips.contains(c)) {
                    cb.transitions.push((r, tr.clone()));
                }
            }
        }
        cb
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    pub fn span(&self) -> Option<SeqRange> {
        let start = self.clips.iter().map(|(_, c)| c.start).min()?;
        let end = self.clips.iter().map(|(_, c)| c.end()).max()?;
        Some(SeqRange::new(start, end))
    }
}

/// Pastes at `at`. The lowest copied video/audio track maps onto `video_base`/`audio_base`
/// (the lowest targeted tracks); relative offsets are kept.
pub fn paste(
    p: &mut Project,
    sid: SequenceId,
    cb: &Clipboard,
    at: SeqTime,
    video_base: usize,
    audio_base: usize,
    insert: bool,
    opts: EditOptions,
) -> EditResult<Vec<ClipId>> {
    let span = cb.span().ok_or(EditError::Nothing)?;
    run(p, sid, opts, |ed| {
        let low_v = cb
            .clips
            .iter()
            .filter(|(r, _)| r.kind == TrackKind::Video)
            .map(|(r, _)| r.index)
            .min()
            .unwrap_or(0);
        let low_a = cb
            .clips
            .iter()
            .filter(|(r, _)| r.kind == TrackKind::Audio)
            .map(|(r, _)| r.index)
            .min()
            .unwrap_or(0);
        let map_track = |r: TrackRef| match r.kind {
            TrackKind::Video => TrackRef::video(r.index - low_v + video_base),
            TrackKind::Audio => TrackRef::audio(r.index - low_a + audio_base),
        };
        let mut clips: Vec<Clip> = cb.clips.iter().map(|(_, c)| c.clone()).collect();
        let map = fresh(ed, &mut clips);
        let dests: Vec<TrackRef> = cb.clips.iter().map(|(r, _)| map_track(*r)).collect();
        for r in &dests {
            ensure_track(ed, *r);
            ed.writable(*r)?;
        }
        let delta = at - span.start;
        if insert {
            let mut tracks = dests.clone();
            for r in ed.sync_tracks() {
                if !tracks.contains(&r) {
                    tracks.push(r);
                }
            }
            tracks.dedup();
            ed.insert_space(&tracks, at, span.duration(), true)?;
        } else {
            for (r, c) in dests.iter().zip(&clips) {
                ed.clear_range(&[*r], c.range().shifted(delta))?;
            }
        }
        let mut out = Vec::new();
        for (r, mut c) in dests.into_iter().zip(clips) {
            // clips from a sequence that no longer exists cannot be pasted
            if let ClipSource::Sequence { sequence, .. } = c.source
                && (ed.p.sequence(sequence).is_none() || ed.p.nests(sequence, sid))
            {
                continue;
            }
            c.start += delta;
            out.push(c.id);
            ed.place(r, c)?;
        }
        for (r, tr) in &cb.transitions {
            let mut tr = tr.clone();
            tr.id = ed.ids.transition();
            tr.cut += delta;
            tr.from = tr.from.and_then(|c| map.get(&c).copied());
            tr.to = tr.to.and_then(|c| map.get(&c).copied());
            let r = map_track(*r);
            ed.track_mut(r)?.transitions.push(tr);
        }
        if out.is_empty() {
            return Err(EditError::Nothing);
        }
        Ok(out)
    })
}

/// Which attributes Paste Attributes copies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attributes {
    pub motion: bool,
    pub opacity: bool,
    pub effects: bool,
    pub volume: bool,
    pub channel_volume: bool,
    pub panner: bool,
}

impl Default for Attributes {
    fn default() -> Self {
        Attributes {
            motion: true,
            opacity: true,
            effects: true,
            volume: true,
            channel_volume: true,
            panner: true,
        }
    }
}

/// Paste Attributes: copies fixed-effect settings and standard effects from `from` to clips of
/// the same kind.
pub fn paste_attributes(
    p: &mut Project,
    sid: SequenceId,
    from: &Clip,
    to: &[ClipId],
    what: Attributes,
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let mut any = false;
        for id in to {
            let (r, mut c) = ed.clip(*id)?;
            if c.kind != from.kind || c.id == from.id {
                continue;
            }
            ed.writable(r)?;
            for src in &from.components {
                let wanted = match src.effect.as_str() {
                    catalog::MOTION => what.motion,
                    catalog::OPACITY => what.opacity,
                    catalog::VOLUME => what.volume,
                    catalog::CHANNEL_VOLUME => what.channel_volume,
                    catalog::PANNER => what.panner,
                    _ => what.effects && is_standard(src),
                };
                if !wanted {
                    continue;
                }
                // keyframes keep their offset from the clip start
                let shift = c.source_in - from.source_in;
                let mut comp = src.clone();
                comp.id = ed.ids.component();
                for prm in &mut comp.params {
                    for k in &mut prm.keys {
                        k.time += shift;
                    }
                }
                if src.is_fixed() {
                    if let Some(existing) = c.component_mut(&src.effect) {
                        existing.params = comp.params;
                        existing.enabled = comp.enabled;
                    }
                } else {
                    c.components.push(comp);
                }
                any = true;
            }
            let t = ed.track_mut(r)?;
            let i = t.index_of(c.id).unwrap();
            t.clips[i] = c;
        }
        if any { Ok(()) } else { Err(EditError::Nothing) }
    })
}

use op_core::catalog;

/// A standard effect (not a fixed effect, nor the text or caption a graphics clip draws).
fn is_standard(c: &Component) -> bool {
    c.def()
        .is_none_or(|d| matches!(d.kind, EffectKind::VideoEffect | EffectKind::AudioEffect))
}

/// Remove Attributes: the chosen fixed effects go back to their defaults and, with `effects`,
/// standard effects are removed; keyframes go with them.
pub fn remove_attributes(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    what: Attributes,
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let mut any = false;
        for id in clips {
            let (r, mut c) = ed.clip(*id)?;
            ed.writable(r)?;
            let before = c.components.clone();
            if what.effects {
                c.components.retain(|x| !is_standard(x));
            }
            for comp in &mut c.components {
                let wanted = match comp.effect.as_str() {
                    catalog::MOTION => what.motion,
                    catalog::OPACITY => what.opacity,
                    catalog::VOLUME => what.volume,
                    catalog::CHANNEL_VOLUME => what.channel_volume,
                    catalog::PANNER => what.panner,
                    _ => false,
                };
                if let (true, Some(def)) = (wanted, comp.def()) {
                    let fresh = Component::new(def, &mut ed.ids);
                    comp.params = fresh.params;
                    comp.enabled = true;
                }
            }
            if c.components != before {
                any = true;
                let t = ed.track_mut(r)?;
                let i = t.index_of(c.id).unwrap();
                t.clips[i] = c;
            }
        }
        if any { Ok(()) } else { Err(EditError::Nothing) }
    })
}

/// Nest: moves the clips into a new sequence and replaces them with one nested clip (plus a
/// linked audio clip when audio was selected). Returns the new sequence and the new clips.
pub fn nest(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    name: &str,
    bin: ItemId,
    opts: EditOptions,
) -> EditResult<(SequenceId, Vec<ClipId>)> {
    let seq = p.sequence(sid).ok_or(EditError::Nothing)?.clone();
    let mut picked: Vec<(TrackRef, Clip)> = Vec::new();
    for id in clips {
        let (r, c) = seq.find_clip(*id).ok_or(EditError::Nothing)?;
        if seq.track(r).is_some_and(|t| t.locked) {
            return Err(EditError::Locked(seq.track_name(r)));
        }
        picked.push((r, c.clone()));
    }
    if picked.is_empty() {
        return Err(EditError::Nothing);
    }
    let start = picked.iter().map(|(_, c)| c.start).min().unwrap();
    let end = picked.iter().map(|(_, c)| c.end()).max().unwrap();
    let mut settings = seq.settings.clone();
    settings.video_tracks = seq.video.len();
    settings.audio_tracks = seq.audio.len();
    let (nested, item) = p.add_sequence(bin, name, settings);
    {
        let n = p.sequence_mut(nested).unwrap();
        for (r, c) in &picked {
            let mut c = c.clone();
            c.start = SeqTime::ZERO + (c.start - start);
            let t = n.track_mut(*r).unwrap();
            let pos = t.clips.partition_point(|x| x.start <= c.start);
            t.clips.insert(pos, c);
        }
        for (r, t) in seq.all_tracks() {
            for tr in &t.transitions {
                if [tr.from, tr.to].iter().flatten().all(|c| clips.contains(c)) {
                    let mut tr = tr.clone();
                    tr.cut = SeqTime::ZERO + (tr.cut - start);
                    n.track_mut(r).unwrap().transitions.push(tr);
                }
            }
        }
    }
    let out = run(
        p,
        sid,
        EditOptions {
            linked_selection: false,
            ..opts
        },
        |ed| {
            for (_, c) in &picked {
                ed.remove(c.id)?;
            }
            let low_v = picked
                .iter()
                .filter(|(r, _)| r.kind == TrackKind::Video)
                .map(|(r, _)| r.index)
                .min();
            let low_a = picked
                .iter()
                .filter(|(r, _)| r.kind == TrackKind::Audio)
                .map(|(r, _)| r.index)
                .min();
            let dur = end - start;
            let link = (low_v.is_some() && low_a.is_some()).then(|| ed.ids.link());
            let mut out = Vec::new();
            let source = ClipSource::Sequence {
                sequence: nested,
                item: Some(item),
            };
            if let Some(v) = low_v {
                let r = TrackRef::video(v);
                ed.clear_range(&[r], SeqRange::new(start, end))?;
                let mut c = ed.new_clip(
                    TrackKind::Video,
                    source.clone(),
                    name,
                    start,
                    dur,
                    SrcTime::ZERO,
                );
                c.link = link;
                out.push(ed.place(r, c)?);
            }
            if let Some(a) = low_a {
                let r = TrackRef::audio(a);
                ed.clear_range(&[r], SeqRange::new(start, end))?;
                let mut c = ed.new_clip(
                    TrackKind::Audio,
                    source.clone(),
                    name,
                    start,
                    dur,
                    SrcTime::ZERO,
                );
                c.link = link;
                c.channels = Some(ChannelLayout::Stereo);
                out.push(ed.place(r, c)?);
            }
            Ok(out)
        },
    )?;
    Ok((nested, out))
}

/// Links the clips into one A/V relation.
pub fn link(p: &mut Project, sid: SequenceId, clips: &[ClipId]) -> EditResult {
    if clips.len() < 2 {
        return Err(EditError::Nothing);
    }
    run(p, sid, EditOptions::default(), |ed| {
        let l = ed.ids.link();
        for id in clips {
            ed.seq.clip_mut(*id).ok_or(EditError::Nothing)?.link = Some(l);
        }
        Ok(())
    })
}

/// Unlinks the clips and their partners.
pub fn unlink(p: &mut Project, sid: SequenceId, clips: &[ClipId]) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        let all = ed.with_links(clips);
        for id in all {
            ed.seq.clip_mut(id).ok_or(EditError::Nothing)?.link = None;
        }
        Ok(())
    })
}

pub fn group(p: &mut Project, sid: SequenceId, clips: &[ClipId]) -> EditResult {
    if clips.len() < 2 {
        return Err(EditError::Nothing);
    }
    run(p, sid, EditOptions::default(), |ed| {
        let g = ed.ids.group();
        for id in clips {
            ed.seq.clip_mut(*id).ok_or(EditError::Nothing)?.group = Some(g);
        }
        Ok(())
    })
}

pub fn ungroup(p: &mut Project, sid: SequenceId, clips: &[ClipId]) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        let mut all = Vec::new();
        for id in clips {
            all.extend(ed.seq.grouped(*id));
        }
        for id in all {
            ed.seq.clip_mut(id).ok_or(EditError::Nothing)?.group = None;
        }
        Ok(())
    })
}

/// Enable/disable: disables all when any is enabled, otherwise enables all.
pub fn toggle_enabled(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    opts: EditOptions,
) -> EditResult<bool> {
    run(p, sid, opts, |ed| {
        let ids = ed.with_links(clips);
        let any_on = ids
            .iter()
            .any(|id| ed.seq.clip(*id).is_some_and(|c| c.enabled));
        for id in ids {
            let (r, _) = ed.clip(id)?;
            ed.writable(r)?;
            ed.seq.clip_mut(id).unwrap().enabled = !any_on;
        }
        Ok(!any_on)
    })
}

/// Adds `count` tracks of a kind at `index` (pushed above existing ones at that index).
pub fn add_tracks(
    p: &mut Project,
    sid: SequenceId,
    kind: TrackKind,
    count: usize,
    index: usize,
    layout: AudioTrackLayout,
) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        for i in 0..count {
            let mut t = Track::new(ed.ids.track(), kind);
            t.layout = layout;
            if kind == TrackKind::Audio {
                t.components = default_components(EffectKind::AudioFixed, &mut ed.ids);
            }
            let tracks = ed.seq.tracks_mut(kind);
            let at = (index + i).min(tracks.len());
            tracks.insert(at, Arc::new(t));
        }
        Ok(())
    })
}

/// Deletes a track and its clips (a sequence keeps at least one track of each kind).
pub fn delete_track(p: &mut Project, sid: SequenceId, r: TrackRef) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        ed.writable(r)?;
        let tracks = ed.seq.tracks_mut(r.kind);
        if tracks.len() <= 1 || r.index >= tracks.len() {
            return Err(EditError::Nothing);
        }
        tracks.remove(r.index);
        Ok(())
    })
}

/// Removes every empty track (keeping at least one of each kind).
pub fn delete_empty_tracks(p: &mut Project, sid: SequenceId) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        for kind in [TrackKind::Video, TrackKind::Audio] {
            let tracks = ed.seq.tracks_mut(kind);
            let mut i = tracks.len();
            while i > 0 {
                i -= 1;
                if tracks.len() > 1 && tracks[i].clips.is_empty() && !tracks[i].locked {
                    tracks.remove(i);
                }
            }
        }
        Ok(())
    })
}

/// Adds a sequence marker at `t` (or returns the one already there).
pub fn add_marker(p: &mut Project, sid: SequenceId, t: SeqTime) -> EditResult<MarkerId> {
    run(p, sid, EditOptions::default(), |ed| {
        if let Some(m) = ed.seq.markers.iter().find(|m| m.start == t) {
            return Ok(m.id);
        }
        let id = ed.ids.marker();
        let m = Marker::new(id, t);
        let pos = ed.seq.markers.partition_point(|x| x.start <= t);
        ed.seq.markers.insert(pos, m);
        Ok(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn remove_attributes_resets_fixed_effects_and_drops_effects() {
        let mut f = fixture();
        let ids = f.put(0.0, 5.0, 0.0);
        let seq = f.seq;
        {
            let ids_gen = &mut f.p.ids;
            let motion = catalog::find(catalog::MOTION).unwrap();
            let blur = catalog::find("op.video.gaussian_blur").unwrap();
            let mut m = Component::new(motion, ids_gen);
            m.params
                .iter_mut()
                .find(|x| x.key == "scale")
                .unwrap()
                .value = Value::Float(150.0);
            let b = Component::new(blur, ids_gen);
            let c = f.p.sequence_mut(seq).unwrap().clip_mut(ids[0]).unwrap();
            c.components.retain(|x| x.effect != catalog::MOTION);
            c.components.push(m);
            c.components.push(b);
        }
        let what = Attributes {
            opacity: false,
            ..Attributes::default()
        };
        f.apply(|p| remove_attributes(p, seq, &[ids[0]], what, EditOptions::default()))
            .unwrap();
        let c = f.seq().clip(ids[0]).unwrap().clone();
        let scale = c
            .component(catalog::MOTION)
            .unwrap()
            .param("scale")
            .unwrap();
        assert_eq!(scale.value, Value::Float(100.0));
        assert!(c.component("op.video.gaussian_blur").is_none());
        // nothing left to remove
        assert!(
            f.apply(|p| remove_attributes(p, seq, &[ids[0]], what, EditOptions::default()))
                .is_err()
        );
    }

    #[test]
    fn move_overwrites_and_carries_links() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        f.put(10.0, 20.0, 5.0);
        let seq = f.seq;
        let spec = MoveSpec {
            clips: vec![a[0]],
            delta: d(7.0),
            track_delta: 0,
            insert: false,
            duplicate: false,
        };
        f.apply(|p| move_clips(p, seq, &spec, EditOptions::default()))
            .unwrap();
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(5.0, 7.0), (7.0, 12.0), (12.0, 15.0)]
        );
        assert_eq!(
            f.layout(TrackRef::audio(0)),
            vec![(5.0, 7.0), (7.0, 12.0), (12.0, 15.0)]
        );
    }

    #[test]
    fn move_up_a_track_and_duplicate() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        let seq = f.seq;
        let spec = MoveSpec {
            clips: vec![a[0]],
            delta: d(0.0),
            track_delta: 1,
            insert: false,
            duplicate: true,
        };
        let new = f
            .apply(|p| move_clips(p, seq, &spec, EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(1)), vec![(0.0, 5.0)]);
        assert_eq!(f.layout(TrackRef::audio(1)), vec![(0.0, 5.0)]);
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 5.0)]);
        let s = f.seq();
        assert!(!new.contains(&a[0]));
        assert_eq!(s.clip(new[0]).unwrap().link, s.clip(new[1]).unwrap().link);
        assert_ne!(s.clip(new[0]).unwrap().link, s.clip(a[0]).unwrap().link);
        // cannot move below V1
        let spec = MoveSpec {
            clips: vec![a[0]],
            delta: d(1.0),
            track_delta: -3,
            insert: false,
            duplicate: false,
        };
        f.apply(|p| move_clips(p, seq, &spec, EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(1.0, 6.0)]);
    }

    #[test]
    fn insert_move_pushes_material() {
        let mut f = fixture();
        f.put(0.0, 5.0, 0.0);
        let b = f.put(10.0, 12.0, 5.0);
        let seq = f.seq;
        let spec = MoveSpec {
            clips: vec![b[0]],
            delta: d(-5.0),
            track_delta: 0,
            insert: true,
            duplicate: false,
        };
        f.apply(|p| move_clips(p, seq, &spec, EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 2.0), (2.0, 7.0)]);
    }

    #[test]
    fn copy_paste_and_attributes() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        let seq = f.seq;
        f.p.sequence_mut(seq)
            .unwrap()
            .clip_mut(a[0])
            .unwrap()
            .component_mut(catalog::OPACITY)
            .unwrap()
            .params[0]
            .value = Value::Float(40.0);
        let cb = Clipboard::copy(f.seq(), &a);
        let pasted = f
            .apply(|p| paste(p, seq, &cb, s(10.0), 1, 1, false, EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(1)), vec![(10.0, 15.0)]);
        assert_eq!(f.layout(TrackRef::audio(1)), vec![(10.0, 15.0)]);
        let target = f.put(20.0, 30.0, 20.0);
        let from = f.seq().clip(pasted[0]).unwrap().clone();
        f.apply(|p| {
            paste_attributes(
                p,
                seq,
                &from,
                &target,
                Attributes::default(),
                EditOptions::default(),
            )
        })
        .unwrap();
        let op = f
            .seq()
            .clip(target[0])
            .unwrap()
            .component(catalog::OPACITY)
            .unwrap()
            .params[0]
            .value
            .clone();
        assert_eq!(op, Value::Float(40.0));
    }

    #[test]
    fn nesting_replaces_clips_with_one_sequence_clip() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 2.0);
        let b = f.put(10.0, 12.0, 7.0);
        let seq = f.seq;
        let root = f.p.root;
        let all: Vec<ClipId> = a.iter().chain(b.iter()).copied().collect();
        let (nested, clips) = f
            .apply(|p| nest(p, seq, &all, "Nested", root, EditOptions::default()))
            .unwrap();
        assert_eq!(clips.len(), 2);
        assert_eq!(f.layout(TrackRef::video(0)), vec![(2.0, 9.0)]);
        let n = f.p.sequence(nested).unwrap();
        assert_eq!(n.video[0].clips.len(), 2);
        assert_eq!(n.video[0].clips[0].start, SeqTime::ZERO);
        assert!(f.p.nests(seq, nested));
    }

    #[test]
    fn links_groups_enable_tracks() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        let seq = f.seq;
        f.apply(|p| unlink(p, seq, &[a[0]])).unwrap();
        assert!(f.seq().clip(a[1]).unwrap().link.is_none());
        f.apply(|p| link(p, seq, &a)).unwrap();
        assert!(f.seq().clip(a[1]).unwrap().link.is_some());
        let on = f
            .apply(|p| toggle_enabled(p, seq, &[a[0]], EditOptions::default()))
            .unwrap();
        assert!(!on && !f.seq().clip(a[1]).unwrap().enabled);
        f.apply(|p| add_tracks(p, seq, TrackKind::Video, 2, 0, AudioTrackLayout::Standard))
            .unwrap();
        assert_eq!(f.seq().video.len(), 5);
        assert_eq!(f.layout(TrackRef::video(2)), vec![(0.0, 5.0)]);
        f.apply(|p| delete_empty_tracks(p, seq)).unwrap();
        assert_eq!(f.seq().video.len(), 1);
        let m = f.apply(|p| add_marker(p, seq, s(3.0))).unwrap();
        assert_eq!(f.seq().markers[0].id, m);
    }
}
