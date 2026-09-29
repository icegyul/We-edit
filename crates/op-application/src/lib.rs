//! Application services (docs/architecture.md 3): the editor state and its commands, the media
//! service, playback, export, autosave and preferences. The UI is a thin layer over `Editor`.

pub mod autosave;
pub mod commands;
pub mod editor;
pub mod export;
pub mod keymap;
pub mod media;
pub mod merge;
pub mod prefs;
pub mod session;

pub use editor::{APP_NAME, Editor, Status, Transport};
pub use export::{ExportJob, ExportSettings, Progress};
pub use keymap::{Binding, Chord, Keymap};
pub use media::MediaService;
pub use prefs::{Dirs, Preferences};
pub use session::*;
