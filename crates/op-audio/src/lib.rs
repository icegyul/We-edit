//! Audio engine (docs/architecture.md 6): sample-accurate mixing of the sequence model, audio
//! effects and transitions, meters, and device output with the playback clock.

#![deny(unsafe_code)]

pub mod device;
pub mod dsp;
pub mod effects;
pub mod engine;
pub mod mixer;

pub use device::Output;
pub use engine::{Playback, render_range};
pub use mixer::{AudioSource, Meters, Mixer, NoAudio};
