//! Clip > Synchronize: lines up the selected clips on a common point, like the Synchronize
//! command of other editors. One clip stays (the one on the lowest track) and the others move
//! until their starts, their ends or their sound match it. Clips that are linked (a video clip
//! and its audio) count once and move together. One undo step.

use std::time::Duration;

use op_core::*;
use op_timeline::MoveSpec;

use crate::audio_sync::{self, Channel};
use crate::editor::Editor;

/// What the clips are lined up by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncMode {
    ClipStart,
    ClipEnd,
    /// The sound of the chosen channel of each clip.
    Audio(Channel),
}

#[derive(Clone, Debug, Default)]
pub struct SyncReport {
    pub moved: usize,
    /// Clips left where they were, and why.
    pub skipped: Vec<String>,
}

/// One clip to line up (its video and audio partners move with it).
struct Unit {
    id: ClipId,
    name: String,
    /// Lowest track first: video before audio, then by number.
    rank: (u8, usize),
    start: f64,
    end: f64,
    /// Where its sound is: stream, seconds into the file, seconds long.
    sound: Option<(AssetId, usize, f64, f64)>,
    skip: Option<String>,
}

impl Editor {
    /// Lines up `clips` of the active sequence by `mode`.
    pub fn synchronize_clips(&mut self, clips: &[ClipId], mode: SyncMode) -> Result<SyncReport, String> {
        let sid = self.active.ok_or("Open a sequence first")?;
        let seq = self.project.sequence(sid).ok_or("The sequence no longer exists")?;
        let rate = seq.rate();

        // one unit per clip, or per link (a video clip and its own audio are one)
        let mut units: Vec<Unit> = Vec::new();
        let mut links: Vec<LinkId> = Vec::new();
        for id in clips {
            let Some((track, c)) = seq.find_clip(*id) else { continue };
            if let Some(l) = c.link {
                if links.contains(&l) {
                    continue;
                }
                links.push(l);
            }
            // the clip that carries the sound: itself, or its audio partner
            let audio = if track.kind == TrackKind::Audio {
                Some(c)
            } else {
                c.link.and_then(|l| {
                    seq.audio.iter().flat_map(|t| &t.clips).find(|x| x.link == Some(l))
                })
            };
            let mut skip = None;
            let sound = audio.and_then(|a| match &a.source {
                ClipSource::Asset { asset, stream, .. } => {
                    if (a.speed.percent() - 100.0).abs() > 1e-6 || a.reverse || a.hold.is_some() {
                        skip = Some("its speed was changed".to_string());
                        None
                    } else {
                        Some((*asset, *stream, a.source_in.seconds(), a.duration.seconds()))
                    }
                }
                _ => None,
            });
            units.push(Unit {
                id: *id,
                name: c.name.clone(),
                rank: (u8::from(track.kind == TrackKind::Audio), track.index),
                start: c.start.seconds(),
                end: c.end().seconds(),
                sound,
                skip,
            });
        }
        if units.len() < 2 {
            return Err("Select two or more clips on different tracks to synchronize".into());
        }
        let anchor = (0..units.len()).min_by_key(|i| units[*i].rank).unwrap_or(0);

        // how far each unit has to move, in seconds
        let mut delta: Vec<Option<f64>> = vec![None; units.len()];
        let mut report = SyncReport::default();
        match mode {
            SyncMode::ClipStart => {
                for (i, u) in units.iter().enumerate() {
                    delta[i] = Some(units[anchor].start - u.start);
                }
            }
            SyncMode::ClipEnd => {
                for (i, u) in units.iter().enumerate() {
                    delta[i] = Some(units[anchor].end - u.end);
                }
            }
            SyncMode::Audio(channel) => {
                let streams: Vec<(AssetId, usize)> = units.iter().filter_map(|u| u.sound.map(|s| (s.0, s.1))).collect();
                self.media.set_assets(self.project.assets.iter());
                self.media.wait_audio(&streams, Duration::from_secs(300));
                let sound_of = |u: &Unit| {
                    let (asset, stream, from, secs) = u.sound?;
                    let audio = self.media.conformed(asset, stream)?;
                    // the first ten minutes of what the clip shows are enough to match
                    Some(audio_sync::mono(&audio, from, secs.min(600.0), channel))
                };
                let reference = sound_of(&units[anchor])
                    .ok_or_else(|| format!("{} has no sound to compare", units[anchor].name))?;
                for (i, u) in units.iter().enumerate() {
                    if i == anchor {
                        delta[i] = Some(0.0);
                        continue;
                    }
                    if let Some(why) = &u.skip {
                        report.skipped.push(format!("{} ({why})", u.name));
                        continue;
                    }
                    let Some(own) = sound_of(u) else {
                        report.skipped.push(format!("{} (no sound)", u.name));
                        continue;
                    };
                    match audio_sync::lag(&reference, &own) {
                        Some(l) if l.confidence >= audio_sync::MIN_CONFIDENCE => {
                            // the clip must start `lag` after the reference starts
                            delta[i] = Some(units[anchor].start + l.seconds - u.start);
                        }
                        _ => report.skipped.push(format!("{} (the sound does not match)", u.name)),
                    }
                }
            }
        }

        // on the frame grid; nothing may end up before the start of the sequence
        let snap = |s: f64| rate.frames_to_dur(rate.dur_to_frames_round(Dur::from_seconds(s)));
        let mut moves: Vec<(ClipId, Dur, f64)> = Vec::new();
        for (i, u) in units.iter().enumerate() {
            if let Some(d) = delta[i] {
                moves.push((u.id, snap(d), u.start));
            }
        }
        let earliest = moves.iter().map(|(_, d, start)| start + d.seconds()).fold(f64::INFINITY, f64::min);
        if earliest < 0.0 {
            let push = snap(-earliest + 1e-9);
            for m in &mut moves {
                m.1 = m.1 + push;
            }
        }
        moves.retain(|(_, d, _)| d.seconds().abs() > 1e-9);
        if moves.is_empty() {
            return Ok(report);
        }
        let done = self.seq_edit("Synchronize Clips", |p, sid, opts| {
            for (id, d, _) in &moves {
                let spec = MoveSpec {
                    clips: vec![*id],
                    delta: *d,
                    track_delta: 0,
                    insert: false,
                    duplicate: false,
                };
                op_timeline::move_clips(p, sid, &spec, opts)?;
            }
            Ok(())
        });
        if done.is_none() {
            return Err("The clips could not be moved".into());
        }
        report.moved = moves.len();
        Ok(report)
    }
}
