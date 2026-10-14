//! Copying the effects of one clip (the ones that were changed: Motion, Opacity, volume, every
//! added effect) and pasting them on any number of other clips at once, from Effect Controls or
//! the timeline's right-click menu. It is Paste Attributes without the questions.

use op_core::catalog::EffectKind;
use op_core::*;
use op_timeline::{Attributes, paste_attributes};

use crate::editor::Editor;

/// Whether a component is an effect: Motion, Opacity and the other fixed ones, or an added
/// effect. What a graphics clip draws (text, shape, caption) is not.
pub fn is_effect(c: &Component) -> bool {
    c.is_fixed() || c.def().is_none_or(|d| matches!(d.kind, EffectKind::VideoEffect | EffectKind::AudioEffect))
}

/// Whether a component differs from a fresh one: changed values or keyframes, switched off, or an
/// added (non-fixed) effect.
pub fn is_modified(c: &Component) -> bool {
    if !c.is_fixed() {
        return is_effect(c);
    }
    let Some(def) = c.def() else { return true };
    let fresh = Component::new(def, &mut IdGen::default());
    !c.enabled || c.params != fresh.params
}

impl Editor {
    /// Copies effects of `clip`: component `only`, or else every modified one; a clip with nothing
    /// modified gives all its effects (their default values: pasted on a clip whose values were
    /// changed they bring it back). Returns their names.
    pub fn copy_effects(&mut self, clip: ClipId, only: Option<ComponentId>) -> Result<Vec<String>, String> {
        let seq = self.active_seq().ok_or("Open a sequence first")?;
        let from = seq.clip(clip).ok_or("클립을 찾을 수 없습니다")?.clone();
        let any_modified = from.components.iter().any(is_modified);
        let picked: Vec<Component> = from
            .components
            .iter()
            .filter(|c| match only {
                Some(id) => c.id == id,
                None if any_modified => is_modified(c),
                None => is_effect(c),
            })
            .cloned()
            .collect();
        if picked.is_empty() {
            return Err("복사할 효과가 없습니다: 이 클립에는 효과가 없습니다".into());
        }
        let names: Vec<String> = picked
            .iter()
            .map(|c| c.def().map_or_else(|| c.effect.clone(), |d| d.name.to_string()))
            .collect();
        let mut copy = from;
        copy.components = picked;
        self.fx_clipboard = Some(copy);
        Ok(names)
    }

    /// Whether effects were copied.
    pub fn has_copied_effects(&self) -> bool {
        self.fx_clipboard.is_some()
    }

    /// Pastes the copied effects on the selected clips (one undo step); fixed effects take the
    /// copied values (keyframes included), added effects are added. Returns how many clips changed.
    pub fn paste_effects(&mut self) -> Result<usize, String> {
        let from = self.fx_clipboard.clone().ok_or("복사한 효과가 없습니다: 효과 컨트롤에서 효과 이름(모션 등)을 오른쪽 클릭 > 이 효과 복사 를 먼저 누르세요")?;
        let seq = self.active_seq().ok_or("Open a sequence first")?;
        let to: Vec<ClipId> = self
            .selection
            .clips
            .iter()
            .copied()
            .filter(|id| *id != from.id && seq.clip(*id).is_some_and(|c| c.kind == from.kind))
            .collect();
        if to.is_empty() {
            return Err(match from.kind {
                TrackKind::Video => "효과를 붙여넣을 영상 클립을 타임라인에서 먼저 선택하세요".into(),
                TrackKind::Audio => "효과를 붙여넣을 오디오 클립을 타임라인에서 먼저 선택하세요".into(),
            });
        }
        let n = to.len();
        self.seq_edit("Paste Effects", |p, sid, opts| paste_attributes(p, sid, &from, &to, Attributes::default(), opts))
            .ok_or("붙여넣을 수 있는 효과가 없습니다")?;
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dirs, Preferences};
    use op_core::catalog;

    fn clip_with(e: &mut Editor, start: f64) -> ClipId {
        // a graphics clip is enough: it has Motion and Opacity like any video clip
        let sid = e.active.unwrap();
        let t = SeqTime::from_seconds(start);
        e.set_playhead(t);
        let id = e.add_graphic(catalog::SHAPE, Some(0)).unwrap();
        let _ = sid;
        id
    }

    fn scale_of(e: &Editor, id: ClipId) -> f64 {
        let c = e.active_seq().unwrap().clip(id).unwrap();
        match c.component(catalog::MOTION).unwrap().param("scale").unwrap().value {
            Value::Float(v) => v,
            ref v => panic!("{v:?}"),
        }
    }

    #[test]
    fn changed_effects_are_copied_and_pasted_on_all_selected_clips_at_once() {
        let _guard = crate::recovery::TEST_GUARD.lock();
        let dir = tempfile::tempdir().unwrap();
        let mut e = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
        e.new_sequence("S", SequenceSettings::default());
        let a = clip_with(&mut e, 0.0);
        let b = clip_with(&mut e, 10.0);
        let c = clip_with(&mut e, 20.0);
        // nothing changed yet: the clip's effects are copied as they are (their default values)
        let defaults = e.copy_effects(a, None).unwrap();
        assert!(defaults.len() >= 2, "{defaults:?}");
        // size and opacity of the first clip are changed
        e.seq_edit("Change", |p, sid, _| {
            let clip = p.sequence_mut(sid).unwrap().clip_mut(a).unwrap();
            for (comp, key, v) in [(catalog::MOTION, "scale", Value::Float(135.0)), (catalog::OPACITY, "opacity", Value::Float(80.0))] {
                clip.component_mut(comp).unwrap().param_mut(key).unwrap().value = v;
            }
            Ok(())
        })
        .unwrap();
        let names = e.copy_effects(a, None).unwrap();
        assert_eq!(names.len(), 2, "{names:?}");
        // paste on the other two, selected together
        e.selection = crate::session::Selection::only(vec![b, c]);
        assert_eq!(e.paste_effects().unwrap(), 2);
        assert_eq!(scale_of(&e, b), 135.0);
        assert_eq!(scale_of(&e, c), 135.0);
        // one undo takes both back
        e.undo();
        assert_eq!(scale_of(&e, b), 100.0);
        assert_eq!(scale_of(&e, c), 100.0);
        // the default values copied from an unchanged clip bring a changed clip back
        e.seq_edit("Change", |p, sid, _| {
            let clip = p.sequence_mut(sid).unwrap().clip_mut(c).unwrap();
            clip.component_mut(catalog::MOTION).unwrap().param_mut("scale").unwrap().value = Value::Float(60.0);
            Ok(())
        })
        .unwrap();
        assert_eq!(scale_of(&e, c), 60.0);
        e.copy_effects(b, None).unwrap();
        e.selection = crate::session::Selection::only(vec![c]);
        assert_eq!(e.paste_effects().unwrap(), 1);
        assert_eq!(scale_of(&e, c), 100.0);
        // pasting with nothing selected says so
        e.selection = crate::session::Selection::default();
        assert!(e.paste_effects().is_err());
    }
}
