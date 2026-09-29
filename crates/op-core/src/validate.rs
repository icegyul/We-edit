//! Invariants and transactions (DM-005, docs/data-model.md 8).
//!
//! A transaction edits a draft revision and commits only if every invariant holds and no locked
//! track changed; otherwise the previous revision is untouched.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use thiserror::Error;

use crate::ids::*;
use crate::model::*;
use crate::time::*;

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum EditError {
    #[error("track {0} is locked")]
    Locked(String),
    #[error("nothing to do")]
    Nothing,
    #[error("clips would overlap on {0}")]
    Overlap(String),
    #[error("not enough media: {0}")]
    OutOfMedia(String),
    #[error("a sequence cannot contain itself")]
    Recursive,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
}

pub type EditResult<T = ()> = Result<T, EditError>;

impl Project {
    /// Runs `f` on a draft and returns the committed revision, or the error with no change.
    pub fn transact<R>(
        &self,
        f: impl FnOnce(&mut Project) -> EditResult<R>,
    ) -> EditResult<(Project, R)> {
        let mut draft = self.clone();
        let r = f(&mut draft)?;
        draft.normalize();
        draft.validate()?;
        check_locks(self, &draft)?;
        Ok((draft, r))
    }

    /// Repairs derived state after an edit: sorting, removing dangling transitions and links,
    /// dropping empty clips.
    pub fn normalize(&mut self) {
        let ids: Vec<SequenceId> = self.sequences.keys().copied().collect();
        for sid in ids {
            let needs = {
                let seq = self.sequence(sid).unwrap();
                sequence_needs_normalizing(seq)
            };
            if needs {
                normalize_sequence(self.sequence_mut(sid).unwrap());
            }
        }
    }

    /// Checks every invariant.
    pub fn validate(&self) -> EditResult {
        for item in self.items.values() {
            if item.id != self.root {
                let parent = item.parent.and_then(|p| self.item(p));
                match parent {
                    Some(p) if p.is_bin() => {}
                    _ => return Err(EditError::Invalid(format!("{} has no bin", item.name))),
                }
            }
            match &item.kind {
                ItemKind::Media { asset, .. } if self.asset(*asset).is_none() => {
                    return Err(EditError::NotFound(format!("media of {}", item.name)));
                }
                ItemKind::Sequence { sequence } if self.sequence(*sequence).is_none() => {
                    return Err(EditError::NotFound(format!("sequence of {}", item.name)));
                }
                _ => {}
            }
        }
        for seq in self.sequences.values() {
            validate_sequence(self, seq)?;
            if seq.nested().any(|n| self.nests(n, seq.id)) {
                return Err(EditError::Recursive);
            }
        }
        Ok(())
    }
}

fn sequence_needs_normalizing(seq: &Sequence) -> bool {
    for (_, t) in seq.all_tracks() {
        if t.clips
            .windows(2)
            .any(|w| (w[0].start, w[0].id) > (w[1].start, w[1].id))
        {
            return true;
        }
        if t.clips.iter().any(|c| c.duration.0 <= 0) {
            return true;
        }
        for tr in &t.transitions {
            if !transition_attached(t, tr) {
                return true;
            }
        }
    }
    let mut links: HashMap<LinkId, usize> = HashMap::new();
    let mut groups: HashMap<GroupId, usize> = HashMap::new();
    for (_, c) in seq.clips() {
        if let Some(l) = c.link {
            *links.entry(l).or_default() += 1;
        }
        if let Some(g) = c.group {
            *groups.entry(g).or_default() += 1;
        }
    }
    links.values().any(|n| *n < 2) || groups.values().any(|n| *n < 2)
}

/// Whether a transition still sits on an edit point of its clips.
fn transition_attached(t: &Track, tr: &Transition) -> bool {
    if tr.duration.0 <= 0 {
        return false;
    }
    let from = tr.from.map(|id| t.clip(id));
    let to = tr.to.map(|id| t.clip(id));
    match (from, to) {
        (None, None) => false,
        (Some(None), _) | (_, Some(None)) => false,
        (Some(Some(a)), Some(Some(b))) => {
            a.end() == tr.cut
                && b.start == tr.cut
                && tr.before_cut() <= a.duration
                && tr.duration - tr.before_cut() <= b.duration
        }
        (Some(Some(a)), None) => a.end() == tr.cut && tr.range().start >= a.start,
        (None, Some(Some(b))) => b.start == tr.cut && tr.range().end <= b.end(),
    }
}

fn normalize_sequence(seq: &mut Sequence) {
    for kind in [TrackKind::Video, TrackKind::Audio] {
        for t in seq.tracks_mut(kind) {
            let t = Arc::make_mut(t);
            t.clips.retain(|c| c.duration.0 > 0);
            t.sort();
            // a transition whose neighbor disappeared becomes one-sided when the other clip still
            // touches the cut; otherwise it goes
            let clips = t.clips.clone();
            let find = |id: Option<ClipId>| id.and_then(|i| clips.iter().find(|c| c.id == i));
            t.transitions.retain_mut(|tr| {
                let a = find(tr.from).filter(|a| a.end() == tr.cut);
                let b = find(tr.to).filter(|b| b.start == tr.cut);
                tr.from = a.map(|c| c.id);
                tr.to = b.map(|c| c.id);
                match (a, b) {
                    (None, None) => false,
                    (Some(a), None) => {
                        tr.alignment = Alignment::EndAtCut;
                        tr.duration = tr.duration.min(a.duration);
                        tr.duration.0 > 0
                    }
                    (None, Some(b)) => {
                        tr.alignment = Alignment::StartAtCut;
                        tr.duration = tr.duration.min(b.duration);
                        tr.duration.0 > 0
                    }
                    (Some(a), Some(b)) => {
                        // keep inside both clips
                        let before = tr.before_cut().min(a.duration);
                        let after = (tr.duration - tr.before_cut()).min(b.duration);
                        if before + after != tr.duration {
                            tr.duration = before + after;
                            tr.alignment = Alignment::Custom(before);
                        }
                        tr.duration.0 > 0
                    }
                }
            });
        }
    }
    // links and groups with a single member dissolve
    let mut links: HashMap<LinkId, usize> = HashMap::new();
    let mut groups: HashMap<GroupId, usize> = HashMap::new();
    for (_, c) in seq.clips() {
        if let Some(l) = c.link {
            *links.entry(l).or_default() += 1;
        }
        if let Some(g) = c.group {
            *groups.entry(g).or_default() += 1;
        }
    }
    let lonely_links: HashSet<LinkId> = links
        .into_iter()
        .filter(|(_, n)| *n < 2)
        .map(|(l, _)| l)
        .collect();
    let lonely_groups: HashSet<GroupId> = groups
        .into_iter()
        .filter(|(_, n)| *n < 2)
        .map(|(g, _)| g)
        .collect();
    if lonely_links.is_empty() && lonely_groups.is_empty() {
        return;
    }
    for kind in [TrackKind::Video, TrackKind::Audio] {
        for t in seq.tracks_mut(kind) {
            let touches = t.clips.iter().any(|c| {
                c.link.is_some_and(|l| lonely_links.contains(&l))
                    || c.group.is_some_and(|g| lonely_groups.contains(&g))
            });
            if touches {
                for c in &mut Arc::make_mut(t).clips {
                    if c.link.is_some_and(|l| lonely_links.contains(&l)) {
                        c.link = None;
                    }
                    if c.group.is_some_and(|g| lonely_groups.contains(&g)) {
                        c.group = None;
                    }
                }
            }
        }
    }
}

fn validate_sequence(p: &Project, seq: &Sequence) -> EditResult {
    let mut seen = HashSet::new();
    for (r, t) in seq.all_tracks() {
        let name = seq.track_name(r);
        for c in &t.clips {
            if !seen.insert(c.id) {
                return Err(EditError::Invalid(format!("clip {} appears twice", c.id)));
            }
            if c.kind != t.kind {
                return Err(EditError::Invalid(format!(
                    "{} clip on {name}",
                    if c.is_video() { "video" } else { "audio" }
                )));
            }
            if c.duration.0 <= 0 {
                return Err(EditError::Invalid("empty clip".into()));
            }
            if c.start < SeqTime::ZERO {
                return Err(EditError::Invalid("clip before the sequence start".into()));
            }
            if let Some(avail) = p.available(&c.source)
                && c.hold.is_none()
            {
                let used = c.source_range();
                // one tick of tolerance absorbs rounding from speed changes
                if used.start < avail.start - Dur(1) || used.end > avail.end + Dur(1) {
                    return Err(EditError::OutOfMedia(c.name.clone()));
                }
            }
        }
        for w in t.clips.windows(2) {
            if w[0].end() > w[1].start {
                return Err(EditError::Overlap(name));
            }
        }
    }
    Ok(())
}

/// Refuses revisions that change the contents of a track locked in both revisions.
fn check_locks(before: &Project, after: &Project) -> EditResult {
    for (sid, seq) in &after.sequences {
        let Some(old) = before.sequences.get(sid) else {
            continue;
        };
        if Arc::ptr_eq(old, seq) {
            continue;
        }
        for kind in [TrackKind::Video, TrackKind::Audio] {
            for (i, t) in seq.tracks(kind).iter().enumerate() {
                let Some(o) = old.tracks(kind).iter().find(|o| o.id == t.id) else {
                    continue;
                };
                if Arc::ptr_eq(o, t) || !(o.locked && t.locked) {
                    continue;
                }
                if o.clips != t.clips || o.transitions != t.transitions {
                    return Err(EditError::Locked(
                        seq.track_name(TrackRef { kind, index: i }),
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::EffectKind;
    use crate::color::Label;

    pub fn clip(ids: &mut IdGen, start: f64, dur: f64) -> Clip {
        Clip {
            id: ids.clip(),
            name: "c".into(),
            kind: TrackKind::Video,
            source: ClipSource::Graphic,
            start: SeqTime::from_seconds(start),
            duration: Dur::from_seconds(dur),
            source_in: SrcTime::ZERO,
            speed: Speed::NORMAL,
            reverse: false,
            hold: None,
            enabled: true,
            link: None,
            group: None,
            label: Label::None,
            components: default_components(EffectKind::VideoFixed, ids),
            gain_db: 0.0,
            scale_to_frame: false,
            channels: None,
        }
    }

    #[test]
    fn overlaps_are_rejected_and_nothing_changes() {
        let mut p = Project::new("t");
        let (sid, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        let r = p.transact(|d| {
            let mut ids = d.ids.clone();
            let a = clip(&mut ids, 0.0, 5.0);
            let b = clip(&mut ids, 4.0, 5.0);
            d.ids = ids;
            let t = d
                .sequence_mut(sid)
                .unwrap()
                .track_mut(TrackRef::video(0))
                .unwrap();
            t.clips.push(a);
            t.clips.push(b);
            Ok(())
        });
        assert!(matches!(r, Err(EditError::Overlap(_))));
        assert!(p.sequence(sid).unwrap().video[0].clips.is_empty());
    }

    #[test]
    fn locked_tracks_refuse_edits() {
        let mut p = Project::new("t");
        let (sid, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        p.sequence_mut(sid)
            .unwrap()
            .track_mut(TrackRef::video(0))
            .unwrap()
            .locked = true;
        let r = p.transact(|d| {
            let mut ids = d.ids.clone();
            let c = clip(&mut ids, 0.0, 1.0);
            d.sequence_mut(sid)
                .unwrap()
                .track_mut(TrackRef::video(0))
                .unwrap()
                .clips
                .push(c);
            Ok(())
        });
        assert!(matches!(r, Err(EditError::Locked(_))));
    }

    #[test]
    fn dangling_transitions_and_single_links_are_cleaned() {
        let mut p = Project::new("t");
        let (sid, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
        let (next, _) = p
            .transact(|d| {
                let mut ids = d.ids.clone();
                let mut a = clip(&mut ids, 0.0, 5.0);
                let b = clip(&mut ids, 5.0, 5.0);
                a.link = Some(ids.link());
                let tr = Transition {
                    id: ids.transition(),
                    effect: crate::catalog::CROSS_DISSOLVE.into(),
                    cut: SeqTime::from_seconds(5.0),
                    duration: Dur::from_seconds(1.0),
                    alignment: Alignment::CenterAtCut,
                    from: Some(a.id),
                    to: Some(b.id),
                    params: vec![],
                };
                d.ids = ids;
                let t = d
                    .sequence_mut(sid)
                    .unwrap()
                    .track_mut(TrackRef::video(0))
                    .unwrap();
                t.clips = vec![a, b.clone()];
                t.transitions.push(tr);
                // remove the incoming clip: the transition becomes one-sided on `a`
                t.clips.retain(|c| c.id != b.id);
                Ok(())
            })
            .unwrap();
        let t = &next.sequence(sid).unwrap().video[0];
        assert_eq!(t.transitions.len(), 1);
        assert_eq!(t.transitions[0].alignment, Alignment::EndAtCut);
        assert!(t.clips[0].link.is_none());
    }
}
