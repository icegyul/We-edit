//! Transitions attach to edit points (TL-TR-001). Duration, alignment, edge and sidedness are
//! independent fields (DM-TL-006, TL-TR-007).

use op_core::*;

use crate::{EditOptions, run};

/// Adds (or replaces) a transition at the edit point `cut` of a track. Without an explicit
/// alignment it is centered on a cut between two clips, and starts/ends at the cut on a clip's
/// free head/tail. The duration is limited so the transition stays inside its clips; where media
/// handles run out, edge frames repeat (TL-TR-004 candidate).
pub fn add_transition(
    p: &mut Project,
    sid: SequenceId,
    track: TrackRef,
    cut: SeqTime,
    effect: &str,
    duration: Dur,
    alignment: Option<Alignment>,
    opts: EditOptions,
) -> EditResult<TransitionId> {
    let def = catalog::find(effect).ok_or_else(|| EditError::NotFound(effect.into()))?;
    let video = track.kind == TrackKind::Video;
    if def.kind.is_video() != video || !def.kind.is_transition() {
        return Err(EditError::Invalid(
            "transition type does not match the track".into(),
        ));
    }
    run(p, sid, opts, |ed| {
        ed.writable(track)?;
        let rate = ed.seq.rate();
        let t = ed.track(track)?;
        let from = t.clips.iter().find(|c| c.end() == cut).cloned();
        let to = t.clips.iter().find(|c| c.start == cut).cloned();
        let alignment = alignment.unwrap_or(match (&from, &to) {
            (Some(_), Some(_)) => Alignment::CenterAtCut,
            (Some(_), None) => Alignment::EndAtCut,
            (None, Some(_)) => Alignment::StartAtCut,
            (None, None) => return Err(EditError::Nothing),
        });
        let (from, to) = match alignment {
            // a one-sided transition only uses the clip on its side
            Alignment::EndAtCut if to.is_none() => (from, None),
            Alignment::StartAtCut if from.is_none() => (None, to),
            _ => (from, to),
        };
        if from.is_none() && to.is_none() {
            return Err(EditError::Nothing);
        }
        let mut duration = duration.round_frames(rate).max(rate.frame_duration());
        let mut tr = Transition {
            id: ed.ids.transition(),
            effect: effect.to_string(),
            cut,
            duration,
            alignment,
            from: from.as_ref().map(|c| c.id),
            to: to.as_ref().map(|c| c.id),
            params: def
                .params
                .iter()
                .map(|s| Param::new(s.key, s.default_value()))
                .collect(),
        };
        align_center(&mut tr, rate);
        // stay inside the clips on each side
        let before_room = from.as_ref().map(|c| c.duration).unwrap_or(Dur::ZERO);
        let after_room = to.as_ref().map(|c| c.duration).unwrap_or(Dur::ZERO);
        let before = tr.before_cut().min(before_room);
        let after = (duration - tr.before_cut()).min(after_room);
        if before + after != duration {
            duration = before + after;
            tr.duration = duration;
            tr.alignment = match alignment {
                Alignment::CenterAtCut | Alignment::Custom(_) => Alignment::Custom(before),
                a => a,
            };
        }
        if tr.duration.0 <= 0 {
            return Err(EditError::Nothing);
        }
        let t = ed.track_mut(track)?;
        // replacing: keep the existing transition's timing when only the effect changes
        if let Some(existing) = t
            .transitions
            .iter_mut()
            .find(|x| x.cut == cut && sides_match(x, &tr))
        {
            existing.effect = tr.effect;
            existing.params = tr.params;
            return Ok(existing.id);
        }
        let id = tr.id;
        t.transitions
            .retain(|x| !(x.cut == cut && x.range().overlaps(&tr.range())));
        t.transitions.push(tr);
        t.sort();
        Ok(id)
    })
}

/// A centered transition with an odd frame count keeps its edges on frame boundaries: the extra
/// frame goes after the cut (provisional; odd-duration placement is open in TL-TR).
fn align_center(tr: &mut Transition, rate: Rate) {
    if tr.alignment == Alignment::CenterAtCut {
        let frames = rate.dur_to_frames_round(tr.duration);
        let before = rate.frames_to_dur(frames / 2);
        if before * 2 != tr.duration {
            tr.alignment = Alignment::Custom(before);
        }
    }
}

fn sides_match(a: &Transition, b: &Transition) -> bool {
    a.from.is_some() == b.from.is_some() && a.to.is_some() == b.to.is_some()
}

/// Changes duration and/or alignment of a transition; the duration is limited by its clips.
pub fn set_transition_timing(
    p: &mut Project,
    sid: SequenceId,
    id: TransitionId,
    duration: Option<Dur>,
    alignment: Option<Alignment>,
) -> EditResult {
    run(p, sid, EditOptions::default(), |ed| {
        let (r, mut tr, from, to) = {
            let (r, t) = ed
                .seq
                .all_tracks()
                .find(|(_, t)| t.transition(id).is_some())
                .ok_or(EditError::Nothing)?;
            let tr = t.transition(id).unwrap().clone();
            let from = tr.from.and_then(|c| t.clip(c)).map(|c| c.duration);
            let to = tr.to.and_then(|c| t.clip(c)).map(|c| c.duration);
            (r, tr, from, to)
        };
        ed.writable(r)?;
        if let Some(d) = duration {
            tr.duration = d.round_frames(ed.seq.rate()).max(ed.frame());
        }
        if let Some(a) = alignment {
            tr.alignment = a;
        }
        align_center(&mut tr, ed.seq.rate());
        let before = tr.before_cut().min(from.unwrap_or(Dur::ZERO));
        let after = (tr.duration - tr.before_cut()).min(to.unwrap_or(Dur::ZERO));
        if before + after != tr.duration {
            tr.duration = before + after;
            if matches!(tr.alignment, Alignment::CenterAtCut | Alignment::Custom(_)) {
                tr.alignment = Alignment::Custom(before);
            }
        }
        let t = ed.track_mut(r)?;
        if let Some(x) = t.transitions.iter_mut().find(|x| x.id == id) {
            *x = tr;
        }
        Ok(())
    })
}

/// Default transition at the edit point nearest the playhead on each targeted track (Ctrl+D /
/// Ctrl+Shift+D).
pub fn apply_default_at_playhead(
    p: &mut Project,
    sid: SequenceId,
    targets: &[TrackRef],
    playhead: SeqTime,
    opts: EditOptions,
) -> EditResult<Vec<TransitionId>> {
    let mut out = Vec::new();
    for &r in targets {
        let Some(cut) = nearest_edit(p, sid, r, playhead) else {
            continue;
        };
        let (effect, dur) = default_for(p, r.kind);
        if let Ok(id) = add_transition(p, sid, r, cut, &effect, dur, None, opts) {
            out.push(id);
        }
    }
    if out.is_empty() {
        Err(EditError::Nothing)
    } else {
        Ok(out)
    }
}

/// Default transitions at both ends of each selected clip (Shift+D).
pub fn apply_default_to_clips(
    p: &mut Project,
    sid: SequenceId,
    clips: &[ClipId],
    opts: EditOptions,
) -> EditResult<Vec<TransitionId>> {
    let mut out = Vec::new();
    for id in clips {
        let Some((r, c)) = p
            .sequence(sid)
            .and_then(|s| s.find_clip(*id))
            .map(|(r, c)| (r, c.clone()))
        else {
            continue;
        };
        let (effect, dur) = default_for(p, r.kind);
        for cut in [c.start, c.end()] {
            if let Ok(t) = add_transition(p, sid, r, cut, &effect, dur, None, opts)
                && !out.contains(&t)
            {
                out.push(t);
            }
        }
    }
    if out.is_empty() {
        Err(EditError::Nothing)
    } else {
        Ok(out)
    }
}

fn default_for(p: &Project, kind: TrackKind) -> (String, Dur) {
    match kind {
        TrackKind::Video => (
            p.settings.video_transition.clone(),
            p.settings.video_transition_duration,
        ),
        TrackKind::Audio => (
            p.settings.audio_transition.clone(),
            p.settings.audio_transition_duration,
        ),
    }
}

/// The clip edge nearest `t` on a track, preferring edges of the clip under `t`.
pub fn nearest_edit(p: &Project, sid: SequenceId, r: TrackRef, t: SeqTime) -> Option<SeqTime> {
    let track = p.sequence(sid)?.track(r)?;
    let candidates: Vec<SeqTime> = match track.clip_at(t) {
        Some(c) => vec![c.start, c.end()],
        None => track
            .clips
            .iter()
            .flat_map(|c| [c.start, c.end()])
            .collect(),
    };
    candidates.into_iter().min_by_key(|e| (*e - t).abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn centered_one_sided_and_clamped() {
        let mut f = fixture();
        f.put(0.0, 5.0, 0.0);
        f.put(10.0, 10.4, 5.0);
        let seq = f.seq;
        let id = f
            .apply(|p| {
                add_transition(
                    p,
                    seq,
                    TrackRef::video(0),
                    s(5.0),
                    catalog::CROSS_DISSOLVE,
                    d(1.0),
                    None,
                    EditOptions::default(),
                )
            })
            .unwrap();
        let tr = f.seq().video[0].transition(id).unwrap().clone();
        // 25 frames centered: 12 before the cut (frame aligned); the incoming clip is only 0.4 s long
        assert_eq!(tr.range(), SeqRange::new(s(4.52), s(5.4)));
        // head of the first clip: one-sided, starting at the cut
        let head = f
            .apply(|p| {
                add_transition(
                    p,
                    seq,
                    TrackRef::video(0),
                    s(0.0),
                    "op.tr.dip_to_black",
                    d(1.0),
                    None,
                    EditOptions::default(),
                )
            })
            .unwrap();
        let tr = f.seq().video[0].transition(head).unwrap().clone();
        assert_eq!(
            (tr.alignment, tr.from.is_none()),
            (Alignment::StartAtCut, true)
        );
        // audio transitions go on audio tracks only
        let r = f.apply(|p| {
            add_transition(
                p,
                seq,
                TrackRef::audio(0),
                s(5.0),
                catalog::CROSS_DISSOLVE,
                d(1.0),
                None,
                EditOptions::default(),
            )
        });
        assert!(r.is_err());
    }

    #[test]
    fn default_at_playhead_and_replacement() {
        let mut f = fixture();
        f.put(0.0, 5.0, 0.0);
        f.put(10.0, 20.0, 5.0);
        let seq = f.seq;
        let ids = f
            .apply(|p| {
                apply_default_at_playhead(
                    p,
                    seq,
                    &[TrackRef::video(0), TrackRef::audio(0)],
                    s(5.3),
                    EditOptions::default(),
                )
            })
            .unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(
            f.seq().audio[0].transitions[0].effect,
            catalog::CONSTANT_POWER
        );
        let again = f
            .apply(|p| {
                add_transition(
                    p,
                    seq,
                    TrackRef::video(0),
                    s(5.0),
                    "op.tr.push",
                    d(2.0),
                    None,
                    EditOptions::default(),
                )
            })
            .unwrap();
        assert_eq!(again, ids[0]);
        assert_eq!(f.seq().video[0].transitions.len(), 1);
        assert_eq!(f.seq().video[0].transitions[0].effect, "op.tr.push");
        f.apply(|p| set_transition_timing(p, seq, again, Some(d(2.0)), Some(Alignment::EndAtCut)))
            .unwrap();
        assert_eq!(
            f.seq().video[0].transitions[0].range(),
            SeqRange::new(s(3.0), s(5.0))
        );
    }
}
