//! Keyboard shortcuts (docs/keyboard-shortcuts.md).
//!
//! Bindings are stored against logical commands identified by `(context, command)` (KBD-008).
//! The default map (data/default_shortcuts.json) uses the keys Premiere Pro documents for its
//! default layout, so the shortcuts match what editors coming from it already know. A focused panel's binding wins over a global one
//! (KBD-003). User changes are a separate named layer; the defaults are never modified (KBD-004).

use serde::{Deserialize, Serialize};

const DEFAULTS: &str = include_str!("../data/default_shortcuts.json");

/// Contexts that correspond to panels of this application.
pub const GLOBAL: &str = "global";
pub const TIMELINE: &str = "timeline";
pub const PROGRAM: &str = "program.monitor";
pub const SOURCE: &str = "source.monitor";
pub const PROJECT: &str = "project";
pub const EFFECT_CONTROLS: &str = "effectcontrols";
pub const EFFECTS: &str = "effects";
pub const HISTORY: &str = "history";

/// A key with modifiers, e.g. "Ctrl+Shift+K".
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Key name: a character (",", "A", "1") or a named key ("Space", "Left", "F5", "Delete").
    pub key: String,
}

impl Chord {
    pub fn parse(text: &str) -> Option<Chord> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let mut c = Chord {
            ctrl: false,
            shift: false,
            alt: false,
            key: String::new(),
        };
        // the key itself may be "+" or "-"; split on '+' only between parts
        let mut parts: Vec<&str> = text.split('+').collect();
        if text.ends_with("++") || text == "+" {
            parts.retain(|p| !p.is_empty());
            parts.push("+");
        }
        let key = parts.pop()?;
        for m in parts {
            match m.trim().to_ascii_lowercase().as_str() {
                "ctrl" | "cmd" | "control" => c.ctrl = true,
                "shift" => c.shift = true,
                "alt" | "opt" | "option" => c.alt = true,
                "" => {}
                _ => return None,
            }
        }
        c.key = normalize_key(key.trim());
        (!c.key.is_empty()).then_some(c)
    }

    pub fn format(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        s.push_str(&self.key);
        s
    }
}

fn normalize_key(k: &str) -> String {
    if k.chars().count() == 1 {
        return k.to_uppercase();
    }
    let lower = k.to_ascii_lowercase();
    let named = [
        "Space",
        "Enter",
        "Delete",
        "Backspace",
        "Tab",
        "Escape",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Up",
        "Down",
        "Left",
        "Right",
        "Insert",
    ];
    for n in named {
        if n.to_ascii_lowercase() == lower {
            return n.to_string();
        }
    }
    match lower.as_str() {
        "return" => "Enter".into(),
        "del" => "Delete".into(),
        "esc" => "Escape".into(),
        "pgup" => "PageUp".into(),
        "pgdn" => "PageDown".into(),
        _ if lower.starts_with('f') && lower[1..].parse::<u8>().is_ok() => lower.to_uppercase(),
        _ if lower.starts_with("num") => k.to_string(),
        _ => k.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub context: String,
    pub command: String,
    pub keys: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
}

impl Keymap {
    /// The default map.
    pub fn defaults() -> Keymap {
        let bindings: Vec<Binding> = serde_json::from_str(DEFAULTS).unwrap_or_default();
        let mut map = Keymap { bindings };
        // the source monitor shares the program monitor's panel commands
        let extra: Vec<Binding> = map
            .bindings
            .iter()
            .filter(|b| b.context == PROGRAM)
            .map(|b| Binding {
                context: SOURCE.into(),
                command: b.command.clone(),
                keys: b.keys.clone(),
            })
            .collect();
        map.bindings.extend(extra);
        map
    }

    /// Defaults plus user changes: (context, command, keys); empty keys remove every binding of
    /// that command in that context.
    pub fn with_overrides(overrides: &[(String, String, String)]) -> Keymap {
        let mut map = Keymap::defaults();
        for (ctx, cmd, keys) in overrides {
            map.bindings
                .retain(|b| !(b.context == *ctx && b.command == *cmd));
            if !keys.is_empty() {
                map.bindings.push(Binding {
                    context: ctx.clone(),
                    command: cmd.clone(),
                    keys: keys.clone(),
                });
            }
        }
        map
    }

    /// The command for a chord: the focused panel's binding first, then the global one.
    pub fn lookup(&self, context: &str, chord: &Chord) -> Option<&str> {
        let find = |ctx: &str| {
            self.bindings
                .iter()
                .find(|b| b.context == ctx && Chord::parse(&b.keys).as_ref() == Some(chord))
                .map(|b| b.command.as_str())
        };
        find(context).or_else(|| find(GLOBAL))
    }

    /// Display text of the first global (or any) binding of a command, for menus.
    pub fn keys_for(&self, command: &str) -> Option<String> {
        self.bindings
            .iter()
            .filter(|b| b.command == command)
            .min_by_key(|b| b.context != GLOBAL)
            .and_then(|b| Chord::parse(&b.keys))
            .map(|c| c.format())
    }

    /// Bindings sharing the same chord in the same context (shown in the shortcut editor).
    pub fn conflicts(&self) -> Vec<(String, Chord, Vec<String>)> {
        let mut seen: std::collections::HashMap<(String, Chord), Vec<String>> =
            std::collections::HashMap::new();
        for b in &self.bindings {
            if let Some(c) = Chord::parse(&b.keys) {
                seen.entry((b.context.clone(), c))
                    .or_default()
                    .push(b.command.clone());
            }
        }
        let mut v: Vec<(String, Chord, Vec<String>)> = seen
            .into_iter()
            .filter(|(_, cmds)| cmds.len() > 1)
            .map(|((ctx, c), cmds)| (ctx, c, cmds))
            .collect();
        v.sort_by(|a, b| (a.0.as_str(), a.1.format()).cmp(&(b.0.as_str(), b.1.format())));
        v
    }
}

/// Windows virtual-key code -> key name used in bindings.
pub fn virtual_key_name(vk: u32) -> Option<String> {
    Some(match vk {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk)?.to_string(),
        0x08 => "Backspace".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x1B => "Escape".into(),
        0x20 => "Space".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        0xBA => ";".into(),
        0xBB => "=".into(),
        0xBC => ",".into(),
        0xBD => "-".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0xC0 => "`".into(),
        0xDB => "[".into(),
        0xDC => "\\".into(),
        0xDD => "]".into(),
        0xDE => "'".into(),
        _ => return None,
    })
}

/// Reads a keyboard shortcut file exported by Premiere Pro (`.kys`): XML with one element per
/// context holding items of command name, modifiers and virtual key.
pub fn import_kys(text: &str) -> Result<Vec<Binding>, String> {
    let doc = op_project::xml::parse(text, op_project::xml::Limits::default())
        .map_err(|e| e.to_string())?;
    let shortcuts = doc
        .child(0, "shortcuts")
        .ok_or("not a keyboard shortcut file")?;
    let mut out = Vec::new();
    for ctx in doc.children(shortcuts).collect::<Vec<_>>() {
        let Some(context) = doc.name(ctx).strip_prefix("context.") else {
            continue;
        };
        for item in doc
            .children(ctx)
            .filter(|i| doc.name(*i).starts_with("item."))
            .collect::<Vec<_>>()
        {
            let Some(command) = doc.text_at(item, "commandname") else {
                continue;
            };
            let Some(vk) = doc
                .text_at(item, "virtualkey")
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let Some(key) = virtual_key_name(vk) else {
                continue;
            };
            let flag = |n: &str| doc.text_at(item, n) == Some("true");
            let chord = Chord {
                ctrl: flag("modifier.ctrl"),
                shift: flag("modifier.shift"),
                alt: flag("modifier.alt"),
                key,
            };
            out.push(Binding {
                context: context.to_string(),
                command: command.to_string(),
                keys: chord.format(),
            });
        }
    }
    if out.is_empty() {
        return Err("the file has no shortcuts".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_documented_map() {
        let k = Keymap::defaults();
        assert!(k.bindings.len() >= 370);
        let c = |s: &str| Chord::parse(s).unwrap();
        assert_eq!(
            k.lookup(TIMELINE, &c("Ctrl+K")),
            Some("cmd.sequence.razorateditline")
        );
        assert_eq!(k.lookup(TIMELINE, &c(",")), Some("cmd.clip.insert"));
        assert_eq!(
            k.lookup(TIMELINE, &c("Shift+Delete")),
            Some("cmd.edit.rippledelete")
        );
        // panel binding beats the global one
        assert_eq!(
            k.lookup(TIMELINE, &c("Ctrl+V")),
            Some("cmd.timeline.paste.to.same.track")
        );
        assert_eq!(k.lookup(PROJECT, &c("Ctrl+V")), Some("cmd.edit.paste"));
        assert_eq!(k.keys_for("cmd.edit.undo").as_deref(), Some("Ctrl+Z"));
    }

    #[test]
    fn chords_round_trip() {
        for s in [
            "Ctrl+Shift+Alt+M",
            "Shift+/",
            ",",
            "Space",
            "Shift+=",
            "F1",
            "Ctrl+`",
        ] {
            assert_eq!(Chord::parse(s).unwrap().format(), s);
        }
        assert_eq!(Chord::parse("ctrl+k").unwrap().format(), "Ctrl+K");
    }

    #[test]
    fn overrides_and_kys_import() {
        let k =
            Keymap::with_overrides(&[("global".into(), "cmd.edit.undo".into(), "Ctrl+U".into())]);
        assert_eq!(k.keys_for("cmd.edit.undo").as_deref(), Some("Ctrl+U"));
        let kys = r#"<?xml version="1.0"?><PremiereData Version="3"><shortcuts Version="4"><context.global Version="1">
            <item.0 Version="1"><commandname>cmd.edit.undo</commandname><modifier.shift>false</modifier.shift>
            <modifier.alt>false</modifier.alt><modifier.ctrl>true</modifier.ctrl><virtualkey>90</virtualkey></item.0>
            <itemcount>1</itemcount></context.global></shortcuts></PremiereData>"#;
        let b = import_kys(kys).unwrap();
        assert_eq!(
            b,
            vec![Binding {
                context: "global".into(),
                command: "cmd.edit.undo".into(),
                keys: "Ctrl+Z".into()
            }]
        );
    }
}
