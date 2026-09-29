//! Timeline navigation: edit points, clip under the playhead, gaps.

use op_core::*;

/// Next edit point after `t` on the given tracks (plus the sequence end).
pub fn next_edit(seq: &Sequence, tracks: &[TrackRef], t: SeqTime) -> Option<SeqTime> {
    seq.edit_points(tracks.iter().copied())
        .into_iter()
        .find(|e| *e > t)
}

/// Previous edit point before `t`.
pub fn prev_edit(seq: &Sequence, tracks: &[TrackRef], t: SeqTime) -> Option<SeqTime> {
    seq.edit_points(tracks.iter().copied())
        .into_iter()
        .rev()
        .find(|e| *e < t)
}

/// Clips under `t` on the given tracks, topmost video first then audio (for `D`).
pub fn clips_at(seq: &Sequence, tracks: &[TrackRef], t: SeqTime) -> Vec<ClipId> {
    let mut v: Vec<(TrackRef, ClipId)> = tracks
        .iter()
        .filter_map(|r| {
            seq.track(*r)
                .and_then(|tr| tr.clip_at(t))
                .map(|c| (*r, c.id))
        })
        .collect();
    v.sort_by_key(|(r, _)| {
        (
            r.kind == TrackKind::Audio,
            if r.kind == TrackKind::Video {
                usize::MAX - r.index
            } else {
                r.index
            },
        )
    });
    v.into_iter().map(|(_, c)| c).collect()
}

/// Start of the next gap after `t` on any of the tracks.
pub fn next_gap(seq: &Sequence, tracks: &[TrackRef], t: SeqTime) -> Option<SeqTime> {
    let mut best: Option<SeqTime> = None;
    for r in tracks {
        let Some(track) = seq.track(*r) else { continue };
        for w in track.clips.windows(2) {
            if w[0].end() < w[1].start && w[0].end() > t {
                best = Some(best.map_or(w[0].end(), |b: SeqTime| b.min(w[0].end())));
            }
        }
    }
    best
}

/// Start of the previous gap before `t`.
pub fn prev_gap(seq: &Sequence, tracks: &[TrackRef], t: SeqTime) -> Option<SeqTime> {
    let mut best: Option<SeqTime> = None;
    for r in tracks {
        let Some(track) = seq.track(*r) else { continue };
        if let Some(first) = track.clips.first()
            && first.start > SeqTime::ZERO
            && SeqTime::ZERO < t
        {
            best = Some(best.map_or(SeqTime::ZERO, |b: SeqTime| b.max(SeqTime::ZERO)));
        }
        for w in track.clips.windows(2) {
            if w[0].end() < w[1].start && w[0].end() < t {
                best = Some(best.map_or(w[0].end(), |b: SeqTime| b.max(w[0].end())));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn edits_and_gaps() {
        let mut f = fixture();
        f.put(0.0, 5.0, 0.0);
        f.put(10.0, 12.0, 7.0);
        let tracks = [TrackRef::video(0)];
        let seq = f.seq();
        assert_eq!(next_edit(seq, &tracks, s(1.0)), Some(s(5.0)));
        assert_eq!(prev_edit(seq, &tracks, s(7.0)), Some(s(5.0)));
        assert_eq!(next_gap(seq, &tracks, s(1.0)), Some(s(5.0)));
        assert_eq!(prev_gap(seq, &tracks, s(8.0)), Some(s(5.0)));
        let all = [TrackRef::video(0), TrackRef::audio(0)];
        let under = clips_at(seq, &all, s(1.0));
        assert_eq!(under.len(), 2);
        assert!(seq.clip(under[0]).unwrap().is_video());
    }
}
