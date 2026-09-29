//! Snapping (TL-SNAP). The threshold is a time span the caller derives from a screen distance at
//! the current zoom (TL-SNAP-004); this module only chooses targets deterministically.

use op_core::*;

/// Target kinds in priority order for ties (TL-SNAP-005).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum SnapKind {
    Playhead,
    Marker,
    InOut,
    ClipEdge,
    SequenceStart,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SnapTarget {
    pub time: SeqTime,
    pub kind: SnapKind,
}

/// Snap targets of a sequence, excluding the edges of clips being dragged.
pub fn snap_targets(
    seq: &Sequence,
    exclude: &[ClipId],
    playhead: Option<SeqTime>,
) -> Vec<SnapTarget> {
    let mut v = vec![SnapTarget {
        time: SeqTime::ZERO,
        kind: SnapKind::SequenceStart,
    }];
    if let Some(p) = playhead {
        v.push(SnapTarget {
            time: p,
            kind: SnapKind::Playhead,
        });
    }
    for m in &seq.markers {
        v.push(SnapTarget {
            time: m.start,
            kind: SnapKind::Marker,
        });
        if m.duration.0 > 0 {
            v.push(SnapTarget {
                time: m.end(),
                kind: SnapKind::Marker,
            });
        }
    }
    for t in [seq.mark_in, seq.mark_out].into_iter().flatten() {
        v.push(SnapTarget {
            time: t,
            kind: SnapKind::InOut,
        });
    }
    for (_, c) in seq.clips() {
        if exclude.contains(&c.id) {
            continue;
        }
        v.push(SnapTarget {
            time: c.start,
            kind: SnapKind::ClipEdge,
        });
        v.push(SnapTarget {
            time: c.end(),
            kind: SnapKind::ClipEdge,
        });
    }
    v.sort_by_key(|t| (t.time, t.kind));
    v.dedup_by_key(|t| t.time);
    v
}

/// Adjusts a drag `delta` so that one of the `anchors` (moving edges, before the drag) lands on
/// the closest target within `threshold`. Returns the adjusted delta and the target hit.
pub fn snap(
    anchors: &[SeqTime],
    delta: Dur,
    targets: &[SnapTarget],
    threshold: Dur,
) -> Option<(Dur, SnapTarget)> {
    let mut best: Option<(Dur, Dur, SnapTarget)> = None;
    for a in anchors {
        let moved = *a + delta;
        for t in targets {
            let dist = (t.time - moved).abs();
            if dist > threshold {
                continue;
            }
            let better = match &best {
                None => true,
                Some((bd, _, bt)) => (dist, t.kind, t.time) < (*bd, bt.kind, bt.time),
            };
            if better {
                best = Some((dist, t.time - *a, *t));
            }
        }
    }
    best.map(|(_, d, t)| (d, t))
}

/// Snaps a single time (playhead scrubbing, razor position).
pub fn snap_time(t: SeqTime, targets: &[SnapTarget], threshold: Dur) -> Option<SnapTarget> {
    snap(&[t], Dur::ZERO, targets, threshold).map(|(_, target)| target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn nearest_target_wins_and_ties_prefer_the_playhead() {
        let mut f = fixture();
        let a = f.put(0.0, 5.0, 0.0);
        f.put(10.0, 15.0, 10.0);
        let targets = snap_targets(f.seq(), &a, Some(s(8.0)));
        // dragging the clip [0,5) by 4.9 s: its tail (5) lands near 10 (clip), its head near 5
        let (d2, hit) = snap(&[s(0.0), s(5.0)], d(4.8), &targets, d(0.5)).unwrap();
        assert_eq!(hit.kind, SnapKind::ClipEdge);
        assert_eq!(d2, d(5.0));
        // within reach of both the playhead at 8 and nothing else
        let (d3, hit) = snap(&[s(0.0), s(5.0)], d(2.9), &targets, d(0.5)).unwrap();
        assert_eq!((hit.kind, d3), (SnapKind::Playhead, d(3.0)));
        assert!(snap(&[s(0.0)], d(30.0), &targets, d(0.5)).is_none());
    }
}
