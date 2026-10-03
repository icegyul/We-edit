# Keyboard shortcuts

**Updated:** 2026-10-02 (version 0.7.0)

The default shortcuts use the keys Premiere Pro documents for its default layout, so editors coming
from it can work without relearning. The map is program data
(`crates/op-application/data/default_shortcuts.json`) of `(context, command, keys)` entries.

## Rules

- **KBD-001:** Bindings are stored against logical commands, not hard-coded in tools.
- **KBD-002:** Global and panel contexts are distinct; a shortcut is looked up in the focused
  panel first.
- **KBD-003:** A focused panel's binding wins over a global one with the same keys.
- **KBD-004:** User changes are a separate layer; the defaults are never modified.
- **KBD-008:** Commands are identified by `(context, command)`, so menus, shortcuts and imported
  maps address the same command.
- Text fields keep their keys while they have the focus; shortcuts do not fire while typing.

## Customizing

Edit > Keyboard Shortcuts changes any binding and can import a `.kys` file exported from Premiere
Pro. On macOS, Ctrl in a binding is the Command key and Alt is Option.
