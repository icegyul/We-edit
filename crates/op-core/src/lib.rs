//! OpenPremier canonical model.
//!
//! Pure, deterministic Rust with no filesystem, codec, GPU, audio-device or GUI dependency
//! (docs/architecture.md 2). Everything that changes a project goes through
//! [`Project::transact`], which validates invariants before committing.

#![forbid(unsafe_code)]

pub mod captions;
pub mod catalog;
pub mod color;
pub mod history;
pub mod ids;
pub mod media;
pub mod model;
pub mod params;
pub mod plan;
pub mod presets;
pub mod time;
pub mod timecode;
pub mod validate;

pub use color::{Label, MarkerColor, Rgba};
pub use history::{Changes, History};
pub use ids::*;
pub use media::*;
pub use model::*;
pub use params::{Ease, Interp, InterpChoice, Keyframe, Param, SpatialInterp, Value};
pub use time::*;
pub use timecode::{Parsed, TimeDisplay, TimecodeFormat};
pub use validate::{EditError, EditResult};
