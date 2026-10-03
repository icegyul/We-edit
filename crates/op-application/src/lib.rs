//! Application services (docs/architecture.md 3): the editor state and its commands, the media
//! service, playback, export, autosave and preferences. The UI is a thin layer over `Editor`.

pub mod autosave;
pub mod captions;
pub mod commands;
pub mod editor;
pub mod export;
pub mod keymap;
pub mod logging;
pub mod media;
pub mod merge;
pub mod performance;
pub mod prefs;
pub mod proxies;
pub mod recovery;
pub mod relink;
pub mod session;

pub use editor::{APP_NAME, Editor, LoadedProject, Placement, Status, Transport};
pub use export::{ExportJob, ExportSettings, PreviewImage, Progress};
pub use keymap::{Binding, Chord, Keymap};
pub use media::MediaService;
pub use prefs::{Dirs, Preferences};
pub use session::*;
