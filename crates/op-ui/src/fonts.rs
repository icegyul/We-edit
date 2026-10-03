//! The interface typeface: the system's own interface font (Segoe UI on Windows, San Francisco
//! on macOS, the desktop's sans-serif on Linux), read from the system and never shipped; egui's
//! bundled font when it cannot be read or "Classic" is chosen in Preferences.

use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily};

/// Semibold text for headings and panel titles.
pub fn strong() -> FontFamily {
    FontFamily::Name("strong".into())
}

/// A font file the interface may use, checked before egui gets it (egui stops on a font it
/// cannot read).
fn checked(data: Vec<u8>, index: u32) -> Option<FontData> {
    ab_glyph::FontRef::try_from_slice_and_index(&data, index).ok()?;
    let mut f = FontData::from_owned(data);
    f.index = index;
    Some(f)
}

fn read(path: PathBuf) -> Option<FontData> {
    checked(std::fs::read(path).ok()?, 0)
}

/// Regular and semibold faces of the system's interface font.
fn system_faces() -> Option<(FontData, Option<FontData>)> {
    if cfg!(windows) {
        let dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
        let regular = read(dir.join("segoeui.ttf"))?;
        return Some((regular, read(dir.join("seguisb.ttf"))));
    }
    if cfg!(target_os = "macos") {
        let regular = read(PathBuf::from("/System/Library/Fonts/SFNS.ttf"))
            .or_else(|| read(PathBuf::from("/System/Library/Fonts/Helvetica.ttc")))?;
        return Some((regular, None));
    }
    // Linux and others: the first of the usual desktop families that is installed
    let fonts = op_render::text::Fonts::global();
    for family in [
        "Inter",
        "Noto Sans",
        "Cantarell",
        "Ubuntu",
        "Open Sans",
        "DejaVu Sans",
    ] {
        if let Some((data, index)) = fonts.face_data(family, false)
            && let Some(regular) = checked(data, index)
        {
            let semibold = fonts
                .face_data(family, true)
                .and_then(|(d, i)| checked(d, i));
            return Some((regular, semibold));
        }
    }
    None
}

/// Installs the interface fonts; `system` false keeps egui's own.
pub fn install(ctx: &egui::Context, system: bool) {
    let mut defs = FontDefinitions::default();
    let base = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut strong_list = base.clone();
    if system && let Some((regular, semibold)) = system_faces() {
        defs.font_data.insert("ui".into(), Arc::new(regular));
        if let Some(list) = defs.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "ui".into());
        }
        strong_list.insert(0, "ui".into());
        if let Some(sb) = semibold {
            defs.font_data.insert("ui-strong".into(), Arc::new(sb));
            strong_list.insert(0, "ui-strong".into());
        }
        log::info!("interface font: system");
    }
    defs.families.insert(strong(), strong_list);
    ctx.set_fonts(defs);
}
