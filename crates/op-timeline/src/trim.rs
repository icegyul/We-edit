//! Trimming tools: edge trim, ripple, roll, slip, slide, rate stretch, speed and frame hold
//! (docs/timeline-behavior.md 7). Every operation clamps the requested amount to what the
//! media handles, minimum duration (one frame) and neighbors allow, and returns the applied amount.

use op_core::*;

use crate::{Ed, EditOptions, run};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Edge {
    Head,
    Tail,
}

const INF: Dur = Dur(i64::MAX / 8);

/// Allowed movement of one clip edge: (min, max) delta in sequence time.
fn edge_bounds(ed: &Ed, c: &Clip, edge: Edge, neighbors: bool) -> (Dur, Dur) {
    let frame = ed.frame().min(c.duration);
    let avail = if c.hold.is_some() {
        None
    } else {
        ed.available(c)
    };
    let (r, _) = ed.clip(c.id).unwrap();
    let track = ed.seq.track(r).unwrap();
    let src = c.source_range();
    match edge {
        Edge::Tail => {
            let min = frame - c.duration;
            let mut max = INF;
            if neighbors
                && let Some(next) = track
                    .clips
                    .iter()
                    .filter(|n| n.id != c.id && n.start >= c.end())
                    .map(|n| n.start)
                    .min()
            {
                max = max.min(next - c.end());
            }
            if let Some(a) = avail {
                let room = if c.reverse {
                    src.start - a.start
                } else {
                    a.end - src.end
                };
                max = max.min(c.speed.to_sequence(room.max(Dur::ZERO)));
            }
            (min, max.max(min))
        }
        Edge::Head => {
            let max = c.duration - frame;
            // without neighbor limits (ripple, roll, slide) the clip start itself does not move
            // back, so only media handles bound the head
            let mut min = -INF;
            if neighbors {
                min = -(c.start.since_zero());
                let prev = track
                    .clips
                    .iter()
                    .filter(|n| n.id != c.id && n.end() <= c.start)
                    .map(|n| n.end())
                    .max();
                if let Some(p) = prev {
                    min = min.max(-(c.start - p));
                }
            }
            if let Some(a) = avail {
                let room = if c.reverse {
                    a.end - src.end
                } else {
                    src.start - a.start
                };
                min = min.max(-c.speed.to_sequence(room.max(Dur::ZERO)));
            }
            (min.min(max), max)
        }
    }
}

/// Moves an edge without touching neighbors (the caller guarantees room).
fn apply_edge(c: &mut Clip, edge: Edge, d: Dur) {
    match edge {
        Edge::Tail => {
            c.duration += d;
            if c.reverse && c.hold.is_none() {
                c.source_in -= c.speed.to_source(d);
            }
        }
        Edge::Head => {
            c.start += d;
            c.duration -= d;
            if !c.reverse && c.hold.is_none() {
                c.source_in += c.speed.to_source(d);
            }
        }
    }
}

fn clamp(d: Dur, (lo, hi): (Dur, Dur)) -> Dur {
    d.clamp(lo, hi)
}

fn members(ed: &Ed, clip: ClipId) -> EditResult<Vec<Clip>> {
    let ids = ed.with_links(&[clip]);
    let mut out = Vec::new();
    for id in ids {
        let (r, c) = ed.clip(id)?;
        ed.writable(r)?;
        out.push(c);
    }
    Ok(out)
}

fn update(ed: &mut Ed, c: Clip) -> EditResult {
    let (r, _) = ed.clip(c.id)?;
    let t = ed.track_mut(r)?;
    let i = t.index_of(c.id).unwrap();
    t.clips[i] = c;
    t.sort();
    Ok(())
}

/// Selection-tool trim of a clip edge (and its linked partners' edges).
pub fn trim(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    edge: Edge,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let clips = members(ed, clip)?;
        let mut d = delta;
        for c in &clips {
            d = clamp(d, edge_bounds(ed, c, edge, true));
        }
        if d.0 == 0 {
            return Ok(d);
        }
        for mut c in clips {
            apply_edge(&mut c, edge, d);
            update(ed, c)?;
        }
        Ok(d)
    })
}

/// Ripple trim: the edge moves and everything after it follows on the clip's tracks and on
/// sync-locked tracks. Removing time is limited to free space on the other participating tracks.
pub fn ripple_trim(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    edge: Edge,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let clips = members(ed, clip)?;
        let own: Vec<TrackRef> = clips.iter().map(|c| ed.clip(c.id).unwrap().0).collect();
        let mut tracks = own.clone();
        for r in ed.sync_tracks() {
            if !tracks.contains(&r) {
                tracks.push(r);
            }
        }
        let mut d = delta;
        for c in &clips {
            d = clamp(d, edge_bounds(ed, c, edge, false));
        }
        let first = &clips[0];
        // amount of time removed (positive) or added (negative) at the edit
        let removed = match edge {
            Edge::Tail => -d,
            Edge::Head => d,
        };
        let at = match edge {
            Edge::Tail => first.end() - removed.max(Dur::ZERO),
            Edge::Head => first.start,
        };
        // the first clip's track and the sync-locked tracks follow its edit point; linked
        // partners that are out of sync follow their own edge, so time is added or removed
        // next to each clip instead of cutting through it
        let first_track = own[0];
        let mut main: Vec<TrackRef> = tracks
            .iter()
            .filter(|r| !own.contains(r))
            .copied()
            .collect();
        main.push(first_track);
        let partners: Vec<(Clip, TrackRef)> = clips
            .iter()
            .cloned()
            .zip(own.iter().copied())
            .skip(1)
            .filter(|(_, r)| *r != first_track)
            .collect();
        if removed.is_positive() {
            // other participating tracks must be free where time disappears
            let mut allowed = removed;
            for r in &tracks {
                if !own.contains(r) {
                    allowed = allowed.min(ed.free_after(*r, at));
                }
            }
            if allowed.0 <= 0 {
                return Err(EditError::Invalid(
                    "a sync-locked track blocks the ripple".into(),
                ));
            }
            let d = match edge {
                Edge::Tail => -allowed,
                Edge::Head => allowed,
            };
            for mut c in clips.clone() {
                apply_edge(&mut c, edge, d);
                update(ed, c)?;
            }
            ed.close_gap(&main, at, allowed)?;
            for (c, r) in &partners {
                let at_c = match edge {
                    Edge::Tail => c.end() - allowed,
                    Edge::Head => c.start,
                };
                let amount = allowed.min(ed.free_after(*r, at_c));
                if amount.0 > 0 {
                    ed.shift_after(&[*r], at_c + amount, -amount, false)?;
                }
            }
            Ok(d)
        } else if removed.is_negative() {
            let add = -removed;
            match edge {
                Edge::Tail => {
                    let old_end = first.end();
                    ed.insert_space(&main, old_end, add, true)?;
                    for (c, r) in &partners {
                        ed.insert_space(&[*r], c.end(), add, false)?;
                    }
                    for c in clips {
                        let mut c = ed.clip(c.id)?.1;
                        apply_edge(&mut c, edge, add);
                        update(ed, c)?;
                    }
                }
                Edge::Head => {
                    ed.insert_space(&main, at, add, true)?;
                    for (c, r) in &partners {
                        ed.insert_space(&[*r], c.start, add, false)?;
                    }
                    for c in clips {
                        let mut c = ed.clip(c.id)?.1;
                        apply_edge(&mut c, edge, -add);
                        update(ed, c)?;
                    }
                }
            }
            Ok(d)
        } else {
            Ok(Dur::ZERO)
        }
    })
}

/// Rolling edit: moves the cut at `cut` on `track` (and on linked partner tracks with a matching
/// cut). The combined duration and later positions stay fixed.
pub fn roll(
    p: &mut Project,
    sid: SequenceId,
    track: TrackRef,
    cut: SeqTime,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let t = ed.track(track)?;
        let a = t.clips.iter().find(|c| c.end() == cut).cloned();
        let b = t.clips.iter().find(|c| c.start == cut).cloned();
        if a.is_none() && b.is_none() {
            return Err(EditError::Nothing);
        }
        // partner cuts
        let mut pairs: Vec<(Option<Clip>, Option<Clip>)> = vec![(a.clone(), b.clone())];
        if opts.linked_selection {
            let mut seen: Vec<TrackRef> = vec![track];
            for c in a.iter().chain(b.iter()) {
                for id in ed.seq.linked(c.id) {
                    let (r, _) = ed.clip(id)?;
                    if seen.contains(&r) {
                        continue;
                    }
                    seen.push(r);
                    let tr = ed.track(r)?;
                    let pa = tr.clips.iter().find(|c| c.end() == cut).cloned();
                    let pb = tr.clips.iter().find(|c| c.start == cut).cloned();
                    if pa.is_some() || pb.is_some() {
                        pairs.push((pa, pb));
                    }
                }
            }
        }
        let mut d = delta;
        for (a, b) in &pairs {
            for c in a.iter().chain(b.iter()) {
                let (r, _) = ed.clip(c.id)?;
                ed.writable(r)?;
            }
            if let Some(a) = a {
                let (lo, hi) = edge_bounds(ed, a, Edge::Tail, b.is_none());
                d = d.clamp(lo, hi);
            }
            if let Some(b) = b {
                let (lo, hi) = edge_bounds(ed, b, Edge::Head, a.is_none());
                d = d.clamp(lo, hi);
            }
        }
        if d.0 == 0 {
            return Ok(d);
        }
        for (a, b) in pairs {
            if let Some(mut a) = a {
                apply_edge(&mut a, Edge::Tail, d);
                update(ed, a)?;
            }
            if let Some(mut b) = b {
                apply_edge(&mut b, Edge::Head, d);
                update(ed, b)?;
            }
        }
        // transitions on the cut follow it
        for (r, _) in ed
            .seq
            .all_tracks()
            .map(|(r, t)| (r, t.id))
            .collect::<Vec<_>>()
        {
            let t = ed.track_mut(r)?;
            for tr in &mut t.transitions {
                if tr.cut == cut {
                    tr.cut += d;
                }
            }
        }
        Ok(d)
    })
}

/// Slip: shifts the source range by `delta` (source time); the clip's place and length stay.
pub fn slip(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let clips = members(ed, clip)?;
        let mut d = delta;
        for c in &clips {
            if c.hold.is_some() {
                continue;
            }
            if let Some(a) = ed.available(c) {
                let s = c.source_range();
                d = d.clamp(a.start - s.start, a.end - s.end);
            }
        }
        if d.0 == 0 {
            return Ok(d);
        }
        for mut c in clips {
            c.source_in += d;
            update(ed, c)?;
        }
        Ok(d)
    })
}

/// Slide: moves a clip between its neighbors, which trim to compensate (TL-TOOL-009).
pub fn slide(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let clips = members(ed, clip)?;
        let mut d = delta;
        let mut plan = Vec::new();
        for c in &clips {
            let (r, _) = ed.clip(c.id)?;
            let t = ed.track(r)?;
            let prev = t.clips.iter().find(|n| n.end() == c.start).cloned();
            let next = t.clips.iter().find(|n| n.start == c.end()).cloned();
            match &prev {
                Some(a) => {
                    ed.writable(r)?;
                    let (lo, hi) = edge_bounds(ed, a, Edge::Tail, false);
                    d = d.clamp(lo, hi);
                }
                None => {
                    let gap_start = t
                        .clips
                        .iter()
                        .filter(|n| n.end() <= c.start)
                        .map(|n| n.end())
                        .max()
                        .unwrap_or(SeqTime::ZERO);
                    d = d.max(gap_start - c.start);
                }
            }
            match &next {
                Some(b) => {
                    let (lo, hi) = edge_bounds(ed, b, Edge::Head, false);
                    d = d.clamp(lo, hi);
                }
                None => {
                    if let Some(n) = t
                        .clips
                        .iter()
                        .filter(|n| n.start >= c.end() && n.id != c.id)
                        .map(|n| n.start)
                        .min()
                    {
                        d = d.min(n - c.end());
                    }
                }
            }
            plan.push((c.clone(), prev, next));
        }
        if d.0 == 0 {
            return Ok(d);
        }
        for (mut c, prev, next) in plan {
            if let Some(mut a) = prev {
                apply_edge(&mut a, Edge::Tail, d);
                update(ed, a)?;
            }
            if let Some(mut b) = next {
                apply_edge(&mut b, Edge::Head, d);
                update(ed, b)?;
            }
            let old_start = c.start;
            let old_end = c.end();
            c.start += d;
            update(ed, c.clone())?;
            let (r, _) = ed.clip(c.id)?;
            for tr in &mut ed.track_mut(r)?.transitions {
                if tr.cut == old_start || tr.cut == old_end {
                    tr.cut += d;
                }
            }
        }
        Ok(d)
    })
}

/// Rate stretch: changes the clip duration by changing its speed, keeping the source range.
pub fn rate_stretch(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    edge: Edge,
    delta: Dur,
    opts: EditOptions,
) -> EditResult<Dur> {
    run(p, sid, opts, |ed| {
        let clips = members(ed, clip)?;
        let mut d = delta;
        for c in &clips {
            let src = c.source_duration();
            let (r, _) = ed.clip(c.id)?;
            let t = ed.track(r)?;
            // between 1% and 10000% speed
            let longest = Dur(src.0.saturating_mul(100));
            let shortest = Dur((src.0 / 100).max(ed.frame().0));
            match edge {
                Edge::Tail => {
                    d = d.clamp(shortest - c.duration, longest - c.duration);
                    if let Some(n) = t
                        .clips
                        .iter()
                        .filter(|n| n.id != c.id && n.start >= c.end())
                        .map(|n| n.start)
                        .min()
                    {
                        d = d.min(n - c.end());
                    }
                }
                Edge::Head => {
                    d = d.clamp(c.duration - longest, c.duration - shortest);
                    let prev = t
                        .clips
                        .iter()
                        .filter(|n| n.id != c.id && n.end() <= c.start)
                        .map(|n| n.end())
                        .max();
                    d = d.max(prev.unwrap_or(SeqTime::ZERO) - c.start);
                }
            }
        }
        if d.0 == 0 {
            return Ok(d);
        }
        for mut c in clips {
            let src = c.source_duration();
            match edge {
                Edge::Tail => c.duration += d,
                Edge::Head => {
                    c.start += d;
                    c.duration -= d;
                }
            }
            c.speed = Speed::from_durations(src, c.duration);
            update(ed, c)?;
        }
        Ok(d)
    })
}

/// Speed/Duration dialog. With `ripple`, later material moves by the change in length;
/// otherwise the new length is limited by the next clip.
pub fn set_speed(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    speed: Speed,
    reverse: bool,
    ripple: bool,
    opts: EditOptions,
) -> EditResult {
    run(p, sid, opts, |ed| {
        let ids = ed.with_links(clips);
        for id in ids {
            let (r, mut c) = ed.clip(id)?;
            ed.writable(r)?;
            let src = c.source_duration();
            let mut new_dur = speed
                .to_sequence(src)
                .round_frames(ed.seq.rate())
                .max(ed.frame());
            let old_end = c.end();
            if ripple {
                let delta = new_dur - c.duration;
                let mut tracks = vec![r];
                for s in ed.sync_tracks() {
                    if !tracks.contains(&s) {
                        tracks.push(s);
                    }
                }
                if delta.is_positive() {
                    ed.insert_space(&tracks, old_end, delta, true)?;
                } else if delta.is_negative() {
                    // shorten first, then close
                    c.duration = new_dur;
                    c.speed = Speed::from_durations(src, new_dur);
                    c.reverse = reverse;
                    update(ed, c)?;
                    ed.close_gap(&tracks, old_end + delta, -delta)?;
                    continue;
                }
            } else if let Some(n) = ed
                .track(r)?
                .clips
                .iter()
                .filter(|n| n.id != id && n.start >= old_end)
                .map(|n| n.start)
                .min()
            {
                new_dur = new_dur.min(n - c.start);
            }
            let c2 = ed.clip(id)?.1;
            c = c2;
            c.duration = new_dur;
            c.speed = Speed::from_durations(src, new_dur);
            c.reverse = reverse;
            update(ed, c)?;
        }
        Ok(())
    })
}

/// Add Frame Hold: splits at `t` and freezes the part after it on the frame shown at `t`.
pub fn add_frame_hold(
    p: &mut Project,
    sid: SequenceId,
    clip: ClipId,
    t: SeqTime,
    opts: EditOptions,
) -> EditResult<ClipId> {
    run(
        p,
        sid,
        EditOptions {
            linked_selection: false,
            ..opts
        },
        |ed| {
            let (r, c) = ed.clip(clip)?;
            ed.writable(r)?;
            if !c.is_video() || !c.range().contains(t) {
                return Err(EditError::Nothing);
            }
            let frame = c.to_source(t);
            let target = if t == c.start {
                clip
            } else {
                ed.split(&[clip], t)
                    .first()
                    .map(|p| p.1)
                    .ok_or(EditError::Nothing)?
            };
            let (_, mut held) = ed.clip(target)?;
            held.hold = Some(frame);
            held.link = None;
            update(ed, held)?;
            Ok(target)
        },
    )
}

/// Extend Edit (E): moves the nearest edge of each clip to the playhead (trim, respecting neighbors).
pub fn extend_to(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    playhead: SeqTime,
    opts: EditOptions,
) -> EditResult {
    let mut any = false;
    for &id in clips {
        let Some(c) = p.sequence(sid).and_then(|s| s.clip(id)).cloned() else {
            continue;
        };
        let (edge, delta) = if playhead >= c.end() {
            (Edge::Tail, playhead - c.end())
        } else if playhead <= c.start || playhead - c.start < c.end() - playhead {
            (Edge::Head, playhead - c.start)
        } else {
            (Edge::Tail, playhead - c.end())
        };
        if trim(
            p,
            sid,
            id,
            edge,
            delta,
            EditOptions {
                linked_selection: false,
                ..opts
            },
        )?
        .0 != 0
        {
            any = true;
        }
    }
    if any { Ok(()) } else { Err(EditError::Nothing) }
}

/// Trim In/Out to playhead without rippling (clip under the playhead on each target track).
pub fn trim_to_playhead(
    p: &mut Project,
    sid: SequenceId,
    targets: &[TrackRef],
    playhead: SeqTime,
    head: bool,
    opts: EditOptions,
) -> EditResult {
    let clips: Vec<ClipId> = {
        let seq = p.sequence(sid).ok_or(EditError::Nothing)?;
        targets
            .iter()
            .filter_map(|r| {
                seq.track(*r)
                    .and_then(|t| t.clip_at(playhead))
                    .map(|c| c.id)
            })
            .collect()
    };
    if clips.is_empty() {
        return Err(EditError::Nothing);
    }
    for id in clips {
        let c = p
            .sequence(sid)
            .and_then(|s| s.clip(id))
            .cloned()
            .ok_or(EditError::Nothing)?;
        let (edge, delta) = if head {
            (Edge::Head, playhead - c.start)
        } else {
            (Edge::Tail, playhead - c.end())
        };
        trim(
            p,
            sid,
            id,
            edge,
            delta,
            EditOptions {
                linked_selection: false,
                ..opts
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    /// A linked partner that was slipped out of sync keeps its own edit point in a ripple trim:
    /// time is added next to it instead of cutting through it (found by the randomized edits).
    #[test]
    fn ripple_trim_with_an_out_of_sync_partner() {
        let mut f = fixture();
        f.put(0.0, 10.0, 0.0);
        let ids = f.put(20.0, 28.0, 12.0);
        let seq = f.seq;
        let audio = ids
            .iter()
            .copied()
            .find(|c| !f.seq().clip(*c).unwrap().is_video())
            .unwrap();
        let video = ids.iter().copied().find(|c| *c != audio).unwrap();
        let unlinked = EditOptions {
            linked_selection: false,
            ..EditOptions::default()
        };
        f.apply(|p| {
            crate::move_clips(
                p,
                seq,
                &crate::MoveSpec {
                    clips: vec![audio],
                    delta: d(-1.0),
                    track_delta: 0,
                    insert: false,
                    duplicate: false,
                },
                unlinked,
            )
        })
        .unwrap();
        assert_eq!(
            f.layout(TrackRef::audio(0)),
            vec![(0.0, 10.0), (11.0, 19.0)]
        );
        f.apply(|p| ripple_trim(p, seq, video, Edge::Head, d(-1.5), EditOptions::default()))
            .unwrap();
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 10.0), (12.0, 21.5)]
        );
        assert_eq!(
            f.layout(TrackRef::audio(0)),
            vec![(0.0, 10.0), (11.0, 20.5)]
        );
    }

    #[test]
    fn trim_is_bounded_by_media_and_neighbors() {
        let mut f = fixture();
        let a = f.put(10.0, 20.0, 0.0);
        f.put(30.0, 35.0, 12.0);
        let seq = f.seq;
        // tail can grow only until the next clip at 12 s
        let applied = f
            .apply(|p| trim(p, seq, a[0], Edge::Tail, d(5.0), EditOptions::default()))
            .unwrap();
        assert_eq!(applied, d(2.0));
        // head cannot go before the sequence start
        let applied = f
            .apply(|p| trim(p, seq, a[0], Edge::Head, d(-3.0), EditOptions::default()))
            .unwrap();
        assert_eq!(applied, Dur::ZERO);
        // shrink the head: media in point follows
        f.apply(|p| trim(p, seq, a[0], Edge::Head, d(1.0), EditOptions::default()))
            .unwrap();
        let c = f.seq().clip(a[0]).unwrap().clone();
        assert_eq!((c.start, c.source_in), (s(1.0), src(11.0)));
        // linked audio followed
        assert_eq!(f.layout(TrackRef::audio(0))[0], (1.0, 12.0));
        // head can extend back to the media start (source 10 s at 0 s)
        let applied = f
            .apply(|p| trim(p, seq, a[0], Edge::Head, d(-5.0), EditOptions::default()))
            .unwrap();
        assert_eq!(applied, d(-1.0));
    }

    #[test]
    fn ripple_trim_moves_later_clips() {
        let mut f = fixture();
        let a = f.put(10.0, 20.0, 0.0);
        f.put(30.0, 35.0, 10.0);
        let seq = f.seq;
        f.apply(|p| ripple_trim(p, seq, a[0], Edge::Tail, d(-2.0), EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 8.0), (8.0, 13.0)]);
        assert_eq!(f.layout(TrackRef::audio(0)), vec![(0.0, 8.0), (8.0, 13.0)]);
        f.apply(|p| ripple_trim(p, seq, a[0], Edge::Tail, d(1.0), EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 9.0), (9.0, 14.0)]);
        // head ripple keeps the clip start and pulls the rest in
        f.apply(|p| ripple_trim(p, seq, a[0], Edge::Head, d(2.0), EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 7.0), (7.0, 12.0)]);
        assert_eq!(f.seq().clip(a[0]).unwrap().source_in, src(12.0));
        // head ripple outwards adds frames before
        f.apply(|p| ripple_trim(p, seq, a[0], Edge::Head, d(-1.0), EditOptions::default()))
            .unwrap();
        assert_eq!(f.layout(TrackRef::video(0)), vec![(0.0, 8.0), (8.0, 13.0)]);
        assert_eq!(f.seq().clip(a[0]).unwrap().source_in, src(11.0));
    }

    #[test]
    fn roll_keeps_total_duration() {
        let mut f = fixture();
        f.put(10.0, 20.0, 0.0);
        f.put(30.0, 40.0, 10.0);
        let seq = f.seq;
        let applied = f
            .apply(|p| {
                roll(
                    p,
                    seq,
                    TrackRef::video(0),
                    s(10.0),
                    d(2.0),
                    EditOptions::default(),
                )
            })
            .unwrap();
        assert_eq!(applied, d(2.0));
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 12.0), (12.0, 20.0)]
        );
        assert_eq!(
            f.layout(TrackRef::audio(0)),
            vec![(0.0, 12.0), (12.0, 20.0)]
        );
        assert_eq!(f.seq().video[0].clips[1].source_in, src(32.0));
        // rolling far back stops when the outgoing clip is one frame long
        let applied = f
            .apply(|p| {
                roll(
                    p,
                    seq,
                    TrackRef::video(0),
                    s(12.0),
                    d(-30.0),
                    EditOptions::default(),
                )
            })
            .unwrap();
        assert_eq!(applied, -(d(12.0) - Rate::FPS_25.frame_duration()));
    }

    #[test]
    fn slip_and_slide() {
        let mut f = fixture();
        f.put(10.0, 20.0, 0.0);
        let mid = f.put(30.0, 35.0, 10.0);
        f.put(40.0, 50.0, 15.0);
        let seq = f.seq;
        let applied = f
            .apply(|p| slip(p, seq, mid[0], d(100.0), EditOptions::default()))
            .unwrap();
        assert_eq!(applied, d(25.0), "slip is bounded by the media end at 60 s");
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 10.0), (10.0, 15.0), (15.0, 25.0)]
        );
        f.apply(|p| slide(p, seq, mid[0], d(2.0), EditOptions::default()))
            .unwrap();
        assert_eq!(
            f.layout(TrackRef::video(0)),
            vec![(0.0, 12.0), (12.0, 17.0), (17.0, 25.0)]
        );
        assert_eq!(f.seq().video[0].clips[2].source_in, src(42.0));
    }

    #[test]
    fn rate_stretch_and_speed() {
        let mut f = fixture();
        let a = f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        f.apply(|p| rate_stretch(p, seq, a[0], Edge::Tail, d(10.0), EditOptions::default()))
            .unwrap();
        let c = f.seq().clip(a[0]).unwrap().clone();
        assert_eq!(c.duration, d(20.0));
        assert!((c.speed.percent() - 50.0).abs() < 1e-9);
        assert_eq!(c.source_range().end, src(10.0));
        f.apply(|p| {
            set_speed(
                p,
                seq,
                &[a[0]],
                Speed::from_percent(200.0),
                false,
                false,
                EditOptions::default(),
            )
        })
        .unwrap();
        assert_eq!(f.seq().clip(a[0]).unwrap().duration, d(5.0));
    }

    #[test]
    fn frame_hold_splits_and_freezes() {
        let mut f = fixture();
        let a = f.put(0.0, 10.0, 0.0);
        let seq = f.seq;
        let held = f
            .apply(|p| add_frame_hold(p, seq, a[0], s(4.0), EditOptions::default()))
            .unwrap();
        let c = f.seq().clip(held).unwrap();
        assert_eq!(c.hold, Some(src(4.0)));
        assert_eq!(c.to_source(s(9.0)), src(4.0));
    }
}
