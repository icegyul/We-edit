//! One front door for speech recognition. When the speech helper is installed (faster-whisper on
//! the NVIDIA graphics card via CUDA, any size up to large-v3) it does the work; without it the
//! built-in Whisper (tiny, base, small) runs on the processor. Remove Silence, automatic captions
//! and the sentence editor all come through here, so they are as fast as the PC allows.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use op_core::captions::TimedWord;
use parking_lot::Mutex;

use crate::captions::{CaptionProgress, CaptionStage, TranscribeOptions, ensure_model, transcribe_words};
use crate::sentences::{helper_files, run_helper};
use crate::whisper_models;

/// The speech helper with one installed model.
#[derive(Clone, Debug)]
pub struct Engine {
    pub python: PathBuf,
    pub script: PathBuf,
    pub model: PathBuf,
    /// Model id, like `large-v3`.
    pub id: String,
}

/// The helper with model `id`. Err says what is missing.
pub fn helper_engine(id: &str) -> Result<Engine, String> {
    let (python, script) = helper_files()
        .ok_or("음성 인식 도우미(helper 폴더)를 찾을 수 없습니다")?;
    if !whisper_models::installed(id) {
        return Err(format!("위스퍼 모델 '{id}'가 설치되어 있지 않습니다: 먼저 내려받으세요"));
    }
    let model = whisper_models::dir_of(id).ok_or("모델 폴더를 찾을 수 없습니다")?;
    Ok(Engine { python, script, model, id: id.to_string() })
}

/// The engine to use for model `id`: Some when the helper is there (Err when it is but the model
/// is not installed), None when only the built-in processor Whisper is available.
pub fn pick_engine(id: &str) -> Result<Option<Engine>, String> {
    if helper_files().is_none() {
        return Ok(None);
    }
    helper_engine(id).map(Some)
}

/// Same as `pick_engine`, but a missing model falls back to the best installed one, and to the
/// built-in Whisper when there is none.
pub fn best_engine(preferred: &str) -> Option<Engine> {
    helper_files()?;
    let dir = whisper_models::chosen_dir(preferred)?;
    let id = whisper_models::MODELS.iter().find(|m| whisper_models::dir_of(m.id).as_deref() == Some(dir.as_path()))?.id;
    helper_engine(id).ok()
}

/// The words of 16 kHz mono speech with their times (seconds from the start of `pcm`), and the
/// language heard. `engine` None uses the built-in Whisper with `opts.model`.
pub(crate) fn words(
    engine: Option<&Engine>,
    pcm: &[f32],
    models: &Path,
    opts: &TranscribeOptions,
    scratch: &Path,
    progress: &Arc<Mutex<CaptionProgress>>,
    cancel: &Arc<AtomicBool>,
) -> Result<(String, Vec<TimedWord>), String> {
    let Some(e) = engine else {
        ensure_model(models, opts.model, progress, cancel)?;
        return transcribe_words(pcm, models, opts, progress, cancel);
    };
    *progress.lock() = CaptionProgress { stage: CaptionStage::Transcribing, fraction: 0.0 };
    let _ = std::fs::create_dir_all(scratch);
    let started = std::time::Instant::now();
    let language = opts.language.clone().unwrap_or_default();
    let (lang, sentences) = run_helper(
        &e.python,
        &e.script,
        pcm,
        &language,
        &e.model,
        opts.verbatim,
        opts.task == crate::captions::Task::Translate,
        scratch,
        progress,
        cancel,
    )?;
    log::info!(
        "transcribed {:.0} s of audio ({lang}) with {} on the graphics card in {:.1} s",
        pcm.len() as f64 / 16_000.0,
        e.id,
        started.elapsed().as_secs_f32()
    );
    let words: Vec<TimedWord> = sentences.into_iter().flat_map(|s| s.words).collect();
    Ok((lang, words))
}
