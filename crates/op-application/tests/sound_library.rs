//! Putting a library sound on the timeline: it goes on the first audio track that is free.

use op_application::{Dirs, Editor, Preferences};
use op_core::*;
use op_media::{AudioCodec, AudioSettings, Muxer};

fn wav(path: &std::path::Path, seconds: f32) {
    let a = AudioSettings { codec: AudioCodec::Pcm16, rate: 48_000, channels: 2, bitrate_kbps: 0 };
    let mut m = Muxer::create(path, None, Some(&a)).unwrap();
    let n = (48_000.0 * seconds) as usize;
    let samples: Vec<f32> = (0..n * 2).map(|k| ((k / 2) as f32 / 48_000.0 * 440.0 * std::f32::consts::TAU).sin() * 0.3).collect();
    for chunk in samples.chunks(1024 * 2) {
        m.push_audio(chunk).unwrap();
    }
    m.finish().unwrap();
}

#[test]
fn a_sound_goes_on_the_first_free_audio_track() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    let file = dir.path().join("ding.wav");
    wav(&file, 1.0);
    ed.add_sound_file(&file, "딩동").unwrap();
    // the same sound at the same time cannot share the track: it takes the next one
    ed.add_sound_file(&file, "딩동").unwrap();
    let seq = ed.active_seq().unwrap();
    assert_eq!(seq.audio[0].clips.len(), 1);
    assert_eq!(seq.audio[1].clips.len(), 1);
    assert_eq!(seq.audio[0].clips[0].start, SeqTime::ZERO);
    assert!((seq.audio[0].clips[0].duration.seconds() - 1.0).abs() < 0.05);
    // the project panel shows the readable name
    assert!(ed.project.items.values().any(|i| i.name == "딩동"));
    // a sound later in the sequence shares the first track again
    ed.set_playhead(SeqTime::from_seconds(3.0));
    ed.add_sound_file(&file, "딩동").unwrap();
    assert_eq!(ed.active_seq().unwrap().audio[0].clips.len(), 2);
}
