//! Remix of a music clip on the timeline: analysis, a plan for a length, the pieces with their
//! crossfades, revert and undo.

use std::time::{Duration, Instant};

use op_application::{Dirs, Editor, Focus, Preferences};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, Muxer};

const RATE: usize = 44_100;

/// Music-like sound: a bar is 2 s (120 bpm) with a kick on every beat and a chord; four chords make
/// a phrase that repeats `bars / 4` times.
fn write_music(path: &std::path::Path, bars: usize) {
    let chords: [&[f32]; 4] = [
        &[220.0, 277.18, 329.63],
        &[146.83, 220.0, 293.66, 369.99],
        &[164.81, 207.65, 246.94, 329.63],
        &[174.61, 220.0, 261.63, 349.23],
    ];
    let a = AudioSettings { codec: AudioCodec::Pcm16, rate: RATE as u32, channels: 2, bitrate_kbps: 0 };
    let mut m = Muxer::create(path, None, Some(&a)).unwrap();
    let bar = RATE * 2;
    let mut samples = Vec::with_capacity(bars * bar * 2);
    for b in 0..bars {
        for i in 0..bar {
            let t = i as f32 / RATE as f32;
            let mut v = 0.0;
            for f in chords[b % 4] {
                v += (t * f * std::f32::consts::TAU).sin() * 0.12 * (1.0 - (t % 0.5) * 0.6);
            }
            let into = i % (RATE / 2);
            if into < 4000 {
                let k = into as f32 / RATE as f32;
                v += (k * (150.0 - 700.0 * k) * std::f32::consts::TAU).sin() * 0.6 * (1.0 - into as f32 / 4000.0);
            }
            samples.push(v);
            samples.push(v);
        }
    }
    for chunk in samples.chunks(2048 * 2) {
        m.push_audio(chunk).unwrap();
    }
    m.finish().unwrap();
}

fn editor_with_music(dir: &std::path::Path, bars: usize) -> (Editor, ClipId) {
    let file = dir.join("music.wav");
    write_music(&file, bars);
    let mut ed = Editor::new(Dirs::portable(dir), Preferences::default(), false);
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    let root = ed.project.root;
    let probed = op_media::probe(&file).unwrap();
    let (_, item) = ed.edit("Import", |p| Ok(p.add_asset(root, probed))).unwrap();
    ed.load_source(item);
    ed.set_playhead(SeqTime::ZERO);
    ed.execute("cmd.clip.overlay", Focus::Timeline);
    let clip = ed.active_seq().unwrap().audio[0].clips[0].id;
    (ed, clip)
}

fn wait_for_analysis(ed: &mut Editor) {
    let start = Instant::now();
    while ed.remix.job.is_some() && start.elapsed() < Duration::from_secs(120) {
        ed.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ed.remix.job.is_none(), "the analysis did not finish");
}

fn audio_clips(ed: &Editor) -> Vec<Clip> {
    ed.active_seq().unwrap().audio[0].clips.clone()
}

#[test]
fn music_is_made_longer_in_whole_phrases_and_back() {
    let dir = tempfile::tempdir().unwrap();
    // 20 bars = 40 s, a phrase of four bars (8 s) five times
    let (mut ed, clip) = editor_with_music(dir.path(), 20);
    let original = audio_clips(&ed)[0].duration.seconds();
    assert!((original - 40.0).abs() < 0.05, "{original}");

    ed.remix_analyze(clip).unwrap();
    wait_for_analysis(&mut ed);
    assert!(ed.remix_ready_for(clip));
    let a = ed.remix_analysis().unwrap();
    assert!((a.bpm() - 120.0).abs() < 4.0, "{}", a.bpm());

    // 58 s is closest to one more phrase of 16 s (56 s) or two of 8 s: whole phrases only
    let plan = ed.remix_plan(clip, 58.0, 4.0).unwrap();
    assert!(!plan.moves.is_empty());
    let steps = (plan.duration - 40.0) / 8.0;
    assert!((steps - steps.round()).abs() < 0.05, "the length grows in phrases: {}", plan.duration);
    assert!(plan.duration > 50.0, "{}", plan.duration);

    let new_len = ed.remix_apply(clip, &plan).unwrap();
    assert!((new_len - plan.duration).abs() < 1e-6);
    let clips = audio_clips(&ed);
    assert!(clips.len() >= 2, "{}", clips.len());
    // the pieces follow each other without gaps, share a group, and add up to the plan
    for w in clips.windows(2) {
        assert_eq!(w[0].end(), w[1].start);
        assert_eq!(w[0].group, w[1].group);
    }
    assert!(clips[0].group.is_some());
    let total: f64 = clips.iter().map(|c| c.duration.seconds()).sum();
    assert!((total - plan.duration).abs() < 0.01, "{total} vs {}", plan.duration);
    // a short crossfade at every join
    assert_eq!(ed.active_seq().unwrap().audio[0].transitions.len(), clips.len() - 1);
    assert!(ed.is_remixed(clips[0].id));
    // the first piece starts where the music did and the last ends at the music's end
    assert_eq!(clips[0].source_in, SrcTime::ZERO);
    let last = clips.last().unwrap();
    assert!(((last.source_in + last.source_duration()).seconds() - 40.0).abs() < 0.05);

    // revert: one clip again, as long as the original
    let any = clips[1].id;
    ed.remix_revert(any).unwrap();
    let back = audio_clips(&ed);
    assert_eq!(back.len(), 1);
    assert!((back[0].duration.seconds() - original).abs() < 0.01);
    assert_eq!(back[0].group, None);
    assert!(ed.active_seq().unwrap().audio[0].transitions.is_empty());
    assert!(!ed.project.remixes.iter().any(|_| true), "{:?}", ed.project.remixes);

    // undo of the revert brings the remix back, one more undo the original
    ed.undo();
    assert_eq!(audio_clips(&ed).len(), clips.len());
    ed.undo();
    assert_eq!(audio_clips(&ed).len(), 1);
}

#[test]
fn music_is_made_shorter_by_skipping_a_phrase() {
    let dir = tempfile::tempdir().unwrap();
    let (mut ed, clip) = editor_with_music(dir.path(), 20);
    ed.remix_analyze(clip).unwrap();
    wait_for_analysis(&mut ed);
    let plan = ed.remix_plan(clip, 30.0, 4.0).unwrap();
    assert!(plan.duration < 36.0, "{}", plan.duration);
    ed.remix_apply(clip, &plan).unwrap();
    let clips = audio_clips(&ed);
    let total: f64 = clips.iter().map(|c| c.duration.seconds()).sum();
    assert!((total - plan.duration).abs() < 0.01);
    // the remixed music can be remixed again from the original: the target applies to the original
    let again = ed.remix_plan(clips[0].id, 40.0, 4.0).unwrap();
    assert_eq!(again.original, 40.0f64.max(again.original).min(again.original));
    assert!((again.original - 40.0).abs() < 0.05, "{}", again.original);
}

#[test]
fn a_clip_that_cannot_be_remixed_says_why() {
    let dir = tempfile::tempdir().unwrap();
    // too short to find repeats
    let (mut ed, clip) = editor_with_music(dir.path(), 3);
    ed.remix_analyze(clip).unwrap();
    wait_for_analysis(&mut ed);
    assert!(!ed.remix_ready_for(clip));
    // a longer remix needs the room: another clip right behind the music blocks it
    let dir2 = tempfile::tempdir().unwrap();
    let (mut ed2, clip2) = editor_with_music(dir2.path(), 20);
    ed2.remix_analyze(clip2).unwrap();
    wait_for_analysis(&mut ed2);
    let plan = ed2.remix_plan(clip2, 60.0, 4.0).unwrap();
    // put a second clip of the same music directly after the first
    ed2.set_playhead(SeqTime::from_seconds(40.0));
    ed2.execute("cmd.clip.overlay", Focus::Timeline);
    let err = ed2.remix_apply(clip2, &plan).err().expect("blocked by the next clip");
    assert!(err.contains("뒤에 다른 클립"), "{err}");
    assert_eq!(audio_clips(&ed2).len(), 2, "nothing changed");
}
