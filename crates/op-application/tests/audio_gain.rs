//! Audio Gain end to end: the peak of a clip is read from its waveform, and Normalize Max Peak
//! and Normalize All Peaks set the gain from it.

use std::time::{Duration, Instant};

use op_application::{Dirs, Editor, Focus, Preferences};
use op_core::*;

/// A 48 kHz mono 16-bit WAV of a 440 Hz tone at `amp` (0..1).
fn tone(path: &std::path::Path, amp: f32, secs: f32) {
    let n = (48_000.0 * secs) as usize;
    let mut b = Vec::with_capacity(44 + n * 2);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + n as u32 * 2).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&96_000u32.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(n as u32 * 2).to_le_bytes());
    for i in 0..n {
        let v = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * amp;
        b.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

#[test]
fn normalize_sets_the_gain_from_the_peaks() {
    let dir = tempfile::tempdir().unwrap();
    let mut ed = Editor::new(Dirs::portable(dir.path()), Preferences::default(), false);
    let sid = ed.new_sequence("S", SequenceSettings::default()).unwrap();
    ed.open_sequence(sid);
    let root = ed.project.root;
    let mut clips = Vec::new();
    // a loud tone (-6 dB) and a quiet one (-20 dB), one after the other
    for (name, amp) in [("loud.wav", 0.5f32), ("quiet.wav", 0.1)] {
        let path = dir.path().join(name);
        tone(&path, amp, 2.0);
        let probed = op_media::probe(&path).unwrap();
        let (_, item) = ed
            .edit("Import", |p| Ok(p.add_asset(root, probed)))
            .unwrap();
        ed.load_source(item);
        let end = ed.active_seq().unwrap().duration();
        ed.set_playhead(SeqTime::ZERO + end);
        ed.execute("cmd.clip.overlay", Focus::Timeline);
    }
    let seq = ed.active_seq().unwrap();
    for t in &seq.audio {
        clips.extend(t.clips.iter().map(|c| c.id));
    }
    assert_eq!(clips.len(), 2);
    ed.selection = op_application::Selection::only(clips.clone());
    // the waveforms are made in the background
    let start = Instant::now();
    while ed
        .selected_audio_peaks()
        .iter()
        .any(|(_, _, p)| p.is_none())
        && start.elapsed() < Duration::from_secs(60)
    {
        ed.tick();
        std::thread::sleep(Duration::from_millis(20));
    }
    let peaks: Vec<f32> = ed
        .selected_audio_peaks()
        .iter()
        .map(|(_, _, p)| p.unwrap())
        .collect();
    assert!((peaks[0] - 0.5).abs() < 0.01, "{peaks:?}");
    assert!((peaks[1] - 0.1).abs() < 0.01, "{peaks:?}");
    let gain = |ed: &Editor, id: ClipId| ed.active_seq().unwrap().clip(id).unwrap().gain_db;

    // one gain for both: the loud clip's peak reaches -1 dB, the quiet one keeps its distance
    ed.normalize_gain(-1.0, false);
    assert!(
        (gain(&ed, clips[0]) - 5.02).abs() < 0.1,
        "{}",
        gain(&ed, clips[0])
    );
    assert!((gain(&ed, clips[1]) - gain(&ed, clips[0])).abs() < 1e-9);

    // each its own gain: both peaks reach -1 dB
    ed.normalize_gain(-1.0, true);
    assert!((gain(&ed, clips[0]) - 5.02).abs() < 0.1);
    assert!(
        (gain(&ed, clips[1]) - 19.0).abs() < 0.1,
        "{}",
        gain(&ed, clips[1])
    );

    // Adjust Gain by adds to what is there
    ed.adjust_gain(-3.0);
    assert!((gain(&ed, clips[1]) - 16.0).abs() < 0.1);
    ed.undo();
    assert!((gain(&ed, clips[1]) - 19.0).abs() < 0.1);
}
