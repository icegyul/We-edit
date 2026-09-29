# Derived Premiere keyboard-locale inventory

**Source scope:** 41 `.kys` shortcut maps and 10 `.ksvlayout` keyboard-layout fragments.<br>
**Evidence level:** E3 for static files in the audited installation; dispatch behavior remains unvalidated.

## Shortcut-map corpus

The installation contains four named maps (Premiere Defaults, Premiere CS6, Avid MC 5 and Final Cut Pro 7.0) for `de`, `en`, `es`, `fr`, `it`, `ja`, `ko`, `pt`, `ru` and `zh`. The `nb` directory contains only Premiere Defaults, producing 41 files total.

The three legacy named maps are byte-identical across their locale directories. Premiere Defaults differs by locale.

### Premiere Defaults comparison

All files report shortcut schema version 5 and platform `windows`.

| Locale directory | Entries / unique context-command pairs | Missing vs `en` | Extra vs `en` | Different raw binding vs `en` |
|---|---:|---:|---:|---:|
| `en` | 370 | 0 | 0 | 0 |
| `de` | 331 | 39 | 0 | 34 |
| `es` | 331 | 39 | 0 | 34 |
| `fr` | 331 | 39 | 0 | 47 |
| `it` | 331 | 39 | 0 | 31 |
| `ja` | 332 | 38 | 0 | 13 |
| `ko` | 332 | 38 | 0 | 2 |
| `nb` | 308 | 62 | 0 | 1 |
| `pt` | 331 | 39 | 0 | 12 |
| `ru` | 332 | 38 | 0 | 29 |
| `zh` | 332 | 38 | 0 | 2 |

For the common 39-pair gap in several localized maps, the English-only pairs group as Titler 26, Capture 11, Global 1 and Project 1. This may reflect packaging/version skew or locale-specific feature exposure; it is not evidence that runtime commands are unavailable.

The English map's 370 entries use 243 character-domain virtual keys and 127 low-valued special-key codes. No numpad-domain binding occurs in that default map.

## Physical-layout fragments

`.ksvlayout` files are XML **fragments**, not well-formed single-root XML documents. Each begins with sibling elements such as `layout_name`, `layout_displayname`, version/margin and `layout_keyboard`. A safe parser must use fragment mode or add a synthetic root in memory after removing the XML declaration.

| File | Declared layout | Rows | Buttons | Unique codes | Modifier-key buttons | Expanded buttons | Spacer slots |
|---|---|---:|---:|---:|---:|---:|---:|
| `de.ksvlayout` | `de_DE` | 6 | 123 | 96 | 9 | 33 | 24 |
| `es.ksvlayout` | `es_ES` | 6 | 123 | 96 | 9 | 33 | 24 |
| `fr.ksvlayout` | `fr_FR` | 6 | 123 | 96 | 9 | 33 | 24 |
| `it.ksvlayout` | `it_IT` | 6 | 123 | 96 | 9 | 33 | 24 |
| `ja.ksvlayout` | `ja_JP` | 6 | 122 | 95 | 9 | 33 | 24 |
| `ko.ksvlayout` | `ko_KR` | 6 | 120 | 95 | 9 | 31 | 22 |
| `nb.ksvlayout` | `de_DE` | 6 | 123 | 95 | 9 | 33 | 24 |
| `pt.ksvlayout` | `pt_BR` | 6 | 124 | 97 | 9 | 33 | 24 |
| `ru.ksvlayout` | `ru_RU` | 6 | 123 | 96 | 9 | 33 | 24 |
| `sv.ksvlayout` | `sv_SE` | 6 | 123 | 96 | 9 | 33 | 24 |

`nb.ksvlayout` declaring `de_DE` is a source-data anomaly and must not be silently corrected. There are default shortcut maps without matching layout fragments (`en`, `zh`) and a layout fragment without a matching default-map directory (`sv`).

## Numeric key-code domains

The Spanish layout provides a representative split of 48 character-domain buttons, 16 numpad-domain buttons, 35 low-valued special-key buttons and 24 spacer slots.

Observed encoding, with semantics still requiring runtime confirmation:

- `0x80000000 | scalar`: character-domain code; low bits decode to the visible Unicode character in the layout, for example `Q`, `1`, `º` or `¡`;
- `0xC0000000 | scalar`: expanded/numpad-domain character, including digits and `/`, `*`, `-`, `+`, `.`, `=`;
- low values: special-key enum used for Space, Return, modifiers, arrows, function keys and related controls; exact mapping is not fully identified;
- `65535`: visual spacer sentinel in the layout.

The `.kys` `virtualkey` values use the same numeric domains. Modifiers remain separate booleans (`ctrl`, `alt`, `shift`). Command identity is the pair `(context, commandname)`; raw virtual key or localized character is not command identity.

Direct set-membership between a shortcut map and one visible layout layer is not a valid coverage test: localized maps can reference shifted or alternate character codes not present in the fragment's displayed layer. Runtime keyboard-layout and AltGr/dead-key tests remain mandatory.

## Corpus fingerprints

Fingerprints use the algorithm defined in the evidence manifest.

| Corpus | Files | Bytes | Manifest SHA-256 |
|---|---:|---:|---|
| all `.kys` maps | 41 | 3,045,521 | `534D20AF777349432DFB3BED58E3A468E62366BE56325DEA57A458D7EE3DF961` |
| localized Premiere Defaults | 11 | 988,031 | `56176D9D1952A3B7205659501A20A142CF1F2C73842E6C4C5077212DAC95BA9E` |
| physical-layout fragments | 10 | 201,837 | `21CB94F8BD830C988351C7B72778DAAD2D01A3C04D823C694F7B4FC4CD7CE278` |
