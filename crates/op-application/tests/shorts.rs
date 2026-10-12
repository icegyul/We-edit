//! Shorts: a horizontal sequence becomes a vertical one, nested and framed.

use op_application::captions::CaptionOptions;
use op_application::shorts::{ShortsLayout, SHORTS_SIZE};
use op_application::{Dirs, Editor, Preferences};
use op_core::captions::{Cue, TimedWord};
use op_core::*;

fn horizontal_editor(dir: &std::path::Path) -> Editor {
    let mut ed = Editor::new(Dirs::portable(dir), Preferences::default(), false);
    let sid = ed.new_sequence("Talk", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    // something on the timeline to nest: two captions
    let word = |t: &str, a: f64, b: f64| TimedWord { text: t.into(), start: a, end: b };
    let cues = vec![
        Cue { start: 0.0, end: 2.0, words: vec![word("hello", 0.0, 1.0), word("there", 1.0, 2.0)] },
        Cue { start: 2.0, end: 4.0, words: vec![word("again", 2.0, 4.0)] },
    ];
    assert_eq!(ed.create_captions(&cues, &CaptionOptions::default()), 2);
    ed
}

fn motion_scale(c: &Clip) -> f64 {
    match c.components.iter().find(|x| x.effect == op_core::catalog::MOTION).and_then(|m| m.param("scale")).map(|p| &p.value) {
        Some(Value::Float(v)) => *v,
        other => panic!("no scale: {other:?}"),
    }
}

#[test]
fn fill_nests_the_sequence_and_crops_it_to_the_tall_frame() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = horizontal_editor(dir.path());
    let original = ed.active.unwrap();
    let sid = ed.make_shorts(ShortsLayout::Fill, 0.5).unwrap();
    assert_ne!(sid, original);
    assert_eq!(ed.active, Some(sid));
    let seq = ed.active_seq().unwrap();
    assert_eq!((seq.settings.width, seq.settings.height), SHORTS_SIZE);
    assert_eq!(seq.name, "Talk 쇼츠");
    let clips: Vec<&Clip> = seq.video.iter().flat_map(|t| &t.clips).collect();
    assert_eq!(clips.len(), 1);
    assert!(matches!(clips[0].source, ClipSource::Sequence { sequence, .. } if sequence == original));
    assert!(clips[0].scale_to_frame);
    assert!((motion_scale(clips[0]) - 316.0).abs() < 1.0, "{}", motion_scale(clips[0]));
    // the horizontal sequence itself is untouched
    assert_eq!(ed.project.sequence(original).unwrap().settings.width, 1920);
}

#[test]
fn blur_puts_the_whole_picture_over_a_blurred_copy() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = horizontal_editor(dir.path());
    ed.make_shorts(ShortsLayout::Blur, 0.5).unwrap();
    let seq = ed.active_seq().unwrap();
    assert!(seq.video.len() >= 2);
    assert_eq!(seq.video.iter().filter(|t| !t.clips.is_empty()).count(), 2);
    let back = &seq.video[0].clips[0];
    let front = &seq.video[1].clips[0];
    assert!(back.components.iter().any(|c| c.effect == "op.video.gaussian_blur"));
    assert!(!front.components.iter().any(|c| c.effect == "op.video.gaussian_blur"));
    assert!((motion_scale(front) - 100.0).abs() < 1e-9);
    assert!(motion_scale(back) > 300.0);
    assert_ne!(back.id, front.id);
}

#[test]
fn a_vertical_sequence_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = horizontal_editor(dir.path());
    ed.make_shorts(ShortsLayout::Fill, 0.5).unwrap();
    assert!(ed.make_shorts(ShortsLayout::Fill, 0.5).is_err());
}
