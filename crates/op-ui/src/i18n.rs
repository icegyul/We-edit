//! Interface languages. English text is the key; other languages map it to their own text and
//! fall back to English for anything not translated yet.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

mod es;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Es];

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Es => "es",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Es => "Español",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "en" => Some(Lang::En),
            "es" => Some(Lang::Es),
            _ => None,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| es::ES.iter().copied().collect())
}

pub fn set(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 {
        Lang::Es
    } else {
        Lang::En
    }
}

/// The system language when supported, else English.
pub fn system() -> Lang {
    sys_locale::get_locale()
        .and_then(|l| Lang::from_code(&l))
        .unwrap_or(Lang::En)
}

/// Translates a UI string.
pub fn t(s: &'static str) -> &'static str {
    if current() == Lang::Es {
        table().get(s).copied().unwrap_or(s)
    } else {
        s
    }
}

/// Translates a string that is not a literal (catalog names).
pub fn tn(s: &str) -> String {
    if current() == Lang::Es {
        table()
            .get(s)
            .map(|x| x.to_string())
            .unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    }
}

/// Translates a format string with `{}` placeholders filled in order.
pub fn tf(s: &'static str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut parts = t(s).split("{}");
    if let Some(first) = parts.next() {
        out.push_str(first);
    }
    for (i, p) in parts.enumerate() {
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        out.push_str(p);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanish_catalog_is_consistent() {
        let mut seen = std::collections::HashSet::new();
        for (en, es) in es::ES {
            assert!(seen.insert(*en), "duplicate key {en}");
            assert_eq!(
                en.matches("{}").count(),
                es.matches("{}").count(),
                "placeholders differ: {en}"
            );
            assert!(!es.is_empty());
        }
    }

    #[test]
    fn every_catalog_name_is_translated() {
        let table: HashMap<&str, &str> = es::ES.iter().copied().collect();
        let mut missing = Vec::new();
        for d in op_core::catalog::CATALOG {
            for s in std::iter::once(d.name)
                .chain(std::iter::once(d.category))
                .chain(d.params.iter().map(|p| p.label))
                .chain(d.params.iter().map(|p| p.group))
            {
                if !s.is_empty() && !table.contains_key(s) && !missing.contains(&s) {
                    missing.push(s);
                }
            }
            for p in d.params {
                if let op_core::catalog::ParamKind::Choice { options, .. } = p.kind {
                    for o in options {
                        if !table.contains_key(o) && !missing.contains(o) {
                            missing.push(o);
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }

    #[test]
    fn every_preset_is_translated() {
        let table: HashMap<&str, &str> = es::ES.iter().copied().collect();
        for p in op_core::presets::PRESETS {
            assert!(table.contains_key(p.name), "untranslated preset {}", p.name);
            assert!(
                table.contains_key(p.category),
                "untranslated {}",
                p.category
            );
        }
    }

    #[test]
    fn formatting() {
        set(Lang::En);
        assert_eq!(tf("{} of {}", &[&1, &2]), "1 of 2");
    }
}
