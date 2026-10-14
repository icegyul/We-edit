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

    /// The component of `c` that is the effect `comp` of the clip `shown`: that very one on the
    /// shown clip, the first of the same effect on the others.
    fn same_effect<'a>(c: &'a mut Clip, shown: ClipId, comp: ComponentId, effect: &str) -> Option<&'a mut Component> {
        if c.id == shown {
            c.components.iter_mut().find(|x| x.id == comp)
        } else {
            c.components.iter_mut().find(|x| x.effect == effect)
        }
    }

    /// Removes an added effect from the clip it is shown on and from the same effect of the other
    /// `clips` (one undo step). Returns how many clips lost it.
    pub fn remove_effect(&mut self, shown: ClipId, comp: ComponentId, clips: &[ClipId]) -> usize {
        let Some(effect) = self.active_seq().and_then(|q| q.clip(shown)).and_then(|c| c.components.iter().find(|x| x.id == comp)).map(|x| x.effect.clone()) else {
            return 0;
        };
        let clips = clips.to_vec();
        let mut removed = 0;
        self.seq_edit("Remove Effect", |p, sid, _| {
            for id in &clips {
                let Some(c) = p.sequence_mut(sid).and_then(|q| q.clip_mut(*id)) else { continue };
                let at = if c.id == shown {
                    c.components.iter().position(|x| x.id == comp)
                } else {
                    c.components.iter().position(|x| x.effect == effect && !x.is_fixed())
                };
                if let Some(i) = at.filter(|i| !c.components[*i].is_fixed()) {
                    c.components.remove(i);
                    removed += 1;
                }
            }
            if removed == 0 { Err(EditError::Nothing) } else { Ok(()) }
        });
        removed
    }

    /// Gives the effect its starting values on the clip it is shown on and on the same effect of
    /// the other `clips` (one undo step). Returns how many clips changed.
    pub fn reset_effect(&mut self, shown: ClipId, comp: ComponentId, clips: &[ClipId]) -> usize {
        let Some(effect) = self.active_seq().and_then(|q| q.clip(shown)).and_then(|c| c.components.iter().find(|x| x.id == comp)).map(|x| x.effect.clone()) else {
            return 0;
        };
        let clips = clips.to_vec();
        let mut done = 0;
        self.seq_edit("Reset Effect", |p, sid, _| {
            for id in &clips {
                let Some(c) = p.sequence_mut(sid).and_then(|q| q.clip_mut(*id)) else { continue };
                let Some(x) = Self::same_effect(c, shown, comp, &effect) else { continue };
                let Some(params) = x.def().map(|d| d.params.iter().map(|sp| Param::new(sp.key, sp.default_value())).collect::<Vec<_>>()) else {
                    continue;
                };
                x.params = params;
                done += 1;
            }
            if done == 0 { Err(EditError::Nothing) } else { Ok(()) }
        });
        done
    }

    /// Switches the effect on or off on the clip it is shown on and on the same effect of the
    /// other `clips` (one undo step). Returns how many clips changed.
    pub fn set_effect_enabled(&mut self, shown: ClipId, comp: ComponentId, clips: &[ClipId], on: bool) -> usize {
        let Some(effect) = self.active_seq().and_then(|q| q.clip(shown)).and_then(|c| c.components.iter().find(|x| x.id == comp)).map(|x| x.effect.clone()) else {
            return 0;
        };
        let clips = clips.to_vec();
        let mut done = 0;
        self.seq_edit("Toggle Effect", |p, sid, _| {
            for id in &clips {
                let Some(c) = p.sequence_mut(sid).and_then(|q| q.clip_mut(*id)) else { continue };
                if let Some(x) = Self::same_effect(c, shown, comp, &effect) {
                    x.enabled = on;
                    done += 1;
                }
            }
            if done == 0 { Err(EditError::Nothing) } else { Ok(()) }
        });
        done
    }

    /// Puts the default transition (a dissolve, an audio crossfade) on the cut that ends `clip`
    /// and, with `linked`, on the cuts of the clips linked to it. Returns how many were added.
    pub fn add_dissolve_at_cut(&mut self, clip: ClipId, linked: bool, only: Option<TrackKind>) -> usize {
        let made = self
            .seq_edit("Add Transition", |p, sid, opts| op_timeline::apply_default_at_cut(p, sid, clip, linked, only, opts))
            .map_or(0, |v| v.len());
        if made == 0 {
            self.error("이 컷에는 전환을 넣을 수 없습니다: 두 클립이 붙어 있어야 하고 앞뒤 클립에 여분 프레임이 있어야 합니다");
        } else {
            self.info(format!("컷에 전환 {made}개를 넣었습니다"));
        }
        made
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
        // an added effect is removed from every selected clip at once (one undo step)
        let blur = catalog::CATALOG.iter().find(|d| d.kind == catalog::EffectKind::VideoEffect).unwrap();
        e.seq_edit("Add", |p, sid, _| {
            let mut ids = p.ids.clone();
            for id in [a, b, c] {
                let clip = p.sequence_mut(sid).unwrap().clip_mut(id).unwrap();
                clip.components.push(Component::new(blur, &mut ids));
            }
            p.ids = ids;
            Ok(())
        })
        .unwrap();
        let comp_of = |e: &Editor, id: ClipId| e.active_seq().unwrap().clip(id).unwrap().components.iter().find(|x| x.effect == blur.id).map(|x| x.id);
        let shown = comp_of(&e, a).unwrap();
        assert_eq!(e.remove_effect(a, shown, &[a, b, c]), 3);
        for id in [a, b, c] {
            assert!(comp_of(&e, id).is_none(), "{id:?}");
        }
        e.undo();
        for id in [a, b, c] {
            assert!(comp_of(&e, id).is_some(), "{id:?}");
        }
        // the same for reset (the values go back) and on / off
        let shown = comp_of(&e, a).unwrap();
        assert_eq!(e.set_effect_enabled(a, shown, &[a, b, c], false), 3);
        assert!(e.active_seq().unwrap().clip(c).unwrap().components.iter().all(|x| x.effect != blur.id || !x.enabled));
        // pasting with nothing selected says so
        e.selection = crate::session::Selection::default();
        assert!(e.paste_effects().is_err());
    }
}
