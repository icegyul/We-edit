//! Cutting by what was said: dead air between words, filler words ("um", "eh"), words and
//! phrases said twice, and takes said again. Works on the word list a transcription gives (word
//! times from Whisper are approximate, so cuts are snapped to the quietest moment nearby by the
//! caller).

use crate::detect::Range;

/// One transcribed word.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// Why a stretch is cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Silence,
    Gap,
    Filler,
    Repeat,
    Retake,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Silence => "Silence",
            Kind::Gap => "Pause",
            Kind::Filler => "Filler",
            Kind::Repeat => "Repeat",
            Kind::Retake => "Retake",
        }
    }
}

/// A stretch to cut and why.
#[derive(Clone, Debug, PartialEq)]
pub struct Cut {
    pub range: Range,
    pub kind: Kind,
    /// The words removed, for the list in the interface.
    pub text: String,
}

/// Lowercase letters and digits only, accents folded: what two spellings of a word share.
pub fn normalize(word: &str) -> String {
    word.chars()
        .filter_map(|c| {
            let c = c.to_lowercase().next().unwrap_or(c);
            let c = match c {
                'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
                'é' | 'è' | 'ë' | 'ê' => 'e',
                'í' | 'ì' | 'ï' | 'î' => 'i',
                'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
                'ú' | 'ù' | 'ü' | 'û' => 'u',
                'ñ' => 'n',
                'ç' => 'c',
                c => c,
            };
            (c.is_alphanumeric()).then_some(c)
        })
        .collect()
}

fn tokens(words: &[Word]) -> Vec<String> {
    words.iter().map(|w| normalize(&w.text)).collect()
}

/// Filler sounds and expressions of a language, as token sequences. `aggressive` adds the
/// discourse markers people lean on ("like", "o sea", "tipo").
pub fn fillers(lang: &str, aggressive: bool) -> Vec<Vec<String>> {
    let (basic, extra): (&[&str], &[&str]) = match lang {
        "es" => (
            &[
                "eh", "ehh", "em", "emm", "mm", "mmm", "hmm", "ah", "ahh", "uh", "um", "este",
            ],
            &[
                "o sea", "bueno", "pues", "digamos", "tipo", "no se", "a ver", "entonces", "vale",
            ],
        ),
        "pt" => (
            &["eh", "hum", "hmm", "ahn", "ah", "uh", "um", "ne"],
            &["tipo", "entao", "assim", "ne", "bom", "ta"],
        ),
        "fr" => (
            &["euh", "heu", "hum", "hmm", "mmm", "ah", "ben", "bah"],
            &["genre", "du coup", "en fait", "voila", "quoi", "bon"],
        ),
        "de" => (
            &["ah", "aeh", "ahm", "ehm", "hm", "hmm", "mmm", "um", "uh"],
            &["also", "halt", "sozusagen", "quasi", "ja", "genau"],
        ),
        "it" => (
            &["eh", "ehm", "mmm", "hmm", "ah", "uh", "um"],
            &[
                "cioe",
                "tipo",
                "allora",
                "diciamo",
                "praticamente",
                "insomma",
                "comunque",
            ],
        ),
        _ => (
            &[
                "um", "umm", "uh", "uhh", "uhm", "er", "erm", "ah", "ahh", "eh", "hmm", "hm", "mm",
                "mmm", "huh",
            ],
            &[
                "you know",
                "i mean",
                "like",
                "kind of",
                "sort of",
                "basically",
                "actually",
                "literally",
                "right",
                "so yeah",
                "well",
            ],
        ),
    };
    let mut out: Vec<Vec<String>> = basic.iter().map(|f| vec![f.to_string()]).collect();
    if aggressive {
        out.extend(
            extra
                .iter()
                .map(|f| f.split(' ').map(str::to_string).collect()),
        );
    }
    out
}

/// What to look for in the words.
#[derive(Clone, Debug)]
pub struct TextParams {
    pub language: String,
    pub fillers: bool,
    /// Also the discourse markers ("like", "o sea").
    pub aggressive_fillers: bool,
    /// Extra words or phrases to cut, separated by commas.
    pub custom: String,
    pub repeated_words: bool,
    pub repeated_phrases: bool,
    /// Said again: keep the last take of a sentence that was repeated.
    pub retakes: bool,
    /// How alike two sentences must be to count as the same take, 0..1.
    pub retake_similarity: f64,
    /// Dead air between words, in seconds (0 off).
    pub gap: f64,
    /// Air left beside the words that stay.
    pub pad_before: f64,
    pub pad_after: f64,
}

impl Default for TextParams {
    fn default() -> Self {
        TextParams {
            language: "en".into(),
            fillers: true,
            aggressive_fillers: false,
            custom: String::new(),
            repeated_words: true,
            repeated_phrases: true,
            retakes: false,
            retake_similarity: 0.6,
            gap: 0.0,
            pad_before: 0.05,
            pad_after: 0.08,
        }
    }
}

fn custom_list(s: &str) -> Vec<Vec<String>> {
    s.split([',', ';', '\n'])
        .map(|p| {
            p.split_whitespace()
                .map(normalize)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// Words `from..to` as a cut: from the start of the first to just before the word after (so the
/// gap that followed them goes too), with `pad_after` of air left before the word that stays.
fn cut_words(words: &[Word], from: usize, to: usize, p: &TextParams, kind: Kind) -> Cut {
    let start = words[from].start;
    let next = words.get(to).map(|w| w.start).unwrap_or(words[to - 1].end);
    let end = (next - p.pad_after)
        .max(words[to - 1].end.min(next))
        .max(start);
    Cut {
        range: Range { start, end },
        kind,
        text: words[from..to]
            .iter()
            .map(|w| w.text.trim())
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Dead air between words (and before the first one, after the last).
pub fn gaps(words: &[Word], total: f64, p: &TextParams) -> Vec<Cut> {
    let mut out = Vec::new();
    if p.gap <= 0.0 || words.is_empty() {
        return out;
    }
    let mut push = |a: f64, b: f64, pad_a: f64, pad_b: f64| {
        let (s, e) = (a + pad_a, b - pad_b);
        if b - a >= p.gap && e - s >= 0.04 {
            out.push(Cut {
                range: Range { start: s, end: e },
                kind: Kind::Gap,
                text: String::new(),
            });
        }
    };
    push(0.0, words[0].start, 0.0, p.pad_after);
    for w in words.windows(2) {
        push(w[0].end, w[1].start, p.pad_before, p.pad_after);
    }
    push(words[words.len() - 1].end, total, p.pad_before, 0.0);
    out
}

/// Filler sounds and expressions, and the custom list.
pub fn find_fillers(words: &[Word], p: &TextParams) -> Vec<Cut> {
    let mut list = if p.fillers {
        fillers(&p.language, p.aggressive_fillers)
    } else {
        Vec::new()
    };
    list.extend(custom_list(&p.custom));
    // longest first, so "you know" wins over "you"
    list.sort_by_key(|f| std::cmp::Reverse(f.len()));
    let toks = tokens(words);
    let mut out = Vec::new();
    let mut i = 0;
    'next: while i < toks.len() {
        for f in &list {
            if i + f.len() <= toks.len() && toks[i..i + f.len()] == f[..] {
                out.push(cut_words(words, i, i + f.len(), p, Kind::Filler));
                i += f.len();
                continue 'next;
            }
        }
        i += 1;
    }
    out
}

/// Words and short phrases said twice or more in a row: the earlier copies go.
pub fn find_repeats(words: &[Word], p: &TextParams) -> Vec<Cut> {
    let toks = tokens(words);
    let mut out = Vec::new();
    let max_n = if p.repeated_phrases { 4 } else { 1 };
    let min_n = if p.repeated_words { 1 } else { 2 };
    let mut i = 0;
    'next: while i < toks.len() {
        for n in (min_n..=max_n).rev() {
            if i + 2 * n > toks.len() || toks[i..i + n].iter().any(String::is_empty) {
                continue;
            }
            if toks[i..i + n] == toks[i + n..i + 2 * n] {
                // the earlier copy goes; a third copy is found on the next turn
                out.push(cut_words(words, i, i + n, p, Kind::Repeat));
                i += n;
                continue 'next;
            }
        }
        i += 1;
    }
    out
}

/// Sentences as word index ranges: a pause of `gap` seconds or sentence punctuation ends one.
fn sentences(words: &[Word], gap: f64) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    for i in 0..words.len() {
        let ends = words[i]
            .text
            .trim_end()
            .ends_with(['.', '?', '!', '…', '。']);
        let pause = words
            .get(i + 1)
            .is_some_and(|n| n.start - words[i].end >= gap);
        if ends || pause || i + 1 == words.len() {
            out.push((from, i + 1));
            from = i + 1;
        }
    }
    out
}

/// Length of the longest common subsequence of two token lists.
fn lcs(a: &[String], b: &[String]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut prev = 0;
        for (j, y) in b.iter().enumerate() {
            let tmp = row[j + 1];
            row[j + 1] = if x == y {
                prev + 1
            } else {
                row[j + 1].max(row[j])
            };
            prev = tmp;
        }
    }
    row[b.len()]
}

/// Takes said again: when a sentence is followed (directly, or after one other sentence) by one
/// that says nearly the same, the earlier one is cut and the last take stays.
pub fn find_retakes(words: &[Word], p: &TextParams) -> Vec<Cut> {
    let toks = tokens(words);
    let sents = sentences(words, 0.6);
    let mut out: Vec<Cut> = Vec::new();
    for (k, &(a0, a1)) in sents.iter().enumerate() {
        let a = &toks[a0..a1];
        if a.len() < 3 {
            continue;
        }
        for &(b0, b1) in sents.iter().skip(k + 1).take(2) {
            let b = &toks[b0..b1];
            if b.len() < 3 {
                continue;
            }
            let common = lcs(a, b) as f64;
            let sim = 2.0 * common / (a.len() + b.len()) as f64;
            // a false start is also a beginning of the later take
            let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
            if sim >= p.retake_similarity || (prefix >= 3 && prefix as f64 >= a.len() as f64 * 0.7)
            {
                if out.last().is_none_or(|c| c.range.end <= words[a0].start) {
                    out.push(cut_words(words, a0, a1, p, Kind::Retake));
                }
                break;
            }
        }
    }
    out
}

/// Everything the text finds, merged into one sorted list without overlaps.
pub fn find_all(words: &[Word], total: f64, p: &TextParams) -> Vec<Cut> {
    let mut all = Vec::new();
    if p.fillers || !p.custom.trim().is_empty() {
        all.extend(find_fillers(words, p));
    }
    if p.repeated_words || p.repeated_phrases {
        all.extend(find_repeats(words, p));
    }
    if p.retakes {
        all.extend(find_retakes(words, p));
    }
    all.extend(gaps(words, total, p));
    merge(all)
}

/// Sorts cuts and joins those that touch or overlap (the first one's kind and text stay, the text
/// of the others is added).
pub fn merge(mut cuts: Vec<Cut>) -> Vec<Cut> {
    cuts.retain(|c| c.range.end > c.range.start);
    cuts.sort_by(|a, b| a.range.start.total_cmp(&b.range.start));
    let mut out: Vec<Cut> = Vec::new();
    for c in cuts {
        match out.last_mut() {
            Some(last) if c.range.start <= last.range.end + 1e-6 => {
                last.range.end = last.range.end.max(c.range.end);
                if !c.text.is_empty() && !last.text.contains(&c.text) {
                    if !last.text.is_empty() {
                        last.text.push(' ');
                    }
                    last.text.push_str(&c.text);
                }
            }
            _ => out.push(c),
        }
    }
    out
}
