//! Whisper's tokenizer, for decoding only: a byte-level BPE vocabulary (GPT-2 style, tokens
//! spelled with printable stand-ins for bytes) plus added special tokens, read from the model's
//! `tokenizer.json`.

use std::collections::HashMap;
use std::path::Path;

use crate::{Result, SpeechError};

pub struct Tokenizer {
    by_id: Vec<Option<String>>,
    ids: HashMap<String, u32>,
    /// Inverse of GPT-2's bytes-to-characters table.
    byte_of: HashMap<char, u8>,
    /// GPT-2's table from bytes to characters.
    char_of: Vec<char>,
    /// Merge ranks of the BPE vocabulary: (left, right) -> position (lower merges first).
    ranks: HashMap<(String, String), usize>,
}

/// GPT-2's table from bytes to the printable characters used in the vocabulary.
fn bytes_to_chars() -> Vec<(u8, char)> {
    let mut printable: Vec<u32> = ('!' as u32..='~' as u32)
        .chain('¡' as u32..='¬' as u32)
        .chain('®' as u32..='ÿ' as u32)
        .collect();
    let mut chars = printable.clone();
    let mut n = 0;
    for b in 0..256u32 {
        if !printable.contains(&b) {
            printable.push(b);
            chars.push(256 + n);
            n += 1;
        }
    }
    printable
        .into_iter()
        .zip(chars)
        .map(|(b, c)| (b as u8, char::from_u32(c).unwrap_or('?')))
        .collect()
}

impl Tokenizer {
    pub fn load(path: &Path) -> Result<Tokenizer> {
        let text = std::fs::read_to_string(path)?;
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| SpeechError::Model(format!("tokenizer.json: {e}")))?;
        let mut ids = HashMap::new();
        if let Some(vocab) = json.pointer("/model/vocab").and_then(|v| v.as_object()) {
            for (tok, id) in vocab {
                if let Some(id) = id.as_u64() {
                    ids.insert(tok.clone(), id as u32);
                }
            }
        }
        if let Some(added) = json.get("added_tokens").and_then(|v| v.as_array()) {
            for t in added {
                if let (Some(id), Some(content)) = (
                    t.get("id").and_then(|v| v.as_u64()),
                    t.get("content").and_then(|v| v.as_str()),
                ) {
                    ids.insert(content.to_string(), id as u32);
                }
            }
        }
        if ids.is_empty() {
            return Err(SpeechError::Model(
                "tokenizer.json has no vocabulary".into(),
            ));
        }
        let mut tok = Self::from_ids(ids);
        if let Some(merges) = json.pointer("/model/merges").and_then(|v| v.as_array()) {
            for (rank, m) in merges.iter().enumerate() {
                // "a b" in older files, ["a", "b"] in newer ones
                let pair = match m {
                    serde_json::Value::String(s) => s
                        .split_once(' ')
                        .map(|(a, b)| (a.to_string(), b.to_string())),
                    serde_json::Value::Array(a) if a.len() == 2 => Some((
                        a[0].as_str().unwrap_or("").to_string(),
                        a[1].as_str().unwrap_or("").to_string(),
                    )),
                    _ => None,
                };
                if let Some(p) = pair {
                    tok.ranks.insert(p, rank);
                }
            }
        }
        Ok(tok)
    }

    pub fn from_ids(ids: HashMap<String, u32>) -> Tokenizer {
        let max = ids.values().copied().max().unwrap_or(0) as usize;
        let mut by_id = vec![None; max + 1];
        for (t, id) in &ids {
            by_id[*id as usize] = Some(t.clone());
        }
        let table = bytes_to_chars();
        let byte_of = table.iter().map(|(b, c)| (*c, *b)).collect();
        let mut char_of = vec!['?'; 256];
        for (b, c) in &table {
            char_of[*b as usize] = *c;
        }
        Tokenizer {
            by_id,
            ids,
            byte_of,
            char_of,
            ranks: HashMap::new(),
        }
    }

    /// Token ids of `text` (byte-level BPE, words split at spaces). Used for the short prompts
    /// that steer the decoder; tokens the vocabulary lacks are skipped.
    pub fn encode(&self, text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        let mut word = String::new();
        let flush = |word: &mut String, out: &mut Vec<u32>| {
            if word.is_empty() {
                return;
            }
            // the word's bytes, spelled with the vocabulary's characters
            let mut parts: Vec<String> = word
                .bytes()
                .map(|b| self.char_of[b as usize].to_string())
                .collect();
            // merge the best-ranked pair until none is left
            loop {
                let best = (0..parts.len().saturating_sub(1))
                    .filter_map(|i| {
                        self.ranks
                            .get(&(parts[i].clone(), parts[i + 1].clone()))
                            .map(|r| (*r, i))
                    })
                    .min();
                let Some((_, i)) = best else { break };
                let merged = format!("{}{}", parts[i], parts[i + 1]);
                parts.splice(i..=i + 1, [merged]);
            }
            for p in parts {
                if let Some(id) = self.ids.get(&p) {
                    out.push(*id);
                }
            }
            word.clear();
        };
        for c in text.chars() {
            if c == ' ' {
                flush(&mut word, &mut out);
            }
            word.push(c);
        }
        flush(&mut word, &mut out);
        out
    }

    pub fn id(&self, token: &str) -> Option<u32> {
        self.ids.get(token).copied()
    }

    /// Text of `tokens`, leaving out special tokens (`<|...|>`) and timestamps (ids from
    /// `ts_begin`).
    pub fn decode(&self, tokens: &[u32], ts_begin: u32) -> String {
        let mut bytes = Vec::new();
        for &t in tokens {
            if t >= ts_begin {
                continue;
            }
            let Some(Some(s)) = self.by_id.get(t as usize) else {
                continue;
            };
            if s.starts_with("<|") && s.ends_with("|>") {
                continue;
            }
            for c in s.chars() {
                match self.byte_of.get(&c) {
                    Some(b) => bytes.push(*b),
                    None => {
                        let mut buf = [0u8; 4];
                        bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_level_tokens_decode_to_utf8() {
        let table: HashMap<u8, char> = bytes_to_chars().into_iter().collect();
        assert_eq!(table.len(), 256);
        assert_eq!(table[&b' '], 'Ġ');
        // "Ġcanci" + "Ã³n" spells " canción" in GPT-2's byte characters
        let o_acute: String = "ó".bytes().map(|b| table[&b]).collect();
        let mut ids = HashMap::new();
        ids.insert("Ġcanci".to_string(), 0);
        ids.insert(format!("{o_acute}n"), 1);
        ids.insert("<|endoftext|>".to_string(), 2);
        let tok = Tokenizer::from_ids(ids);
        assert_eq!(tok.decode(&[0, 1, 2], 100), " canción");
        assert_eq!(tok.id("<|endoftext|>"), Some(2));
    }

    #[test]
    fn text_is_merged_by_rank() {
        let mut ids = HashMap::new();
        for (i, t) in ["h", "i", "hi", "Ġ", "Ġh", "Ġhi", "t", "Ġt"]
            .iter()
            .enumerate()
        {
            ids.insert(t.to_string(), i as u32);
        }
        let mut tok = Tokenizer::from_ids(ids);
        for (rank, (a, b)) in [("Ġ", "h"), ("Ġh", "i"), ("h", "i"), ("Ġ", "t")]
            .iter()
            .enumerate()
        {
            tok.ranks.insert((a.to_string(), b.to_string()), rank);
        }
        // "hi hi" is "hi" then " hi", and the letters nothing merges stay apart
        assert_eq!(tok.encode("hi hi"), vec![2, 5]);
        assert_eq!(tok.encode("hi t"), vec![2, 7]);
    }
}
