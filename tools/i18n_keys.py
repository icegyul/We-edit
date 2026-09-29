"""Lists every user-visible English string that the Spanish catalog must translate.

Usage: python tools/i18n_keys.py [--missing]
With --missing, prints only the strings not yet in crates/op-ui/src/i18n/es.rs.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
UI = ROOT / "crates/op-ui/src"
APP = ROOT / "crates/op-application/src"
CORE = ROOT / "crates/op-core/src/catalog.rs"

S = r'"((?:[^"\\]|\\.)*)"'
keys = []


def add(s):
    # identifiers, format fragments and fractions are not interface text
    if not s or re.fullmatch(r"[a-z]+|1/\S+", s) or "{base}" in s or s.startswith(("cmd.", "op.", "uif.")):
        return
    if s not in keys:
        keys.append(s)


def scan(text, patterns):
    for pat in patterns:
        for m in re.finditer(pat, text, re.S):
            add(m.group(1))


ui_patterns = [
    r"\bt\(" + S + r"\)",
    r"\btf\(" + S,
    r"\btn\(" + S + r"\)",
    r"\bitem(?:_if)?\(ui, s, " + S,
    r"\bcheck\(ui, s, " + S,
    r"\brun\(s, ui, " + S,
    r"\bentry\(s, ui, " + S,
    r"\bb\(ui, s, Icon::\w+, " + S,
    r"\.(?:edit|seq_edit|edit_merge)\(\s*" + S,
    r"toggle_track\(s, sid, row\.r, " + S,
]
for f in sorted(UI.rglob("*.rs")):
    if f.name == "es.rs":
        continue
    text = f.read_text(encoding="utf-8")
    scan(text, ui_patterns)
    # string tables whose entries go through t(): titles, names, presets, menus
    for fn in ["title", "tool_name", "label_name", "interp_label", "scope_name", "context_name", "res_label", "label"]:
        for m in re.finditer(r"fn " + fn + r"\([^)]*\)[^{]*\{(.*?)\n\}", text, re.S):
            for s in re.findall(S, m.group(1)):
                add(s)
    for block in re.findall(r"const NAMES: &\[\(&str, &str\)\] = &\[(.*?)\];", text, re.S):
        for s in re.findall(r"\(" + S + r", " + S + r"\)", block):
            add(s[1])
    for block in re.findall(r"const EXPORT_PRESETS: &\[ExportPreset\] = &\[(.*?)\];", text, re.S):
        for s in re.findall(r"name: " + S, block):
            add(s)
    for block in re.findall(r"const GROUPS: .*? = \[(.*?)\];", text, re.S):
        for s in re.findall(S, block):
            add(s)
    for s in re.findall(r"\(\d+u?\d*, " + S + r"\)", text):
        add(s)
    for s in re.findall(r"\(TimeDisplay::\w+, " + S + r"\)", text):
        add(s)
    for s in re.findall(r"\(\(\d, \d\), " + S + r"\)", text):
        add(s)
    for s in re.findall(r"\(Alignment::\w+, " + S + r"\)", text):
        add(s)
    for s in re.findall(r"\(" + S + r", catalog::(?:TEXT|SHAPE), ", text):
        add(s)

app_patterns = [
    r"\.(?:edit|seq_edit|edit_merge|with_selection)\(\s*" + S,
    r"self\.(?:info|error)\(" + S + r"\)",
    r"self\.info\(if on \{ " + S + r" \} else \{ " + S,
]
for f in sorted(APP.rglob("*.rs")):
    text = f.read_text(encoding="utf-8")
    scan(text, app_patterns)
    for m in re.finditer(r"self\.info\(if on \{ " + S + r" \} else \{ " + S, text):
        add(m.group(2))

cat = CORE.read_text(encoding="utf-8")
for m in re.finditer(r"def!\([^,]+, " + S + r", \w+, (\w+|\"[^\"]*\")", cat):
    add(m.group(1))
consts = dict(re.findall(r"pub const (CAT_\w+): &str = " + S + ";", cat))
for c in consts.values():
    add(c)
for m in re.finditer(r"\b[a-z_0-9]+\(\s*" + S + r",\s*" + S, cat):
    add(m.group(2))
for m in re.finditer(r"label: " + S, cat):
    add(m.group(1))
for m in re.finditer(r"\bg\(.*?, (\w+|" + S + r")\)", cat):
    pass
for name in re.findall(r"const (LUMETRI_\w+|GROUP_\w+): &str = " + S + ";", cat):
    add(name[1])
for g in re.findall(r"\), " + S + r"\)", cat):
    add(g)
for block in re.findall(r"pub const \w+: &\[&str\] =\s*&\[(.*?)\];", cat, re.S):
    for s in re.findall(S, block):
        add(s)
for block in re.findall(r"(?:static|const) \w+: \[&str; \d+\] = \[(.*?)\];", cat, re.S):
    for s in re.findall(S, block):
        add(s)

extra = ["Undo {}", "Redo {}", "Saved {}", "Imported {} files", "{} media files are offline", "Add {}"]
for e in extra:
    add(e)

if "--missing" in sys.argv:
    es = (UI / "i18n/es.rs").read_text(encoding="utf-8")
    have = set(re.findall(r'\(\s*' + S + r',', es))
    keys = [k for k in keys if k not in have]
for k in keys:
    print(k)
