//! The roman input alphabet, from `vocab.json`.
//!
//! Only `s2i` is read; `T` and `L` carry the output side themselves.

use crate::Result;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
struct VocabJson {
    /// Source symbol -> id. Only the keys are used; the ids were the encoder's.
    s2i: HashMap<String, i32>,
}

/// The set of roman characters the transliteration transducer accepts.
pub struct Vocab {
    input: HashSet<char>,
}

impl Vocab {
    /// Parse `vocab.json`.
    pub fn from_json(json: &str) -> Result<Self> {
        let v: VocabJson = serde_json::from_str(json)?;
        /* The acceptor has one arc per codepoint, so a multi-char symbol could never
        match. Taking a longer key's first char would admit tokens `T` cannot
        consume. */
        let input = v
            .s2i
            .keys()
            .filter_map(|k| {
                let mut cs = k.chars();
                match (cs.next(), cs.next()) {
                    (Some(c), None) => Some(c),
                    _ => None,
                }
            })
            .collect();
        Ok(Self { input })
    }

    /// Whether `w` is non-empty and spelled entirely in input symbols. Everything else
    /// — digits, punctuation, text already in Devanagari — takes the rule path.
    pub fn is_roman(&self, w: &str) -> bool {
        !w.is_empty() && w.chars().all(|c| self.input.contains(&c))
    }
}
