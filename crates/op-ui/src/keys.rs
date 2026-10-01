//! Keyboard: egui key events -> chords -> commands of the focused panel (KBD-002, KBD-003).

use egui::{Event, Key, Modifiers};
use op_application::{Chord, Focus};

use crate::app::State;

/// Key name used by bindings for an egui key.
fn key_name(k: Key) -> Option<&'static str> {
    use Key::*;
    Some(match k {
        A => "A",
        B => "B",
        C => "C",
        D => "D",
        E => "E",
        F => "F",
        G => "G",
        H => "H",
        I => "I",
        J => "J",
        K => "K",
        L => "L",
        M => "M",
        N => "N",
        O => "O",
        P => "P",
        Q => "Q",
        R => "R",
        S => "S",
        T => "T",
        U => "U",
        V => "V",
        W => "W",
        X => "X",
        Y => "Y",
        Z => "Z",
        Num0 => "0",
        Num1 => "1",
        Num2 => "2",
        Num3 => "3",
        Num4 => "4",
        Num5 => "5",
        Num6 => "6",
        Num7 => "7",
        Num8 => "8",
        Num9 => "9",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        ArrowUp => "Up",
        ArrowDown => "Down",
        Escape => "Escape",
        Tab => "Tab",
        Backspace => "Backspace",
        Enter => "Enter",
        Space => "Space",
        Insert => "Insert",
        Delete => "Delete",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Comma => ",",
        Period => ".",
        Semicolon | Colon => ";",
        Quote => "'",
        Slash | Questionmark => "/",
        Backslash | Pipe => "\\",
        OpenBracket | OpenCurlyBracket => "[",
        CloseBracket | CloseCurlyBracket => "]",
        Backtick => "`",
        Minus => "-",
        Equals | Plus => "=",
        F1 => "F1",
        F2 => "F2",
        F3 => "F3",
        F4 => "F4",
        F5 => "F5",
        F6 => "F6",
        F7 => "F7",
        F8 => "F8",
        F9 => "F9",
        F10 => "F10",
        F11 => "F11",
        F12 => "F12",
        _ => return None,
    })
}

fn chord(m: Modifiers, key: &str) -> Chord {
    Chord {
        ctrl: m.command || m.ctrl,
        shift: m.shift,
        alt: m.alt,
        key: key.to_string(),
    }
}

/// Symbols that only exist as the shifted form of another key.
fn is_shifted_symbol(k: Key) -> bool {
    matches!(
        k,
        Key::Colon
            | Key::Questionmark
            | Key::Pipe
            | Key::OpenCurlyBracket
            | Key::CloseCurlyBracket
            | Key::Plus
            | Key::Exclamationmark
    )
}

/// The chord of a key event. The logical key follows the keyboard layout; a shifted symbol
/// (":" for Shift+;) or a key without a name falls back to the key position, so bindings such
/// as `Shift+;` or `Shift+1` still match.
fn event_chord(key: Key, physical: Option<Key>, m: Modifiers) -> Option<Chord> {
    let k = if key_name(key).is_some() && !is_shifted_symbol(key) {
        key
    } else {
        physical.filter(|p| key_name(*p).is_some()).unwrap_or(key)
    };
    key_name(k).map(|n| chord(m, n))
}

/// Handles this frame's keyboard input. Text fields keep their keys.
pub fn handle(s: &mut State, ctx: &egui::Context) {
    // clicked buttons and panels keep egui's keyboard focus; only text fields should
    let typing = ctx.text_edit_focused();
    if !typing && let Some(id) = ctx.memory(|m| m.focused()) {
        ctx.memory_mut(|m| m.surrender_focus(id));
    }
    let events = ctx.input(|i| i.events.clone());
    let mut handled: Vec<(Modifiers, Key)> = Vec::new();
    let mut clipboard = false;
    for e in events {
        let c = match e {
            Event::Key {
                key,
                physical_key,
                pressed,
                modifiers,
                repeat,
            } => {
                if key == Key::K {
                    s.k_down = pressed;
                }
                // with only an image (or files) in the clipboard egui sends no paste event;
                // the release of Ctrl+V is the only trace of the shortcut
                if key == Key::V && !pressed && modifiers.command && !modifiers.shift && !typing {
                    if std::mem::take(&mut s.paste_seen) {
                        continue;
                    }
                    let found =
                        s.ed.keymap
                            .lookup(s.focus.context(), &Chord::parse("Ctrl+V").unwrap())
                            .or_else(|| {
                                s.ed.keymap.lookup(
                                    op_application::keymap::GLOBAL,
                                    &Chord::parse("Ctrl+V").unwrap(),
                                )
                            })
                            .map(str::to_string);
                    if found.as_deref() == Some("cmd.edit.paste") {
                        s.command("cmd.edit.paste");
                    }
                    continue;
                }
                if !pressed || typing {
                    continue;
                }
                // J/L while K is held steps one frame (KBD transport convention)
                if s.k_down && !modifiers.any() && (key == Key::J || key == Key::L) {
                    let m = s.focus.monitor();
                    s.ed.step(m, if key == Key::J { -1 } else { 1 });
                    handled.push((modifiers, key));
                    continue;
                }
                handled.push((modifiers, key));
                if repeat
                    && !matches!(
                        key,
                        Key::ArrowLeft
                            | Key::ArrowRight
                            | Key::ArrowUp
                            | Key::ArrowDown
                            | Key::Minus
                            | Key::Equals
                            | Key::Plus
                    )
                {
                    continue;
                }
                event_chord(key, physical_key, modifiers)
            }
            // egui turns these chords into clipboard events
            Event::Copy if !typing => {
                clipboard = true;
                Some(Chord::parse("Ctrl+C").unwrap())
            }
            Event::Cut if !typing => {
                clipboard = true;
                let m = ctx.input(|i| i.modifiers);
                Some(if m.shift && !m.ctrl {
                    Chord::parse("Shift+Delete").unwrap()
                } else {
                    Chord::parse("Ctrl+X").unwrap()
                })
            }
            Event::Paste(_) if !typing => {
                clipboard = true;
                s.paste_seen = true;
                let m = ctx.input(|i| i.modifiers);
                Some(if m.shift {
                    Chord::parse("Ctrl+Shift+V").unwrap()
                } else {
                    Chord::parse("Ctrl+V").unwrap()
                })
            }
            _ => None,
        };
        let Some(c) = c else { continue };
        if c.key == "Escape" && !c.ctrl && !c.shift && !c.alt {
            s.escape();
            continue;
        }
        let context = if s.focus == Focus::Other {
            op_application::keymap::GLOBAL
        } else {
            s.focus.context()
        };
        let found = s.ed.keymap.lookup(context, &c).map(str::to_string);
        log::trace!("{} in {context}: {found:?}", c.format());
        if let Some(cmd) = found {
            s.command(&cmd);
        }
    }
    // widgets drawn later in this frame must not act on keys that were shortcuts
    if !typing {
        ctx.input_mut(|i| {
            for (m, k) in handled {
                i.consume_key(m, k);
            }
            if clipboard {
                i.events
                    .retain(|e| !matches!(e, Event::Copy | Event::Cut | Event::Paste(_)));
            }
        });
    }
}

/// Formats a chord for display with the platform's modifier names.
pub fn display(keys: &str) -> String {
    let text = Chord::parse(keys).map(|c| c.format()).unwrap_or_default();
    if cfg!(target_os = "macos") {
        // Ctrl in a binding is the Command key on a Mac
        text.replace("Ctrl+", "Cmd+").replace("Alt+", "Option+")
    } else {
        text
    }
}

/// The chord for a key pressed in the shortcut editor.
pub fn capture(ctx: &egui::Context) -> Option<Chord> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            Event::Key {
                key,
                physical_key,
                pressed: true,
                modifiers,
                ..
            } => event_chord(*key, *physical_key, *modifiers),
            Event::Copy => Some(Chord::parse("Ctrl+C").unwrap()),
            Event::Cut => Some(Chord::parse("Ctrl+X").unwrap()),
            Event::Paste(_) => Some(Chord::parse("Ctrl+V").unwrap()),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords_from_events() {
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        // Shift+; arrives as ':' on US layouts; the physical key keeps the binding working
        assert_eq!(
            event_chord(Key::Colon, Some(Key::Semicolon), shift)
                .unwrap()
                .format(),
            "Shift+;"
        );
        assert_eq!(
            event_chord(Key::K, Some(Key::K), Modifiers::COMMAND)
                .unwrap()
                .format(),
            "Ctrl+K"
        );
        assert_eq!(
            event_chord(Key::Comma, None, Modifiers::NONE)
                .unwrap()
                .format(),
            ","
        );
        assert_eq!(
            event_chord(Key::Exclamationmark, Some(Key::Num1), shift)
                .unwrap()
                .format(),
            "Shift+1"
        );
        // arrows sent with a numeric keypad position keep their meaning
        assert_eq!(
            event_chord(Key::ArrowRight, Some(Key::Num6), Modifiers::NONE)
                .unwrap()
                .format(),
            "Right"
        );
    }
}
