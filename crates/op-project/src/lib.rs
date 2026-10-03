//! Project files and interchange (docs/data-model.md 9).
//!
//! * `native`: the `.opproj` project format (read/write, atomic save).
//! * `prproj`: read-only import of Premiere Pro project files. Writing that format is not
//!   supported until round-trip conformance is validated (PR-SAVE-002).
//! * `otio`, `fcpxml`, `edl`: timeline interchange.

#![forbid(unsafe_code)]

pub mod edl;
pub mod fcpxml;
pub mod native;
pub mod otio;
pub mod prproj;
pub mod xml;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Format(String),
    #[error("the project was saved by a newer version (format {0})")]
    Version(u32),
    #[error("the project is inconsistent: {0}")]
    Invalid(op_core::EditError),
}
