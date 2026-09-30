//! Randomized edit sequences (the automated part of the timeline behavior matrix,
//! docs/implementation-status.md): every operation either refuses cleanly or leaves a project
//! that passes validation, whatever the order, the lock state or the targets.

#[cfg(test)]
mod tests {
    use crate::testutil::*;
    use crate::*;
    use op_core::catalog;

    /// A small deterministic generator (xorshift), so failures replay exactly.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
        /// Seconds on the 25 fps frame grid in `[0, max)`.
        fn secs(&mut self, max: f64) -> f64 {
            (self.below((max * 25.0) as usize) as f64) / 25.0
        }
        fn coin(&mut self) -> bool {
            self.next() & 1 == 1
        }
    }

    fn ok<T>(r: EditResult<T>) -> EditResult<()> {
        r.map(|_| ())
    }

    fn all_tracks(seq: &Sequence) -> Vec<TrackRef> {
        seq.all_tracks().map(|(r, _)| r).collect()
    }

    fn any_clip(seq: &Sequence, rng: &mut Rng) -> Option<Clip> {
        let clips: Vec<Clip> = seq.clips().map(|(_, c)| c.clone()).collect();
        (!clips.is_empty()).then(|| clips[rng.below(clips.len())].clone())
    }

    fn step(f: &Fixture, rng: &mut Rng, p: &mut Project) -> (&'static str, EditResult<()>) {
        let sid = f.seq;
        let seq = p.sequence(sid).unwrap().clone();
        let opts = EditOptions {
            linked_selection: rng.coin(),
            ripple_markers: rng.coin(),
        };
        match rng.below(12) {
            0 | 1 => {
                let from = rng.secs(50.0);
                let spec = f.source(from, from + 0.2 + rng.secs(8.0));
                let patch = Patch {
                    video: Some(rng.below(3)),
                    audio: vec![Some(rng.below(3))],
                };
                (
                    "overwrite",
                    ok(overwrite(
                        p,
                        sid,
                        &spec,
                        SeqTime::from_seconds(rng.secs(40.0)),
                        &patch,
                        opts,
                    )),
                )
            }
            2 => {
                let from = rng.secs(50.0);
                let spec = f.source(from, from + 0.2 + rng.secs(5.0));
                let patch = Patch {
                    video: Some(rng.below(3)),
                    audio: vec![Some(rng.below(3))],
                };
                (
                    "insert",
                    ok(insert(
                        p,
                        sid,
                        &spec,
                        SeqTime::from_seconds(rng.secs(40.0)),
                        &patch,
                        opts,
                    )),
                )
            }
            3 => match any_clip(&seq, rng) {
                Some(c) => {
                    let t = c.start + Dur::from_seconds(rng.secs(c.duration.seconds()));
                    ("razor", ok(razor(p, sid, &[c.id], t, opts)))
                }
                None => ("razor", Err(EditError::Nothing)),
            },
            4 => {
                let t = SeqTime::from_seconds(rng.secs(40.0));
                ("add edit", ok(add_edit(p, sid, &all_tracks(&seq), t, opts)))
            }
            5 => match any_clip(&seq, rng) {
                Some(c) if rng.coin() => {
                    ("ripple delete", ok(ripple_delete(p, sid, &[c.id], opts)))
                }
                Some(c) => ("delete", ok(delete(p, sid, &[c.id], &[], opts))),
                None => ("delete", Err(EditError::Nothing)),
            },
            6 => {
                let a = rng.secs(40.0);
                let range = SeqRange::new(
                    SeqTime::from_seconds(a),
                    SeqTime::from_seconds(a + 0.2 + rng.secs(6.0)),
                );
                let targets = all_tracks(&seq);
                if rng.coin() {
                    ("lift", ok(lift(p, sid, range, &targets, opts)))
                } else {
                    ("extract", ok(extract(p, sid, range, &targets, opts)))
                }
            }
            7 | 8 => match any_clip(&seq, rng) {
                Some(c) => {
                    let edge = if rng.coin() { Edge::Head } else { Edge::Tail };
                    let delta = Dur::from_seconds(rng.secs(4.0) - 2.0);
                    if rng.coin() {
                        ("trim", ok(trim(p, sid, c.id, edge, delta, opts)))
                    } else {
                        (
                            "ripple trim",
                            ok(ripple_trim(p, sid, c.id, edge, delta, opts)),
                        )
                    }
                }
                None => ("trim", Err(EditError::Nothing)),
            },
            9 => match any_clip(&seq, rng) {
                Some(c) => {
                    let spec = MoveSpec {
                        clips: vec![c.id],
                        delta: Dur::from_seconds(rng.secs(6.0) - 3.0),
                        track_delta: rng.below(3) as i32 - 1,
                        insert: rng.coin(),
                        duplicate: rng.below(4) == 0,
                    };
                    ("move", ok(move_clips(p, sid, &spec, opts)))
                }
                None => ("move", Err(EditError::Nothing)),
            },
            10 => match any_clip(&seq, rng) {
                Some(c) => {
                    let (r, _) = seq.find_clip(c.id).unwrap();
                    let cut = if rng.coin() { c.start } else { c.end() };
                    let effect = if r.kind == TrackKind::Video {
                        catalog::CROSS_DISSOLVE
                    } else {
                        catalog::CONSTANT_POWER
                    };
                    let align = [
                        Alignment::CenterAtCut,
                        Alignment::StartAtCut,
                        Alignment::EndAtCut,
                    ][rng.below(3)];
                    let dur = Dur::from_seconds(0.2 + rng.secs(2.0));
                    (
                        "transition",
                        ok(add_transition(
                            p,
                            sid,
                            r,
                            cut,
                            effect,
                            dur,
                            Some(align),
                            opts,
                        )),
                    )
                }
                None => ("transition", Err(EditError::Nothing)),
            },
            _ => {
                // lock or unlock a track: edits must then refuse to touch it
                let tracks = all_tracks(&seq);
                let r = tracks[rng.below(tracks.len())];
                let t = p.sequence_mut(sid).unwrap().track_mut(r).unwrap();
                t.locked = !t.locked;
                ("lock", Ok(()))
            }
        }
    }

    #[test]
    fn random_edits_keep_the_project_valid() {
        for seed in 1..=12u64 {
            let mut f = fixture();
            let sid = f.seq;
            for kind in [TrackKind::Video, TrackKind::Audio] {
                f.apply(|p| add_tracks(p, sid, kind, 2, 1, AudioTrackLayout::Standard))
                    .unwrap();
            }
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            let mut applied = 0;
            for i in 0..400 {
                let mut draft = f.p.clone();
                let before = f.p.clone();
                let (name, r) = step(&f, &mut rng, &mut draft);
                if r.is_err() {
                    continue;
                }
                draft.normalize();
                if let Err(e) = draft.validate() {
                    panic!("seed {seed}, step {i}: {name} left an invalid project: {e}");
                }
                if name != "lock" {
                    // edits never move, trim or replace clips on a locked track (a link to a
                    // partner removed elsewhere may be dropped)
                    let content = |t: &Track| -> Vec<_> {
                        t.clips
                            .iter()
                            .map(|c| (c.id, c.start, c.duration, c.source_in, c.speed, c.enabled))
                            .collect()
                    };
                    for (r, t) in before.sequence(f.seq).unwrap().all_tracks() {
                        if t.locked {
                            let after = draft.sequence(f.seq).unwrap().track(r).unwrap();
                            assert_eq!(
                                content(t),
                                content(after),
                                "seed {seed}, step {i}: {name} changed locked track {r:?}"
                            );
                        }
                    }
                }
                f.p = draft;
                applied += 1;
            }
            assert!(applied > 50, "seed {seed}: only {applied} edits applied");
        }
    }
}
