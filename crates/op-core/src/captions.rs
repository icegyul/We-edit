//! Captions: timed words, grouping into on-screen captions, and SRT/WebVTT files.
//!
//! A caption is a graphics clip with a Caption component: the text, the style and the time of
//! each word (seconds from the clip start, stored as `start-end` pairs separated by spaces in
//! the hidden "timing" parameter). When the text is edited and the words no longer match the
//! stored times, the words share the clip's duration in proportion to their length.

#[derive(Clone, Debug, PartialEq)]
pub struct TimedWord {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

/// One caption: words with times in seconds (on the sequence or relative to the caption).
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub words: Vec<TimedWord>,
}

impl Cue {
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// A cue whose words share its time in proportion to their length.
    pub fn from_text(text: &str, start: f64, end: f64) -> Cue {
        Cue {
            start,
            end,
            words: spread(text, start, end),
        }
    }
}

/// Words of `text` sharing `start..end` in proportion to their letters (plus one each).
pub fn spread(text: &str, start: f64, end: f64) -> Vec<TimedWord> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let weights: Vec<f64> = words
        .iter()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).count() as f64 + 1.0)
        .collect();
    let total: f64 = weights.iter().sum::<f64>().max(1e-9);
    let span = (end - start).max(0.0);
    let mut t = start;
    words
        .iter()
        .zip(weights)
        .map(|(w, k)| {
            let d = span * k / total;
            let tw = TimedWord {
                text: w.to_string(),
                start: t,
                end: t + d,
            };
            t += d;
            tw
        })
        .collect()
}

/// The hidden timing parameter: "start-end" per word, in seconds.
pub fn encode_timing(words: &[TimedWord]) -> String {
    words
        .iter()
        .map(|w| format!("{:.3}-{:.3}", w.start, w.end))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The words of a caption at render time: the text's words with their stored times, or spread
/// over the caption when the text was edited (the word count changed) or has no times.
pub fn words(text: &str, timing: &str, duration: f64) -> Vec<TimedWord> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    let times: Vec<(f64, f64)> = timing
        .split_whitespace()
        .filter_map(|p| {
            let (a, b) = p.split_once('-')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .collect();
    if times.len() == parts.len() && !parts.is_empty() {
        parts
            .iter()
            .zip(times)
            .map(|(w, (s, e))| TimedWord {
                text: w.to_string(),
                start: s,
                end: e,
            })
            .collect()
    } else {
        spread(text, 0.0, duration.max(0.1))
    }
}

/// Groups transcribed words into captions of at most `max_words`, breaking after sentence
/// punctuation and at pauses longer than `max_gap` seconds. Each caption stays on screen until
/// the next one starts (or `hold` seconds after its last word).
pub fn chunk(words: &[TimedWord], max_words: usize, max_gap: f64, hold: f64) -> Vec<Cue> {
    let max_words = max_words.max(1);
    let mut groups: Vec<Vec<TimedWord>> = Vec::new();
    let mut cur: Vec<TimedWord> = Vec::new();
    for w in words {
        if let Some(prev) = cur.last()
            && (cur.len() >= max_words
                || w.start - prev.end > max_gap
                || prev.text.ends_with(['.', '!', '?', ';', ':']))
        {
            groups.push(std::mem::take(&mut cur));
        }
        cur.push(w.clone());
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    let mut cues: Vec<Cue> = groups
        .into_iter()
        .map(|g| Cue {
            start: g[0].start,
            end: g.last().map(|w| w.end).unwrap_or(g[0].end),
            words: g,
        })
        .collect();
    for i in 0..cues.len() {
        let next = cues.get(i + 1).map(|c| c.start);
        let c = &mut cues[i];
        let end = c.end + hold;
        c.end = match next {
            Some(n) => end.min(n).max(c.end),
            None => end,
        };
    }
    cues
}

// ------------------------------------------------------------------------------- files

fn parse_time(s: &str) -> Option<f64> {
    // hh:mm:ss,mmm (SRT) or [hh:]mm:ss.mmm (WebVTT)
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<f64>().ok()?,
            m.parse::<f64>().ok()?,
            s.parse::<f64>().ok()?,
        ),
        [m, s] => (0.0, m.parse::<f64>().ok()?, s.parse::<f64>().ok()?),
        _ => return None,
    };
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// Removes markup such as `<i>`, `<c.color>` or `{\an8}` from a cue line.
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '<' | '{' => depth += 1,
            '>' | '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Cues of an SRT or WebVTT file (the format is recognized from its content).
pub fn parse(text: &str) -> Vec<Cue> {
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().map(|l| l.trim()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let (a, b) = lines[ti].split_once("-->").unwrap_or_default();
        // WebVTT settings follow the end time ("00:01.000 align:start")
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_time(a), parse_time(b)) else {
            continue;
        };
        let body: Vec<String> = lines[ti + 1..]
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| strip_tags(l))
            .collect();
        let body = body.join(" ");
        if body.trim().is_empty() || end <= start {
            continue;
        }
        cues.push(Cue::from_text(body.trim(), start, end));
    }
    cues
}

fn stamp(t: f64, sep: char) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn to_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            stamp(c.start, ','),
            stamp(c.end, ','),
            c.text()
        ));
    }
    out
}

pub fn to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for c in cues {
        out.push_str(&format!(
            "{} --> {}\n{}\n\n",
            stamp(c.start, '.'),
            stamp(c.end, '.'),
            c.text()
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> TimedWord {
        TimedWord {
            text: text.into(),
            start,
            end,
        }
    }

    #[test]
    fn timing_round_trips_and_falls_back_to_spreading() {
        let ws = vec![w("Hola", 0.0, 0.4), w("mundo", 0.4, 1.0)];
        let enc = encode_timing(&ws);
        assert_eq!(enc, "0.000-0.400 0.400-1.000");
        assert_eq!(words("Hola mundo", &enc, 3.0), ws);
        // edited text: the words share the caption's duration
        let edited = words("Hola a todo el mundo", &enc, 2.0);
        assert_eq!(edited.len(), 5);
        assert!((edited[4].end - 2.0).abs() < 1e-9);
    }

    #[test]
    fn chunks_break_on_count_punctuation_and_pauses() {
        let ws = vec![
            w("Hello", 0.0, 0.3),
            w("everyone.", 0.3, 0.8),
            w("Today", 0.9, 1.2),
            w("we", 1.2, 1.3),
            w("learn", 1.3, 1.6),
            w("editing", 1.6, 2.0),
            w("now", 3.5, 3.8),
        ];
        let cues = chunk(&ws, 3, 0.8, 0.5);
        let texts: Vec<String> = cues.iter().map(|c| c.text()).collect();
        assert_eq!(
            texts,
            ["Hello everyone.", "Today we learn", "editing", "now"]
        );
        // a caption holds until the next one, never overlapping it
        assert!((cues[0].end - 0.9).abs() < 1e-9);
        assert!((cues[2].end - 2.5).abs() < 1e-9);
        assert!((cues[3].end - 4.3).abs() < 1e-9);
    }

    #[test]
    fn srt_and_vtt_parse_and_write() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\n<i>Hola</i> mundo\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nSegunda\r\nlínea\r\n";
        let cues = parse(srt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text(), "Hola mundo");
        assert_eq!((cues[0].start, cues[0].end), (1.0, 2.5));
        assert_eq!(cues[1].text(), "Segunda línea");
        let out = to_srt(&cues);
        assert!(out.starts_with("1\n00:00:01,000 --> 00:00:02,500\nHola mundo\n"));
        assert_eq!(parse(&out), cues);
        let vtt = "WEBVTT\n\n00:01.000 --> 00:02.000 align:start\n{\\an8}Arriba\n";
        let v = parse(vtt);
        assert_eq!(v[0].text(), "Arriba");
        assert_eq!((v[0].start, v[0].end), (1.0, 2.0));
        assert!(to_vtt(&cues).starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHola mundo"));
    }
}
