//! The sequence mixer: clips -> clip effects and fixed components -> tracks -> master.
//!
//! The same code renders live playback (at the device rate, any speed including reverse) and
//! export (at the sequence rate), so what is heard is what is exported (DM-AUD-003). Channel
//! conversions are explicit: mono clips feed both sides of a stereo bus through the panner,
//! 5.1 clips fold down to a stereo bus with the standard -3 dB center/surround coefficients,
//! and nothing is reduced silently at import (DM-AUD-001).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use op_core::catalog;
use op_core::plan::{EvalComponent, eval_component};
use op_core::*;
use op_media::ConformedAudio;
use parking_lot::Mutex;

use crate::dsp::db_to_gain;
use crate::effects::{self, FxState};

/// Provides conformed audio for asset streams.
pub trait AudioSource: Send + Sync {
    fn audio(&self, asset: AssetId, stream: usize) -> Option<Arc<ConformedAudio>>;
}

/// No audio at all (tests, silent previews).
pub struct NoAudio;

impl AudioSource for NoAudio {
    fn audio(&self, _asset: AssetId, _stream: usize) -> Option<Arc<ConformedAudio>> {
        None
    }
}

/// Peak levels since the UI last read them, as bit patterns of f32 (lock-free).
#[derive(Debug, Default)]
pub struct Meters {
    master: [AtomicU32; 8],
    tracks: Mutex<HashMap<TrackId, [f32; 2]>>,
}

impl Meters {
    fn raise(slot: &AtomicU32, v: f32) {
        let mut cur = slot.load(Ordering::Relaxed);
        while f32::from_bits(cur) < v {
            match slot.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => break,
                Err(x) => cur = x,
            }
        }
    }

    /// Takes (and resets) the master peaks.
    pub fn take_master(&self, channels: usize) -> Vec<f32> {
        (0..channels.min(8))
            .map(|c| f32::from_bits(self.master[c].swap(0, Ordering::Relaxed)))
            .collect()
    }

    pub fn take_tracks(&self) -> HashMap<TrackId, [f32; 2]> {
        std::mem::take(&mut *self.tracks.lock())
    }
}

pub struct Mixer {
    /// Output sample rate.
    pub rate: u32,
    /// Output channels (the sequence master layout).
    pub channels: usize,
    states: HashMap<(u64, ComponentId), FxState>,
    pub meters: Arc<Meters>,
    scratch: Vec<f32>,
}

const MAX_DEPTH: usize = 8;

fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c0 = y1;
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + c0
}

impl Mixer {
    pub fn new(rate: u32, channels: usize) -> Mixer {
        Mixer {
            rate,
            channels: channels.max(1),
            states: HashMap::new(),
            meters: Arc::new(Meters::default()),
            scratch: Vec::new(),
        }
    }

    /// Clears effect memories (after a seek, so filters and echoes do not carry over).
    pub fn reset(&mut self) {
        for s in self.states.values_mut() {
            s.reset();
        }
    }

    /// Renders `out.len() / channels` frames of `seq` starting at `start`, advancing `speed`
    /// sequence seconds per real second (negative plays backwards).
    pub fn render(
        &mut self,
        project: &Project,
        seq: SequenceId,
        start: SeqTime,
        speed: f64,
        out: &mut [f32],
        source: &dyn AudioSource,
    ) {
        out.fill(0.0);
        let channels = self.channels;
        self.mix_sequence(project, seq, start, speed, out, channels, source, 0);
        for frame in out.chunks_exact(channels) {
            for (c, v) in frame.iter().enumerate().take(8) {
                Meters::raise(&self.meters.master[c], v.abs());
            }
        }
    }

    fn time_at(&self, start: SeqTime, speed: f64, i: usize) -> SeqTime {
        start + Dur::from_seconds(i as f64 * speed / self.rate as f64)
    }

    #[allow(clippy::too_many_arguments)]
    fn mix_sequence(
        &mut self,
        project: &Project,
        sid: SequenceId,
        start: SeqTime,
        speed: f64,
        out: &mut [f32],
        out_ch: usize,
        source: &dyn AudioSource,
        depth: usize,
    ) {
        let Some(seq) = project.sequence(sid) else {
            return;
        };
        let frames = out.len() / out_ch;
        if frames == 0 || depth > MAX_DEPTH {
            return;
        }
        let t_first = start;
        let t_last = self.time_at(start, speed, frames.saturating_sub(1));
        let (lo, hi) = if t_first <= t_last {
            (t_first, t_last)
        } else {
            (t_last, t_first)
        };
        let block = SeqRange::new(lo, hi + Dur(1));
        let any_solo = seq.audio.iter().any(|t| t.solo);
        let mut bus = vec![0f32; frames * 2];
        let mut surround = vec![0f32; if out_ch == 6 { frames * 6 } else { 0 }];
        for track in &seq.audio {
            if track.muted || (any_solo && !track.solo) {
                continue;
            }
            bus.fill(0.0);
            let mut active = false;
            // clips and the transitions that extend them
            for clip in &track.clips {
                let mut reach = clip.range();
                for tr in &track.transitions {
                    if tr.from == Some(clip.id) || tr.to == Some(clip.id) {
                        let r = tr.range();
                        reach = SeqRange::new(reach.start.min(r.start), reach.end.max(r.end));
                    }
                }
                if !reach.overlaps(&block) || !clip.enabled {
                    continue;
                }
                active = true;
                self.mix_clip(
                    project, seq, track, clip, start, speed, &mut bus, source, depth,
                );
            }
            if !active {
                continue;
            }
            // track fader and pan
            let t_mid = self.time_at(start, speed, frames / 2);
            let tsrc: SrcTime = t_mid.cast();
            let mut vol = 1.0f32;
            let mut balance = 0.0f64;
            for comp in &track.components {
                if !comp.enabled {
                    continue;
                }
                let e = eval_component(comp, tsrc);
                match comp.effect.as_str() {
                    catalog::VOLUME if !e.bool("bypass") => {
                        vol *= db_to_gain(e.f64("level")) as f32
                    }
                    catalog::PANNER => balance = e.f64("balance") / 100.0,
                    _ => {}
                }
            }
            let (gl, gr) = balance_gains(balance);
            let mut peak = [0f32; 2];
            for i in 0..frames {
                let l = bus[i * 2] * vol * gl;
                let r = bus[i * 2 + 1] * vol * gr;
                peak[0] = peak[0].max(l.abs());
                peak[1] = peak[1].max(r.abs());
                match out_ch {
                    1 => out[i] += (l + r) * 0.5,
                    6 => {
                        surround[i * 6] += l;
                        surround[i * 6 + 1] += r;
                    }
                    n => {
                        out[i * n] += l;
                        out[i * n + 1] += r;
                    }
                }
            }
            let mut m = self.meters.tracks.lock();
            let e = m.entry(track.id).or_insert([0.0; 2]);
            e[0] = e[0].max(peak[0]);
            e[1] = e[1].max(peak[1]);
        }
        if out_ch == 6 {
            for (o, s) in out.iter_mut().zip(&surround) {
                *o += *s;
            }
        }
        // master fader
        let tsrc: SrcTime = self.time_at(start, speed, frames / 2).cast();
        for comp in &seq.master {
            if comp.enabled && comp.effect == catalog::VOLUME {
                let e = eval_component(comp, tsrc);
                if !e.bool("bypass") {
                    let g = db_to_gain(e.f64("level")) as f32;
                    out.iter_mut().for_each(|s| *s *= g);
                }
            }
        }
    }

    /// Adds one clip (stereo) into `bus`.
    #[allow(clippy::too_many_arguments)]
    fn mix_clip(
        &mut self,
        project: &Project,
        seq: &Sequence,
        track: &Track,
        clip: &Clip,
        start: SeqTime,
        speed: f64,
        bus: &mut [f32],
        source: &dyn AudioSource,
        depth: usize,
    ) {
        let frames = bus.len() / 2;
        // per-frame gain from the clip's own range and its transitions
        let mut env = vec![0f32; frames];
        for (i, g) in env.iter_mut().enumerate() {
            let t = self.time_at(start, speed, i);
            *g = clip_gain(track, clip, t) as f32;
        }
        if env.iter().all(|g| *g == 0.0) {
            return;
        }
        // source samples (stereo) for every output frame
        let mut buf = vec![0f32; frames * 2];
        match &clip.source {
            ClipSource::Asset { asset, stream, .. } => {
                let Some(audio) = source.audio(*asset, *stream) else {
                    return;
                };
                self.read_asset(&audio, clip, start, speed, &mut buf);
            }
            ClipSource::Sequence { sequence, .. } => {
                if depth < MAX_DEPTH && *sequence != seq.id {
                    let s0 = clip.to_source(start);
                    let s1 = clip.to_source(self.time_at(start, speed, frames.max(2) - 1));
                    let nested_speed = if frames > 1 {
                        (s1 - s0).seconds() * self.rate as f64 / (frames - 1) as f64
                    } else {
                        speed
                    };
                    self.mix_sequence(
                        project,
                        *sequence,
                        s0.cast(),
                        nested_speed,
                        &mut buf,
                        2,
                        source,
                        depth + 1,
                    );
                }
            }
            ClipSource::Generator { item } => {
                if let Some(ItemKind::Synthetic {
                    generator: Generator::BarsAndTone,
                    ..
                }) = project.item(*item).map(|i| &i.kind)
                {
                    // 1 kHz reference tone at -20 dBFS
                    for i in 0..frames {
                        let s = clip.to_source(self.time_at(start, speed, i)).seconds();
                        let v = (s * 1000.0 * std::f64::consts::TAU).sin() as f32 * 0.1;
                        buf[i * 2] = v;
                        buf[i * 2 + 1] = v;
                    }
                }
            }
            ClipSource::Graphic => return,
        }
        // clip gain and effects
        let t_src = clip.to_source(self.time_at(start, speed, frames / 2));
        let evals: Vec<EvalComponent> = clip
            .components
            .iter()
            .filter(|c| c.enabled)
            .map(|c| eval_component(c, t_src))
            .collect();
        let pre = db_to_gain(clip.gain_db) as f32;
        if pre != 1.0 {
            buf.iter_mut().for_each(|s| *s *= pre);
        }
        for e in &evals {
            if e.def_is_standard_audio() {
                let state = self.states.entry((clip.id.0, e.id)).or_default();
                effects::process(state, e, &mut buf, 2, self.rate);
            }
        }
        // fixed components: channel volume, volume (ramped between block ends), panner
        let mut vol_a = 1.0f32;
        let mut vol_b = 1.0f32;
        let mut balance = 0.0f64;
        let mut ch_gain = [1.0f32, 1.0];
        let src_first = clip.to_source(start);
        let src_last = clip.to_source(self.time_at(start, speed, frames.saturating_sub(1)));
        for comp in clip.components.iter().filter(|c| c.enabled) {
            match comp.effect.as_str() {
                catalog::VOLUME => {
                    let a = eval_component(comp, src_first);
                    let b = eval_component(comp, src_last);
                    if !a.bool("bypass") {
                        vol_a = db_to_gain(a.f64("level")) as f32;
                        vol_b = db_to_gain(b.f64("level")) as f32;
                    }
                }
                catalog::CHANNEL_VOLUME => {
                    let e = eval_component(comp, t_src);
                    if !e.bool("bypass") {
                        ch_gain = [
                            db_to_gain(e.f64("left")) as f32,
                            db_to_gain(e.f64("right")) as f32,
                        ];
                    }
                }
                catalog::PANNER => balance = eval_component(comp, t_src).f64("balance") / 100.0,
                _ => {}
            }
        }
        let mono = clip.channels == Some(ChannelLayout::Mono);
        let (gl, gr) = if mono {
            pan_gains(balance)
        } else {
            balance_gains(balance)
        };
        for i in 0..frames {
            let k = if frames > 1 {
                i as f32 / (frames - 1) as f32
            } else {
                0.0
            };
            let g = env[i] * (vol_a + (vol_b - vol_a) * k);
            bus[i * 2] += buf[i * 2] * g * ch_gain[0] * gl;
            bus[i * 2 + 1] += buf[i * 2 + 1] * g * ch_gain[1] * gr;
        }
    }

    /// Reads stereo samples of a conformed stream for each output frame (Hermite interpolation
    /// between source samples, so any speed and rate works).
    fn read_asset(
        &mut self,
        audio: &ConformedAudio,
        clip: &Clip,
        start: SeqTime,
        speed: f64,
        out: &mut [f32],
    ) {
        let frames = out.len() / 2;
        let rate = audio.info.rate as f64;
        let ch = audio.info.channels.max(1) as usize;
        let pos =
            |i: usize, me: &Self| clip.to_source(me.time_at(start, speed, i)).seconds() * rate;
        let p0 = pos(0, self);
        let p1 = pos(frames.saturating_sub(1), self);
        let first = p0.min(p1).floor() as i64 - 2;
        let last = p0.max(p1).ceil() as i64 + 3;
        let span = (last - first).max(1) as usize;
        if span > 48_000_000 {
            return;
        }
        self.scratch.resize(span * ch, 0.0);
        audio.read(first, &mut self.scratch[..span * ch]);
        let fold = |frame: &[f32]| -> (f32, f32) {
            match ch {
                1 => (frame[0], frame[0]),
                6 => {
                    // L R C LFE Ls Rs -> stereo
                    let c = frame[2] * std::f32::consts::FRAC_1_SQRT_2;
                    (
                        frame[0] + c + frame[4] * std::f32::consts::FRAC_1_SQRT_2,
                        frame[1] + c + frame[5] * std::f32::consts::FRAC_1_SQRT_2,
                    )
                }
                _ => (frame[0], frame[1]),
            }
        };
        for i in 0..frames {
            let p = pos(i, self);
            let base = p.floor();
            let t = (p - base) as f32;
            let idx = base as i64 - first;
            let get = |k: i64| -> (f32, f32) {
                let j = (idx + k).clamp(0, span as i64 - 1) as usize;
                fold(&self.scratch[j * ch..j * ch + ch])
            };
            let (l, r) = if t.abs() < 1e-6 {
                get(0)
            } else {
                let (a, b, c, d) = (get(-1), get(0), get(1), get(2));
                (
                    hermite(a.0, b.0, c.0, d.0, t),
                    hermite(a.1, b.1, c.1, d.1, t),
                )
            };
            out[i * 2] = l;
            out[i * 2 + 1] = r;
        }
    }
}

trait StandardAudio {
    fn def_is_standard_audio(&self) -> bool;
}

impl StandardAudio for EvalComponent {
    fn def_is_standard_audio(&self) -> bool {
        catalog::find(&self.effect).is_some_and(|d| d.kind == catalog::EffectKind::AudioEffect)
    }
}

/// Stereo balance: attenuates the opposite side only.
fn balance_gains(b: f64) -> (f32, f32) {
    let b = b.clamp(-1.0, 1.0);
    ((1.0 - b.max(0.0)) as f32, (1.0 + b.min(0.0)) as f32)
}

/// Mono pan with a -3 dB center (constant power).
fn pan_gains(p: f64) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * std::f64::consts::FRAC_PI_4;
    (
        (a.cos() * std::f64::consts::SQRT_2) as f32,
        (a.sin() * std::f64::consts::SQRT_2) as f32,
    )
}

/// Gain of a clip at sequence time `t`, including transition fades and handles.
fn clip_gain(track: &Track, clip: &Clip, t: SeqTime) -> f64 {
    let mut gain = if clip.range().contains(t) { 1.0 } else { 0.0 };
    for tr in &track.transitions {
        let r = tr.range();
        if !r.contains(t) {
            continue;
        }
        let p = tr.progress(t);
        let (out_g, in_g) = effects::crossfade(&tr.effect, p);
        if tr.from == Some(clip.id) {
            gain = out_g;
        } else if tr.to == Some(clip.id) {
            gain = in_g;
        }
    }
    gain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_laws() {
        let (l, r) = pan_gains(0.0);
        assert!((l - 1.0).abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = pan_gains(-1.0);
        assert!((l - std::f32::consts::SQRT_2).abs() < 1e-3 && r.abs() < 1e-6);
        assert_eq!(balance_gains(0.5), (0.5, 1.0));
    }
}
