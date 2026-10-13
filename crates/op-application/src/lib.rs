//! Application services (docs/architecture.md 3): the editor state and its commands, the media
//! service, playback, export, autosave and preferences. The UI is a thin layer over `Editor`.

pub mod audio_sync;
pub mod autosave;
pub mod clipsync;
pub mod comfy;
pub mod comfy_packs;
pub mod download;
pub mod caption_designs;
pub mod captions;
pub mod cloud_images;
pub mod generative;
pub mod secrets;
pub mod commands;
pub mod editor;
pub mod effect_preview;
pub mod effects_clipboard;
pub mod export;
pub mod keymap;
pub mod logging;
pub mod media;
pub mod motion_templates;
pub mod merge;
pub mod performance;
pub mod prefs;
pub mod proxies;
pub mod recovery;
pub mod relink;
pub mod remix;
pub mod sentences;
pub mod whisper_models;
pub mod session;
pub mod shorts;
pub mod sound_library;
pub mod silence;
pub mod speech;

pub use editor::{APP_NAME, Editor, LoadedProject, Placement, Status, Transport};
pub use export::{ExportJob, ExportSettings, PreviewImage, Progress};
pub use keymap::{Binding, Chord, Keymap};
pub use media::MediaService;
pub use prefs::{Dirs, Preferences};
pub use session::*;
