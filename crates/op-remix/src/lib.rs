//! Music remix (the Remix of Premiere's Essential Sound): a longer or shorter version of a piece
//! of music made by repeating or skipping the parts where the music repeats itself, so the
//! result keeps its natural flow. Pure Rust, no models to download.
//!
//! The repeat finder follows PyMusicLooper by arkrow (MIT license) and librosa's feature
//! choices (ISC license); see the NOTICE of the program.

mod beats;
mod dsp;
mod loops;
mod plan;

pub use dsp::{Cancelled, HOP, RATE};
pub use loops::{Analysis, AnalyzeError, Loop, analyze};
pub use plan::{Move, Plan, PlanOptions, plan};
