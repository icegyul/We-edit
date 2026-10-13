//! From the places where the music repeats to a plan with the length asked for: which stretches
//! are played again and which are skipped, and the pieces of the original that make up the
//! result, in order.

use crate::loops::Loop;

/// What the plan is for.
#[derive(Clone, Copy, Debug)]
pub struct PlanOptions {
    /// The part of the analysed sound to remix, in seconds.
    pub range: (f64, f64),
    /// The length wanted, in seconds.
    pub target: f64,
    /// Shortest stretch that may be repeated or skipped, in seconds.
    pub min_loop: f64,
}

/// One jump: `times` > 0 plays the stretch that many extra times, -1 skips it once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub at: Loop,
    pub times: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub moves: Vec<Move>,
    /// The pieces of the original (start, end in seconds) that, played one after the other, make
    /// the result. Neighbours that follow each other in the original are one piece.
    pub segments: Vec<(f64, f64)>,
    /// Length of the result.
    pub duration: f64,
    /// Length of the original range.
    pub original: f64,
}

/// A moved stretch costs: how badly it joins, and each jump and repeat that is made.
const SCORE_COST: f64 = 8.0;
const MOVE_COST: f64 = 1.5;
const REPEAT_COST: f64 = 0.6;

fn cost(delta_error: f64, moves: &[(Loop, i32)]) -> f64 {
    delta_error
        + moves
            .iter()
            .map(|(l, r)| (1.0 - f64::from(l.score)) * SCORE_COST + MOVE_COST + REPEAT_COST * f64::from(r.abs() - 1))
            .sum::<f64>()
}

/// The best way to reach `opts.target` with at most two kinds of jump.
pub fn plan(loops: &[Loop], opts: &PlanOptions) -> Plan {
    let (a, b) = opts.range;
    let original = (b - a).max(0.0);
    let want = opts.target - original;
    let usable: Vec<Loop> = loops
        .iter()
        .copied()
        .filter(|l| l.start >= a && l.end <= b && l.length() >= opts.min_loop.max(0.5))
        .collect();
    let mut best: (f64, Vec<(Loop, i32)>) = (want.abs(), Vec::new());
    let mut consider = |moves: Vec<(Loop, i32)>| {
        let delta: f64 = moves.iter().map(|(l, r)| l.length() * f64::from(if *r < 0 { -1 } else { *r })).sum();
        let c = cost((want - delta).abs(), &moves);
        if c < best.0 {
            best = (c, moves);
        }
    };
    let counts = |l: &Loop, need: f64| -> Vec<i32> {
        let mut v = vec![-1];
        let n = (need / l.length()).round() as i32;
        for r in [n - 1, n, n + 1] {
            if (1..=12).contains(&r) {
                v.push(r);
            }
        }
        v.dedup();
        v
    };
    for l in &usable {
        for r in counts(l, want) {
            consider(vec![(*l, r)]);
        }
    }
    for (i, l1) in usable.iter().enumerate() {
        for r1 in [-1, 1, 2, 3] {
            let rest = want - l1.length() * f64::from(r1);
            for l2 in &usable[i + 1..] {
                // the two stretches must not overlap
                if l2.start < l1.end && l1.start < l2.end {
                    continue;
                }
                for r2 in counts(l2, rest) {
                    consider(vec![(*l1, r1), (*l2, r2)]);
                }
            }
        }
    }
    let mut chosen = best.1;
    chosen.sort_by(|x, y| x.0.start.total_cmp(&y.0.start));
    // the pieces
    let mut segments: Vec<(f64, f64)> = Vec::new();
    let mut cursor = a;
    for (l, r) in &chosen {
        if *r > 0 {
            segments.push((cursor, l.end));
            for _ in 0..*r {
                segments.push((l.start, l.end));
            }
            cursor = l.end;
        } else {
            segments.push((cursor, l.start));
            cursor = l.end;
        }
    }
    segments.push((cursor, b));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for s in segments {
        match merged.last_mut() {
            Some(last) if (last.1 - s.0).abs() < 1e-9 => last.1 = s.1,
            _ => merged.push(s),
        }
    }
    let duration = merged.iter().map(|(s, e)| e - s).sum();
    Plan { moves: chosen.into_iter().map(|(at, times)| Move { at, times }).collect(), segments: merged, duration, original }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lp(start: f64, end: f64, score: f32) -> Loop {
        Loop { start, end, score }
    }

    #[test]
    fn a_longer_version_repeats_one_stretch() {
        let loops = [lp(8.0, 16.0, 0.97), lp(8.0, 12.0, 0.9), lp(20.0, 40.0, 0.8)];
        let p = plan(&loops, &PlanOptions { range: (0.0, 60.0), target: 76.0, min_loop: 2.0 });
        // two more passes of the 8 s stretch that joins best make exactly 76 s
        assert_eq!(p.moves.len(), 1, "{p:?}");
        assert_eq!((p.moves[0].at.start, p.moves[0].times), (8.0, 2));
        assert!((p.duration - 76.0).abs() < 0.01, "{}", p.duration);
        // the original is played in order with the repeats inside it
        assert_eq!(p.segments.first().unwrap().0, 0.0);
        assert_eq!(p.segments.last().unwrap().1, 60.0);
        let total: f64 = p.segments.iter().map(|s| s.1 - s.0).sum();
        assert!((total - p.duration).abs() < 1e-9);
    }

    #[test]
    fn a_shorter_version_skips_a_stretch() {
        let loops = [lp(8.0, 16.0, 0.97)];
        let p = plan(&loops, &PlanOptions { range: (0.0, 60.0), target: 52.0, min_loop: 2.0 });
        assert_eq!(p.moves.len(), 1);
        assert_eq!(p.moves[0].times, -1);
        assert_eq!(p.segments, vec![(0.0, 8.0), (16.0, 60.0)]);
        assert!((p.duration - 52.0).abs() < 1e-9);
    }

    #[test]
    fn the_length_of_the_original_needs_no_jump() {
        let loops = [lp(8.0, 16.0, 0.97)];
        let p = plan(&loops, &PlanOptions { range: (0.0, 60.0), target: 60.0, min_loop: 2.0 });
        assert!(p.moves.is_empty());
        assert_eq!(p.segments, vec![(0.0, 60.0)]);
    }

    #[test]
    fn a_target_between_two_sizes_takes_the_nearer_and_a_poor_join_is_avoided() {
        // 8 s steps: 60 -> 70 is closest to +8; the better joining stretch wins over the poorer one
        let loops = [lp(8.0, 16.0, 0.99), lp(30.0, 38.0, 0.5)];
        let p = plan(&loops, &PlanOptions { range: (0.0, 60.0), target: 70.0, min_loop: 2.0 });
        assert_eq!(p.moves.len(), 1);
        assert_eq!(p.moves[0].at.start, 8.0);
        assert!((p.duration - 68.0).abs() < 1e-9);
    }
}
