//! The interface typeface. Inter (included; open source under the SIL Open Font License, and the
//! free typeface closest to Apple's San Francisco) by default; or the system's own interface font
//! (Segoe UI on Windows, San Francisco on macOS, the desktop's sans-serif on Linux), any installed
//! font, any font file the user picks, or egui's bundled "Classic" font.

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

static INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

/// The faces of an installed family, by name.
fn family_faces(name: &str) -> Option<(FontData, Option<FontData>)> {
    let fonts = op_render::text::Fonts::global();
    let (data, index) = fonts.face_data(name, false)?;
    let regular = checked(data, index)?;
    let semibold = fonts.face_data(name, true).and_then(|(d, i)| checked(d, i));
    Some((regular, semibold))
}

/// The faces a preference value asks for: "inter" (included, close to San Francisco), "system",
/// "family:<name>" (any installed font), "file:<path>" (any .ttf or .otf file); None for
/// "classic" or when the choice cannot be read.
fn faces(choice: &str) -> Option<(FontData, Option<FontData>)> {
    match choice {
        "classic" => None,
        "system" => system_faces(),
        // Inter has a taller x-height and wider letters than the system fonts: a little smaller
        // it takes the room the panels were laid out for
        "inter" | "" => {
            let tweak = |f: FontData| {
                f.tweak(egui::FontTweak {
                    scale: 0.92,
                    ..Default::default()
                })
            };
            Some((
                tweak(checked(INTER_REGULAR.to_vec(), 0)?),
                checked(INTER_SEMIBOLD.to_vec(), 0).map(tweak),
            ))
        }
        c => {
            if let Some(name) = c.strip_prefix("family:") {
                family_faces(name)
            } else if let Some(path) = c.strip_prefix("file:") {
                read(PathBuf::from(path)).map(|f| (f, None))
            } else {
                None
            }
        }
    }
}

/// A font with Hangul, for Korean text in panels.
fn korean_fallback() -> Option<FontData> {
    if cfg!(windows) {
        let dir = PathBuf::from(std::env::var_os("WINDIR").unwrap_or_else(|| "C:\\Windows".into()))
            .join("Fonts");
        return read(dir.join("malgun.ttf"));
    }
    op_render::text::Fonts::global()
        .face_data("Noto Sans CJK KR", false)
        .and_then(|(d, i)| checked(d, i))
}

/// A short name for a preference value, for the Preferences combo.
pub fn label(choice: &str) -> String {
    if let Some(name) = choice.strip_prefix("family:") {
        name.to_string()
    } else if let Some(path) = choice.strip_prefix("file:") {
        PathBuf::from(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string())
    } else {
        String::new()
    }
}

/// Installs the interface font `choice` (see `faces`); egui's own font stays behind it for the
/// characters it lacks. Returns false when the choice could not be read (Inter is used).
pub fn install(ctx: &egui::Context, choice: &str) -> bool {
    let mut defs = FontDefinitions::default();
    let base = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut strong_list = base.clone();
    let wanted = faces(choice);
    let ok = wanted.is_some() || choice == "classic";
    let used = if choice == "classic" {
        None
    } else {
        wanted.or_else(|| faces("inter"))
    };
    if let Some((regular, semibold)) = used {
        defs.font_data.insert("ui".into(), Arc::new(regular));
        if let Some(list) = defs.families.get_mut(&FontFamily::Proportional) {
            list.insert(0, "ui".into());
        }
        strong_list.insert(0, "ui".into());
        if let Some(sb) = semibold {
            defs.font_data.insert("ui-strong".into(), Arc::new(sb));
            strong_list.insert(0, "ui-strong".into());
        }
        log::info!("interface font: {choice}");
    }
    // egui's and Inter's glyphs have no Hangul: Malgun Gothic (Windows) stands behind them
    if let Some(ko) = korean_fallback() {
        defs.font_data.insert("ko".into(), Arc::new(ko));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(list) = defs.families.get_mut(&family) {
                list.push("ko".into());
            }
        }
        strong_list.push("ko".into());
    }
    defs.families.insert(strong(), strong_list);
    ctx.set_fonts(defs);
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inter_is_included_and_any_font_file_can_be_added() {
        // the included typeface reads, with its semibold
        let (_, semibold) = faces("inter").expect("Inter");
        assert!(semibold.is_some());
        assert!(faces("classic").is_none());
        assert!(faces("file:/does/not/exist.ttf").is_none());
        // a font file added by the user shows up in every font menu and can be chosen
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Mine.ttf");
        std::fs::write(&file, INTER_REGULAR).unwrap();
        let fonts = op_render::text::Fonts::global();
        let names = fonts.add_file(&file).expect("a font");
        assert!(names.iter().any(|n| n == "Inter"), "{names:?}");
        assert!(fonts.families().iter().any(|n| n == "Inter"));
        assert!(faces("family:Inter").is_some());
        // something that is not a font is refused
        let junk = dir.path().join("junk.ttf");
        std::fs::write(&junk, b"not a font").unwrap();
        assert!(fonts.add_file(&junk).is_none());
        assert_eq!(label("family:Inter"), "Inter");
        assert_eq!(label(&format!("file:{}", file.display())), "Mine.ttf");
    }
}
