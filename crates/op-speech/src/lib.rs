//! Speech to text for captions, on this computer: OpenAI's Whisper models run through candle
//! (pure Rust, CPU). Models are downloaded once from Hugging Face on the user's request and kept
//! in the data folder; nothing is sent anywhere.
//!
//! Whisper reads 30-second windows of 16 kHz mono audio and writes text with timestamp tokens
//! every few words. Words inside a timed span get times in proportion to their length, which is
//! close enough to light up each word as it is said.

mod decoder;
mod mel;
mod tokenizer;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use candle_core::{D, Device, IndexOp, Tensor};
use candle_transformers::models::whisper::{
    self as m, Config, audio,
    model::{AudioEncoder, Whisper},
};
use thiserror::Error;

pub use tokenizer::Tokenizer;

#[derive(Debug, Error)]
pub enum SpeechError {
    #[error("{0}")]
    Model(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("download failed: {0}")]
    Download(String),
    #[error("cancelled")]
    Cancelled,
}

impl From<candle_core::Error> for SpeechError {
    fn from(e: candle_core::Error) -> Self {
        SpeechError::Model(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, SpeechError>;

pub const SAMPLE_RATE: u32 = m::SAMPLE_RATE as u32;

/// The multilingual Whisper models offered for download.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
}

impl ModelSize {
    pub const ALL: [ModelSize; 3] = [ModelSize::Tiny, ModelSize::Base, ModelSize::Small];

    pub fn id(self) -> &'static str {
        match self {
            ModelSize::Tiny => "whisper-tiny",
            ModelSize::Base => "whisper-base",
            ModelSize::Small => "whisper-small",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ModelSize::Tiny => "Tiny",
            ModelSize::Base => "Base",
            ModelSize::Small => "Small",
        }
    }

    /// Approximate download size in megabytes.
    pub fn megabytes(self) -> u32 {
        match self {
            ModelSize::Tiny => 152,
            ModelSize::Base => 291,
            ModelSize::Small => 968,
        }
    }

    pub fn from_id(id: &str) -> Option<ModelSize> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    fn files() -> [&'static str; 3] {
        ["config.json", "tokenizer.json", "model.safetensors"]
    }

    /// Folder of this model inside `models`.
    pub fn dir(self, models: &Path) -> PathBuf {
        models.join(self.id())
    }

    pub fn is_downloaded(self, models: &Path) -> bool {
        let d = self.dir(models);
        Self::files().iter().all(|f| d.join(f).is_file())
    }

    /// Downloads the model files from Hugging Face. `progress(done, total)` returns false to
    /// cancel; partial files never take the final names.
    pub fn download(self, models: &Path, progress: &mut dyn FnMut(u64, u64) -> bool) -> Result<()> {
        use std::io::{Read, Write};
        let dir = self.dir(models);
        std::fs::create_dir_all(&dir)?;
        let total_guess = self.megabytes() as u64 * 1_000_000;
        let mut done_before = 0u64;
        for f in Self::files() {
            let dst = dir.join(f);
            if dst.is_file() {
                continue;
            }
            let url = format!(
                "https://huggingface.co/openai/{}/resolve/main/{f}",
                self.id()
            );
            let resp = ureq::get(&url)
                .header("User-Agent", "OpenPremier")
                .call()
                .map_err(|e| SpeechError::Download(format!("{f}: {e}")))?;
            let part = dir.join(format!("{f}.part"));
            let mut out = std::fs::File::create(&part)?;
            let mut reader = resp.into_body().into_reader();
            let mut buf = vec![0u8; 1 << 16];
            let mut done = 0u64;
            loop {
                let n = reader
                    .read(&mut buf)
                    .map_err(|e| SpeechError::Download(e.to_string()))?;
                if n == 0 {
                    break;
                }
                out.write_all(&buf[..n])?;
                done += n as u64;
                if !progress(done_before + done, total_guess.max(done_before + done)) {
                    drop(out);
                    let _ = std::fs::remove_file(&part);
                    return Err(SpeechError::Cancelled);
                }
            }
            out.flush()?;
            drop(out);
            std::fs::rename(&part, &dst)?;
            done_before += done;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// Seconds from the start of the audio.
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub words: Vec<Word>,
}

/// Whisper's language codes in token order.
pub const LANGUAGES: [&str; 99] = [
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln",
    "ha", "ba", "jw", "su",
];

/// Splits a timed span of text into words with times in proportion to their length (letters
/// plus one, so short words still get a moment).
pub fn spread_words(text: &str, start: f64, end: f64) -> Vec<Word> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let weights: Vec<f64> = words
        .iter()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).count() as f64 + 1.0)
        .collect();
    let total: f64 = weights.iter().sum();
    let span = (end - start).max(0.0);
    let mut t = start;
    words
        .iter()
        .zip(weights)
        .map(|(w, k)| {
            let d = span * k / total;
            let word = Word {
                start: t,
                end: t + d,
                text: w.to_string(),
            };
            t += d;
            word
        })
        .collect()
}

/// What Whisper writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Task {
    /// The words in the language spoken.
    #[default]
    Transcribe,
    /// An English translation of what is said (Whisper only translates into English).
    Translate,
}

/// A loaded model, ready to transcribe.
pub struct Transcriber {
    encoder: AudioEncoder,
    decoder: decoder::Decoder,
    config: Config,
    tok: Tokenizer,
    filters: Vec<f32>,
    device: Device,
    sot: u32,
    eot: u32,
    transcribe: u32,
    translate: u32,
    no_timestamps: u32,
    no_speech: Option<u32>,
    ts_begin: u32,
    suppress: Vec<bool>,
}

struct Decoded {
    tokens: Vec<u32>,
    avg_logprob: f64,
    no_speech_prob: f64,
}

/// Samples per silence check (half a second).
const CHUNK: usize = m::SAMPLE_RATE / 2;

/// The level below which half a second counts as silence: 30 dB under the loud parts of this
/// recording, and never above -50 dBFS.
fn silence_level(pcm: &[f32]) -> f32 {
    let mut levels: Vec<f32> = pcm.chunks(CHUNK).map(rms).collect();
    if levels.is_empty() {
        return 0.0;
    }
    levels.sort_by(f32::total_cmp);
    let loud = levels[(levels.len() * 9 / 10).min(levels.len() - 1)];
    (loud * 0.03).min(0.003)
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

impl Transcriber {
    pub fn load(dir: &Path) -> Result<Transcriber> {
        let device = Device::Cpu;
        let config: Config =
            serde_json::from_str(&std::fs::read_to_string(dir.join("config.json"))?)
                .map_err(|e| SpeechError::Model(format!("config.json: {e}")))?;
        let tok = Tokenizer::load(&dir.join("tokenizer.json"))?;
        let weights = std::fs::read(dir.join("model.safetensors"))?;
        let vb = candle_nn::VarBuilder::from_buffered_safetensors(weights, m::DTYPE, &device)?;
        let filters = mel::filters(config.num_mel_bins, m::N_FFT, SAMPLE_RATE);
        // candle's encoder, and our decoder with a self-attention cache in place of its own
        let Whisper { encoder, .. } = Whisper::load(&vb, config.clone())?;
        let decoder = decoder::Decoder::load(vb.pp("model.decoder"), &config)?;
        drop(vb);
        let id = |t: &str| {
            tok.id(t)
                .ok_or_else(|| SpeechError::Model(format!("the tokenizer has no {t}")))
        };
        let sot = id(m::SOT_TOKEN)?;
        let eot = id(m::EOT_TOKEN)?;
        let transcribe = id(m::TRANSCRIBE_TOKEN)?;
        let translate = id(m::TRANSLATE_TOKEN)?;
        let no_timestamps = id(m::NO_TIMESTAMPS_TOKEN)?;
        let no_speech = m::NO_SPEECH_TOKENS.iter().find_map(|t| tok.id(t));
        let mut suppress = vec![false; config.vocab_size];
        for &t in &config.suppress_tokens {
            if let Some(s) = suppress.get_mut(t as usize) {
                *s = true;
            }
        }
        suppress[no_timestamps as usize] = true;
        Ok(Transcriber {
            encoder,
            decoder,
            config,
            tok,
            filters,
            device,
            sot,
            eot,
            transcribe,
            translate,
            no_timestamps,
            no_speech,
            ts_begin: no_timestamps + 1,
            suppress,
        })
    }

    /// Transcribes 16 kHz mono samples. `language` is a code such as "es" (None detects it).
    /// `progress` gets 0..1. Returns the language spoken and the timed segments.
    pub fn transcribe(
        &mut self,
        pcm: &[f32],
        language: Option<&str>,
        task: Task,
        progress: &mut dyn FnMut(f32),
        cancel: &AtomicBool,
    ) -> Result<(String, Vec<Segment>)> {
        let mel = audio::pcm_to_mel(&self.config, pcm, &self.filters);
        let n_mels = self.config.num_mel_bins;
        let mel_len = mel.len() / n_mels;
        let mel = Tensor::from_vec(mel, (1, n_mels, mel_len), &self.device)?;
        // frames that hold audio (the rest is padding)
        let content = (pcm.len() / m::HOP_LENGTH).min(mel_len);
        let quiet = silence_level(pcm);
        let language = match language {
            Some(l) if LANGUAGES.contains(&l) => l.to_string(),
            _ => self.detect_language(&mel, pcm, content)?,
        };
        let lang_token = self
            .tok
            .id(&format!("<|{language}|>"))
            .ok_or_else(|| SpeechError::Model(format!("no language token for {language}")))?;
        let task_token = match task {
            Task::Transcribe => self.transcribe,
            Task::Translate => self.translate,
        };
        let prompt = [self.sot, lang_token, task_token];
        let secs_per_frame = m::HOP_LENGTH as f64 / m::SAMPLE_RATE as f64;
        let frames_per_chunk = CHUNK / m::HOP_LENGTH;
        let mut segments = Vec::new();
        let mut seek = 0usize;
        while seek < content {
            if cancel.load(Ordering::Relaxed) {
                return Err(SpeechError::Cancelled);
            }
            // silence costs nothing: skip it half a second at a time
            let silent = |f: usize| {
                let a = f * m::HOP_LENGTH;
                let b = (a + CHUNK).min(pcm.len());
                a >= pcm.len() || rms(&pcm[a..b]) < quiet
            };
            while seek < content && silent(seek) {
                seek += frames_per_chunk;
            }
            if seek >= content {
                break;
            }
            progress(seek as f32 / content.max(1) as f32);
            let size = (content - seek).min(m::N_FRAMES);
            let window = self.window(&mel, seek, size)?;
            let offset = seek as f64 * secs_per_frame;
            let window_secs = size as f64 * secs_per_frame;
            let dr = self.decode_with_fallback(&window, &prompt, window_secs)?;
            if dr.no_speech_prob > m::NO_SPEECH_THRESHOLD && dr.avg_logprob < m::LOGPROB_THRESHOLD {
                seek += size;
                continue;
            }
            let (mut segs, last_ts) = self.segments(&dr.tokens, offset, window_secs);
            segments.append(&mut segs);
            // continue from the last complete segment; a window without one moves on whole
            let advance = match last_ts {
                Some(t) if t > 1.0 && t < window_secs - 0.5 => (t / secs_per_frame) as usize,
                _ => size,
            };
            seek += advance.max(1);
        }
        progress(1.0);
        Ok((language, segments))
    }

    /// `size` frames from `seek`, padded to the 30 seconds the encoder expects.
    fn window(&self, mel: &Tensor, seek: usize, size: usize) -> Result<Tensor> {
        let window = mel.narrow(2, seek, size)?;
        Ok(if size < m::N_FRAMES {
            let n_mels = self.config.num_mel_bins;
            let pad = Tensor::zeros((1, n_mels, m::N_FRAMES - size), m::DTYPE, &self.device)?;
            Tensor::cat(&[&window, &pad], 2)?
        } else {
            window
        })
    }

    /// The language spoken, from up to three 30-second stretches with the most speech (the
    /// start of a video is often music or silence, which Whisper takes for English).
    fn detect_language(&mut self, mel: &Tensor, pcm: &[f32], content: usize) -> Result<String> {
        let quiet = silence_level(pcm);
        let mut windows: Vec<(usize, usize)> = (0..content.max(1))
            .step_by(m::N_FRAMES)
            .map(|f| {
                let a = f * m::HOP_LENGTH;
                let b = (a + m::N_SAMPLES).min(pcm.len());
                let voiced = pcm
                    .get(a..b)
                    .unwrap_or(&[])
                    .chunks(CHUNK)
                    .filter(|c| rms(c) >= quiet)
                    .count();
                (f, voiced)
            })
            .collect();
        windows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        windows.truncate(3);
        let codes: Vec<(&str, u32)> = LANGUAGES
            .iter()
            .filter_map(|l| Some((*l, self.tok.id(&format!("<|{l}|>"))?)))
            .collect();
        let mut score = vec![0.0f32; codes.len()];
        for (f, voiced) in windows {
            if voiced == 0 && !score.iter().all(|s| *s == 0.0) {
                continue;
            }
            let size = (content - f.min(content))
                .clamp(1, m::N_FRAMES)
                .min(mel.dim(2)? - f);
            let window = self.window(mel, f, size)?;
            let features = self.encoder.forward(&window, true)?;
            self.decoder.reset();
            let ys = self.decoder.forward(&[self.sot], &features)?;
            let logits: Vec<f32> = self.decoder.logits(&ys)?.i(0)?.i(0)?.to_vec1()?;
            // probabilities among the language tokens, added up over the windows
            let lang: Vec<f32> = codes
                .iter()
                .map(|(_, id)| {
                    logits
                        .get(*id as usize)
                        .copied()
                        .unwrap_or(f32::NEG_INFINITY)
                })
                .collect();
            for (s, p) in score.iter_mut().zip(log_softmax(&lang)) {
                *s += p.exp();
            }
        }
        let best = codes
            .iter()
            .zip(&score)
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|((l, _), _)| l.to_string())
            .unwrap_or_else(|| "en".into());
        log::info!("detected language: {best}");
        Ok(best)
    }

    fn decode_with_fallback(
        &mut self,
        mel: &Tensor,
        prompt: &[u32],
        window_secs: f64,
    ) -> Result<Decoded> {
        // the encoder runs once per window, whatever the number of tries
        let features = self.encoder.forward(mel, true)?;
        self.decoder.reset();
        let mut last = None;
        for (i, &t) in m::TEMPERATURES.iter().enumerate() {
            let dr = self.decode(&features, prompt, t, window_secs, i as u64)?;
            let text = self.tok.decode(&dr.tokens, self.ts_begin);
            let ratio = compression_ratio(&text);
            let good =
                ratio <= m::COMPRESSION_RATIO_THRESHOLD && dr.avg_logprob >= m::LOGPROB_THRESHOLD;
            if good || dr.no_speech_prob > m::NO_SPEECH_THRESHOLD {
                return Ok(dr);
            }
            last = Some(dr);
        }
        last.ok_or_else(|| SpeechError::Model("no decoding".into()))
    }

    fn decode(
        &mut self,
        features: &Tensor,
        prompt: &[u32],
        temperature: f64,
        window_secs: f64,
        seed: u64,
    ) -> Result<Decoded> {
        self.decoder.restart();
        let mut tokens: Vec<u32> = prompt.to_vec();
        let mut sum_logprob = 0.0f64;
        let mut no_speech_prob = 0.0f64;
        let mut rng = 0x9E37_79B9_7F4A_7C15u64 ^ seed.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let capacity = self.decoder.capacity();
        let sample_len = capacity / 2;
        // the last timestamp this window may use
        let max_ts = self.ts_begin + (window_secs / 0.02).round() as u32;
        let mut ys = self.decoder.forward(prompt, features)?;
        if let Some(ns) = self.no_speech {
            let first = self.decoder.logits(&ys.i((..1, ..1))?)?.i(0)?.i(0)?;
            let probs: Vec<f32> = candle_nn::ops::softmax(&first, D::Minus1)?.to_vec1()?;
            no_speech_prob = probs.get(ns as usize).copied().unwrap_or(0.0) as f64;
        }
        for _ in 0..sample_len {
            let (_, seq_len, _) = ys.dims3()?;
            let logits = self
                .decoder
                .logits(&ys.i((..1, seq_len - 1..))?)?
                .i(0)?
                .i(0)?;
            let mut logits: Vec<f32> = logits.to_vec1()?;
            self.apply_rules(&mut logits, &tokens[prompt.len()..], max_ts);
            let logprobs = log_softmax(&logits);
            let next = if temperature > 0.0 {
                sample(&logits, temperature, &mut rng)
            } else {
                argmax(&logits)
            };
            sum_logprob += logprobs[next as usize] as f64;
            tokens.push(next);
            if next == self.eot || tokens.len() >= capacity {
                break;
            }
            ys = self.decoder.forward(&[next], features)?;
        }
        let generated = tokens[prompt.len()..].to_vec();
        let n = generated.len().max(1);
        Ok(Decoded {
            tokens: generated,
            avg_logprob: sum_logprob / n as f64,
            no_speech_prob,
        })
    }

    /// Whisper's timestamp rules: text starts with a timestamp, timestamps come in pairs and
    /// never go back, and a timestamp wins when timestamps together are more likely than any
    /// single text token.
    fn apply_rules(&self, logits: &mut [f32], generated: &[u32], max_ts: u32) {
        const NEG: f32 = f32::NEG_INFINITY;
        for (l, s) in logits.iter_mut().zip(&self.suppress) {
            if *s {
                *l = NEG;
            }
        }
        let ts = self.ts_begin as usize;
        let is_ts = |t: u32| t >= self.ts_begin;
        let last_ts = generated.last().is_some_and(|t| is_ts(*t));
        // with a single token so far, Whisper counts the missing one before it as a timestamp
        let penult_ts = generated.len() < 2 || is_ts(generated[generated.len() - 2]);
        if last_ts {
            if penult_ts {
                // a closed pair: text must follow
                logits[ts..].fill(NEG);
            } else {
                // an opened timestamp must be closed (or the text ends)
                logits[..self.eot as usize].fill(NEG);
            }
        }
        if generated.is_empty() {
            // the first token is a timestamp within the first second
            for l in logits[..ts].iter_mut() {
                *l = NEG;
            }
            let limit = (ts + 50).min(logits.len());
            logits[limit..].fill(NEG);
        }
        // timestamps never go backwards, nor past the audio in this window
        if let Some(prev) = generated.iter().rev().copied().find(|t| is_ts(*t)) {
            let floor = if last_ts && !penult_ts {
                prev
            } else {
                prev + 1
            };
            let end = (floor as usize).min(logits.len());
            if end > ts {
                logits[ts..end].fill(NEG);
            }
        }
        if (max_ts as usize) + 1 < logits.len() {
            logits[max_ts as usize + 1..].fill(NEG);
        }
        // when a timestamp is more likely than any text token, it must come now
        let lp = log_softmax(logits);
        let ts_mass = log_sum_exp(&lp[ts..]);
        let text_max = lp[..ts].iter().copied().fold(NEG, f32::max);
        if ts_mass > text_max {
            logits[..ts].fill(NEG);
        }
    }

    /// Timed segments from the tokens of one window, and the last closing timestamp (seconds
    /// from the window start).
    fn segments(
        &self,
        tokens: &[u32],
        offset: f64,
        window_secs: f64,
    ) -> (Vec<Segment>, Option<f64>) {
        let mut out = Vec::new();
        let mut start: Option<f64> = None;
        let mut text_tokens: Vec<u32> = Vec::new();
        let mut last_close = None;
        let time = |t: u32| (t - self.ts_begin) as f64 * 0.02;
        for &t in tokens {
            if t == self.eot {
                break;
            }
            if t >= self.ts_begin {
                let s = time(t);
                match start {
                    None => start = Some(s),
                    Some(s0) => {
                        let text = self.tok.decode(&text_tokens, self.ts_begin);
                        let text = text.trim();
                        if has_words(text) {
                            out.push(segment(text, offset + s0, offset + s.max(s0 + 0.1)));
                        }
                        text_tokens.clear();
                        last_close = Some(s);
                        start = None;
                    }
                }
            } else if t < self.eot {
                text_tokens.push(t);
            }
        }
        // text after the last timestamp runs to the end of the speech in this window
        let tail = self.tok.decode(&text_tokens, self.ts_begin);
        if has_words(&tail) {
            let s0 = start.or(last_close).unwrap_or(0.0);
            out.push(segment(
                tail.trim(),
                offset + s0,
                offset + window_secs.max(s0 + 0.5),
            ));
            last_close = None;
        }
        (out, last_close)
    }

    pub fn no_timestamps_token(&self) -> u32 {
        self.no_timestamps
    }
}

/// Text worth a caption: at least one letter or digit (Whisper sometimes ends with a lone ".").
fn has_words(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric())
}

fn segment(text: &str, start: f64, end: f64) -> Segment {
    Segment {
        start,
        end,
        text: text.to_string(),
        words: spread_words(text, start, end),
    }
}

fn argmax(v: &[f32]) -> u32 {
    v.iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i as u32)
        .unwrap_or(0)
}

fn log_sum_exp(v: &[f32]) -> f32 {
    let m = v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if !m.is_finite() {
        return m;
    }
    m + v.iter().map(|x| (x - m).exp()).sum::<f32>().ln()
}

fn log_softmax(v: &[f32]) -> Vec<f32> {
    let lse = log_sum_exp(v);
    v.iter().map(|x| x - lse).collect()
}

fn sample(logits: &[f32], temperature: f64, rng: &mut u64) -> u32 {
    let t = temperature as f32;
    let m = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let w: Vec<f32> = logits.iter().map(|l| ((l - m) / t).exp()).collect();
    let total: f32 = w.iter().sum();
    // xorshift
    *rng ^= *rng << 13;
    *rng ^= *rng >> 7;
    *rng ^= *rng << 17;
    let mut r = (*rng >> 11) as f32 / (1u64 << 53) as f32 * total;
    for (i, x) in w.iter().enumerate() {
        r -= x;
        if r <= 0.0 {
            return i as u32;
        }
    }
    argmax(logits)
}

/// How repetitive a text is (Whisper's gzip ratio, approximated by repeated 4-grams).
fn compression_ratio(text: &str) -> f64 {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 16 {
        return 1.0;
    }
    let mut seen = std::collections::HashSet::new();
    let grams = chars.windows(4).count();
    for g in chars.windows(4) {
        seen.insert(g.to_vec());
    }
    grams as f64 / seen.len().max(1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_share_the_span_by_length() {
        let w = spread_words("Hola a todos", 1.0, 3.0);
        assert_eq!(w.len(), 3);
        assert_eq!(w[0].start, 1.0);
        assert!((w[2].end - 3.0).abs() < 1e-9);
        assert!(
            w[0].end - w[0].start > w[1].end - w[1].start,
            "longer words last longer"
        );
        assert!(spread_words("   ", 0.0, 1.0).is_empty());
    }

    #[test]
    fn repetition_is_detected() {
        assert!(compression_ratio("la la la la la la la la la la la la la la") > 2.4);
        assert!(compression_ratio("Hoy vamos a ver como editar un video rapidamente") < 1.5);
    }

    #[test]
    fn model_catalog() {
        assert_eq!(ModelSize::from_id("whisper-base"), Some(ModelSize::Base));
        let dir = tempfile::tempdir().unwrap();
        assert!(!ModelSize::Tiny.is_downloaded(dir.path()));
    }

    #[test]
    fn silence_is_measured_against_the_recording() {
        // ten seconds of speech-level tone, five of hiss
        let mut pcm: Vec<f32> = (0..160_000)
            .map(|i| (i as f32 * 0.07).sin() * 0.3)
            .collect();
        pcm.extend((0..80_000).map(|i| if i % 2 == 0 { 0.0002 } else { -0.0002 }));
        let level = silence_level(&pcm);
        assert!(level > rms(&pcm[170_000..178_000]));
        assert!(level < rms(&pcm[..8_000]));
        // a quiet recording is not all silence
        let quiet: Vec<f32> = pcm.iter().map(|v| v * 0.05).collect();
        assert!(silence_level(&quiet) < rms(&quiet[..8_000]));
        assert_eq!(silence_level(&[]), 0.0);
    }
}
