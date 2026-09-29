//! Mixer tests with generated audio.

use std::collections::HashMap;
use std::sync::Arc;

use op_audio::*;
use op_core::catalog;
use op_core::*;
use op_media::ConformedAudio;
use op_timeline::*;

struct Source(HashMap<AssetId, Arc<ConformedAudio>>);

impl AudioSource for Source {
    fn audio(&self, asset: AssetId, _stream: usize) -> Option<Arc<ConformedAudio>> {
        self.0.get(&asset).cloned()
    }
}

fn tone_asset(
    p: &mut Project,
    dir: &std::path::Path,
    name: &str,
    freq: f64,
    amp: f32,
) -> (AssetId, ItemId, Arc<ConformedAudio>) {
    let rate = 48000;
    let n = rate * 10;
    let samples: Vec<f32> = (0..n)
        .flat_map(|i| {
            let v = (i as f64 * freq * std::f64::consts::TAU / rate as f64).sin() as f32 * amp;
            [v, v]
        })
        .collect();
    let path = dir.join(format!("{name}.f32"));
    op_media::audio::write_cache(&path, rate as u32, 2, &samples).unwrap();
    let asset = MediaAsset {
        id: AssetId(0),
        path: path.to_string_lossy().into_owned(),
        proxy: None,
        kind: MediaKind::Audio,
        video: None,
        audio: vec![AudioStream {
            index: 0,
            codec: "pcm".into(),
            sample_rate: 48000,
            layout: ChannelLayout::Stereo,
            samples: n as i64,
            start: Dur::ZERO,
        }],
        duration: Dur::from_seconds(10.0),
        interpretation: Interpretation::default(),
        file_size: 0,
        modified_unix: 0,
    };
    let (id, item) = p.add_asset(p.root, asset);
    (id, item, Arc::new(ConformedAudio::open(&path).unwrap()))
}

fn rms(v: &[f32]) -> f64 {
    (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt()
}

fn zero_crossings(v: &[f32]) -> usize {
    v.windows(2)
        .filter(|w| (w[0] <= 0.0) != (w[1] <= 0.0))
        .count()
}

#[test]
fn levels_speed_and_crossfades() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new("audio");
    let (a, a_item, a_audio) = tone_asset(&mut p, dir.path(), "a", 1000.0, 0.5);
    let (b, b_item, b_audio) = tone_asset(&mut p, dir.path(), "b", 500.0, 0.5);
    let src = Source(HashMap::from([(a, a_audio), (b, b_audio)]));
    let (seq, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
    let put = |p: &mut Project, item: ItemId, from: f64, to: f64, at: f64| {
        let spec = SourceClip::from_asset(
            p,
            item,
            SrcRange::new(SrcTime::from_seconds(from), SrcTime::from_seconds(to)),
        )
        .unwrap();
        let (next, ids) = p
            .transact(|p| {
                overwrite(
                    p,
                    seq,
                    &spec,
                    SeqTime::from_seconds(at),
                    &Patch {
                        video: None,
                        audio: vec![Some(0)],
                    },
                    EditOptions::default(),
                )
            })
            .unwrap();
        *p = next;
        ids[0]
    };
    let clip_a = put(&mut p, a_item, 0.0, 4.0, 0.0);
    put(&mut p, b_item, 0.0, 4.0, 4.0);

    let mut m = Mixer::new(48000, 2);
    let mut buf = vec![0f32; 4800 * 2];
    // unity: sine of amplitude 0.5 has RMS 0.354
    m.render(&p, seq, SeqTime::from_seconds(1.0), 1.0, &mut buf, &src);
    assert!((rms(&buf) - 0.3536).abs() < 0.005, "{}", rms(&buf));
    let left: Vec<f32> = buf.iter().step_by(2).copied().collect();
    assert!((zero_crossings(&left) as i64 - 200).abs() <= 2);

    // clip volume -6.02 dB halves the amplitude
    p.sequence_mut(seq)
        .unwrap()
        .clip_mut(clip_a)
        .unwrap()
        .component_mut(catalog::VOLUME)
        .unwrap()
        .param_mut("level")
        .unwrap()
        .value = Value::Float(-6.0206);
    m.render(&p, seq, SeqTime::from_seconds(1.0), 1.0, &mut buf, &src);
    assert!((rms(&buf) - 0.1768).abs() < 0.004, "{}", rms(&buf));

    // double speed doubles the frequency
    let (next, _) = p
        .transact(|p| {
            set_speed(
                p,
                seq,
                &[clip_a],
                Speed::new(2, 1),
                false,
                false,
                EditOptions::default(),
            )
        })
        .unwrap();
    p = next;
    m.render(&p, seq, SeqTime::from_seconds(0.5), 1.0, &mut buf, &src);
    let left: Vec<f32> = buf.iter().step_by(2).copied().collect();
    assert!(
        (zero_crossings(&left) as i64 - 400).abs() <= 4,
        "{}",
        zero_crossings(&left)
    );

    // track mute silences, master volume scales
    p.sequence_mut(seq)
        .unwrap()
        .track_mut(TrackRef::audio(0))
        .unwrap()
        .muted = true;
    m.render(&p, seq, SeqTime::from_seconds(0.5), 1.0, &mut buf, &src);
    assert!(rms(&buf) < 1e-6);
}

#[test]
fn constant_power_crossfade_keeps_level() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = Project::new("audio");
    // the same tone on both sides: a constant-power crossfade of uncorrelated material keeps
    // power; of identical material it peaks at +3 dB mid-way
    let (a, a_item, a_audio) = tone_asset(&mut p, dir.path(), "a", 1000.0, 0.5);
    let src = Source(HashMap::from([(a, a_audio)]));
    let (seq, _) = p.add_sequence(p.root, "S", SequenceSettings::default());
    for (from, at) in [(0.0, 0.0), (4.0, 2.0)] {
        let spec = SourceClip::from_asset(
            &p,
            a_item,
            SrcRange::new(
                SrcTime::from_seconds(from),
                SrcTime::from_seconds(from + 2.0),
            ),
        )
        .unwrap();
        let (next, _) = p
            .transact(|p| {
                overwrite(
                    p,
                    seq,
                    &spec,
                    SeqTime::from_seconds(at),
                    &Patch {
                        video: None,
                        audio: vec![Some(0)],
                    },
                    EditOptions::default(),
                )
            })
            .unwrap();
        p = next;
    }
    let (next, _) = p
        .transact(|p| {
            add_transition(
                p,
                seq,
                TrackRef::audio(0),
                SeqTime::from_seconds(2.0),
                catalog::CONSTANT_POWER,
                Dur::from_seconds(1.0),
                Some(Alignment::CenterAtCut),
                EditOptions::default(),
            )
        })
        .unwrap();
    p = next;
    let mut m = Mixer::new(48000, 2);
    let mut buf = vec![0f32; 480 * 2];
    // before the transition: unity
    m.render(&p, seq, SeqTime::from_seconds(1.0), 1.0, &mut buf, &src);
    assert!((rms(&buf) - 0.3536).abs() < 0.01);
    // well inside the incoming clip: unity again
    m.render(&p, seq, SeqTime::from_seconds(3.0), 1.0, &mut buf, &src);
    assert!((rms(&buf) - 0.3536).abs() < 0.01);
    // during the fade the level stays within the constant-power envelope for in-phase material
    m.render(&p, seq, SeqTime::from_seconds(2.0), 1.0, &mut buf, &src);
    let level = rms(&buf);
    assert!(level > 0.3 && level < 0.51, "{level}");
}

#[test]
fn offline_render_matches_length() {
    let p = {
        let mut p = Project::new("x");
        p.add_sequence(p.root, "S", SequenceSettings::default());
        p
    };
    let seq = *p.sequences.keys().next().unwrap();
    let mut total = 0;
    render_range(
        &p,
        seq,
        SeqRange::new(SeqTime::ZERO, SeqTime::from_seconds(1.5)),
        48000,
        2,
        &NoAudio,
        |b| {
            total += b.len();
            true
        },
    );
    assert_eq!(total, 72000 * 2);
}
