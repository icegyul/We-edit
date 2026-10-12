//! Motion templates land as one group of animated graphics clips on free tracks.

use op_application::motion_templates::{TemplateOptions, find};
use op_application::{Dirs, Editor, Preferences};
use op_core::*;

fn opts(id: &str) -> TemplateOptions {
    let t = find(id).unwrap();
    TemplateOptions { text: "홍길동".into(), sub: "대표".into(), accent: t.accent, seconds: t.seconds }
}

#[test]
fn a_template_is_a_group_of_animated_clips_on_free_tracks() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    assert_eq!(ed.add_motion_template("lower_third", &opts("lower_third")).unwrap(), 3);
    let seq = ed.active_seq().unwrap();
    let clips: Vec<&Clip> = seq.video.iter().flat_map(|t| &t.clips).collect();
    assert_eq!(clips.len(), 3);
    // one group, one clip per track
    assert!(clips.iter().all(|c| c.group == clips[0].group && c.group.is_some()));
    assert_eq!(seq.video.iter().filter(|t| !t.clips.is_empty()).count(), 3);
    // the bar slides in: position has keyframes
    let motion = clips[0].components.iter().find(|c| c.effect == op_core::catalog::MOTION).unwrap();
    let position = motion.param("position").unwrap();
    assert!(position.animated && position.keys.len() >= 3);
    // the text clip carries the text
    assert!(clips.iter().any(|c| c.components.iter().any(|x| matches!(x.param("text").map(|p| &p.value), Some(Value::Text(t)) if t == "홍길동"))));
    // selecting is done for the user: all three layers
    assert_eq!(ed.selection.clips.len(), 3);
    // the same template at the same time goes above the first one
    ed.add_motion_template("lower_third", &opts("lower_third")).unwrap();
    assert_eq!(ed.active_seq().unwrap().video.iter().filter(|t| !t.clips.is_empty()).count(), 6);
}

#[test]
fn text_is_required_for_templates_that_show_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    let mut o = opts("pop_title");
    o.text = "  ".into();
    assert!(ed.add_motion_template("pop_title", &o).is_err());
    // the flash has no text
    o.text.clear();
    assert_eq!(ed.add_motion_template("flash", &o).unwrap(), 1);
    assert!(ed.add_motion_template("nope", &o).is_err());
}
