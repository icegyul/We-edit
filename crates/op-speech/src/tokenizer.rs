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
        Ok(Self::from_ids(ids))
    }

    pub fn from_ids(ids: HashMap<String, u32>) -> Tokenizer {
        let max = ids.values().copied().max().unwrap_or(0) as usize;
        let mut by_id = vec![None; max + 1];
        for (t, id) in &ids {
            by_id[*id as usize] = Some(t.clone());
        }
        let byte_of = bytes_to_chars().into_iter().map(|(b, c)| (c, b)).collect();
        Tokenizer {
            by_id,
            ids,
            byte_of,
        }
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
}
