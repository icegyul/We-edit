//! Silence removal end to end: a recording with a pause is analyzed, the pause is cut out of
//! the timeline (audio and linked picture together) in one undo step, sped up, or marked.

use std::time::{Duration, Instant};

use op_application::silence::{Action, Scope, SilenceSettings};
use op_application::{Dirs, Editor, Focus, Preferences};
use op_core::*;
use op_silence as sil;

/// A 48 kHz mono 16-bit WAV: 3 s of a voice-like tone, 2 s of quiet hiss, 3 s of tone.
fn recording(path: &std::path::Path) {
    let rate = 48_000u32;
    let mut samples: Vec<i16> = Vec::new();
    let mut x = 7u32;
    for part in 0..3 {
        let secs = if part == 1 { 2 } else { 3 };
        for i in 0..rate * secs {
            let t = i as f32 / rate as f32;
            let v = if part == 1 {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                ((x >> 8) as f32 / (1u32 << 24) as f32 - 0.5) * 0.002
            } else {
                let tone: f32 = (1..=8)
                    .map(|h| (t * 180.0 * h as f32 * std::f32::consts::TAU).sin() / h as f32)
                    .sum();
                tone * 0.12 * (0.7 + 0.3 * (t * 5.0 * std::f32::consts::TAU).sin())
            };
            samples.push((v * 32767.0) as i16);
        }
    }
    let n = samples.len() as u32;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n * 2).to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

fn analyzed(ed: &mut Editor, s: &SilenceSettings) {
    ed.silence_result = None;
    ed.analyze_silence(s).unwrap();
    let start = Instant::now();
    while ed.silencing.is_some() && start.elapsed() < Duration::from_secs(60) {
        ed.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ed.silence_result.is_some(), "the analysis finished");
}

#[test]
fn a_pause_is_cut_sped_up_or_marked() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let wav = dir.path().join("talk.wav");
    recording(&wav);
    let root = ed.project.root;
    let probed = op_media::probe(&wav).unwrap();
    let (_, item) = ed
        .edit("Import", |p| Ok(p.add_asset(root, probed)))
        .unwrap();
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    ed.load_source(item);
    ed.execute("cmd.clip.overlay", Focus::Timeline);
    let length = |ed: &Editor| ed.active_seq().unwrap().duration().seconds();
    assert!((length(&ed) - 8.0).abs() < 0.1);

    let mut s = SilenceSettings {
        scope: Scope::Sequence,
        smooth_ms: 0.0,
        ..Default::default()
    };
    s.sound.method = sil::Method::Adaptive { margin_db: 9.0 };
    s.sound.min_silence = 0.5;
    s.sound.pad_before = 0.1;
    s.sound.pad_after = 0.1;
    analyzed(&mut ed, &s);
    let found = ed.silence_result.clone().unwrap();
    let cuts = found.cuts(&s);
    assert_eq!(cuts.len(), 1, "{cuts:?}");
    // 3.0 .. 5.0 s of pause, 0.1 s of air kept at each side
    assert!(
        (cuts[0].range.start - 3.1).abs() < 0.08 && (cuts[0].range.end - 4.9).abs() < 0.08,
        "{cuts:?}"
    );
    let ranges: Vec<sil::Range> = cuts.iter().map(|c| c.range).collect();

    // marked
    let history = ed.history.labels().1;
    assert_eq!(
        ed.apply_cuts(found.origin, &ranges, Action::Markers, 0.0),
        1
    );
    assert_eq!(ed.active_seq().unwrap().markers.len(), 1);
    assert_eq!(ed.history.labels().1, history + 1);
    ed.undo();

    // sped up: the same length of pause plays four times faster
    assert_eq!(
        ed.apply_cuts(found.origin, &ranges, Action::SpeedUp(400.0), 0.0),
        1
    );
    let sped = length(&ed);
    assert!((sped - (8.0 - 1.8 * 0.75)).abs() < 0.15, "{sped}");
    ed.undo();
    assert!((length(&ed) - 8.0).abs() < 0.1);

    // cut out and closed: one undo step, 1.8 s shorter
    let history = ed.history.labels().1;
    assert_eq!(ed.apply_cuts(found.origin, &ranges, Action::Remove, 0.0), 1);
    assert!((length(&ed) - 6.2).abs() < 0.1, "{}", length(&ed));
    assert_eq!(ed.history.labels().1, history + 1);
    let clips: usize = ed
        .active_seq()
        .unwrap()
        .audio
        .iter()
        .map(|t| t.clips.len())
        .sum();
    assert_eq!(clips, 2, "the clip was split in two");
    ed.undo();
    assert!((length(&ed) - 8.0).abs() < 0.1);

    // with a crossfade where the audio joins
    assert_eq!(
        ed.apply_cuts(found.origin, &ranges, Action::Remove, 15.0),
        1
    );
    assert!((length(&ed) - 6.2).abs() < 0.1);
    let transitions: usize = ed
        .active_seq()
        .unwrap()
        .audio
        .iter()
        .map(|t| t.transitions.len())
        .sum();
    assert_eq!(transitions, 1, "an audio crossfade at the join");
}

#[test]
fn only_the_selected_clip_is_heard() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let wav = dir.path().join("talk.wav");
    recording(&wav);
    let root = ed.project.root;
    let probed = op_media::probe(&wav).unwrap();
    let (_, item) = ed
        .edit("Import", |p| Ok(p.add_asset(root, probed)))
        .unwrap();
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    ed.load_source(item);
    ed.execute("cmd.clip.overlay", Focus::Timeline);
    // nothing selected: the selection scope asks for clips
    ed.selection.clear();
    assert!(ed.analyze_silence(&SilenceSettings::default()).is_err());
    let clip = ed.active_seq().unwrap().audio[0].clips[0].id;
    ed.selection = op_application::Selection::only(vec![clip]);
    let (clips, range) = ed.silence_targets(Scope::Selection).unwrap();
    assert_eq!(clips, vec![clip]);
    assert!((range.duration().seconds() - 8.0).abs() < 0.1);
}

#[test]
fn a_file_is_cut_and_written() {
    let Ok(gpu) = op_render::Gpu::headless() else {
        eprintln!("no GPU adapter, skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("talk.wav");
    recording(&wav);
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    ed.background = true;
    let mut s = SilenceSettings::default();
    s.sound.min_silence = 0.5;
    s.sound.pad_before = 0.1;
    s.sound.pad_after = 0.1;
    s.smooth_ms = 0.0;
    let out = dir.path().join("out").join("talk_cut.wav");
    let done = op_application::silence::cut_file(
        &mut ed,
        &gpu,
        &wav,
        &out,
        &s,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(done.count, 1, "{done:?}");
    assert!(
        (done.before - 8.0).abs() < 0.1 && (done.after - 6.2).abs() < 0.15,
        "{done:?}"
    );
    let probed = op_media::probe(&out).unwrap();
    assert!(
        (probed.duration.seconds() - 6.2).abs() < 0.2,
        "{}",
        probed.duration.seconds()
    );
}
