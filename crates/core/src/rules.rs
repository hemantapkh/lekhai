//! Deterministic phonetic (ITRANS-ish) transliteration, from `rules.json`.
//!
//! The layer that guarantees an answer: `T ∘ L` returns nothing for a name, a typo or
//! anything outside the lexicon, and these rules always produce something. Also the
//! whole path for digits and punctuation.
//!
//! `SIGN` and `LAM` are in the file but not read here: `pipeline/` applies them when it
//! builds `T` and `lexicon.fst`.

use crate::{Error, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct RulesJson {
    #[serde(rename = "CONS")]
    cons: Vec<Vec<String>>, // [ [tok, dev], ... ]
    #[serde(rename = "VOW")]
    vow: Vec<(String, Vec<String>)>, // [ (tok, [matra, independent]), ... ]
    #[serde(rename = "VIRAMA")]
    virama: String,
    #[serde(rename = "DIGITS_FROM")]
    digits_from: String,
    #[serde(rename = "DIGITS_TO")]
    digits_to: String,
    #[serde(rename = "PUNCT")]
    punct: HashMap<String, String>,
}

pub struct Rules {
    cons: Vec<(Vec<char>, String)>,
    vow: Vec<(Vec<char>, (String, String))>, // (tok chars, (matra, independent))
    virama: String,
    digits: HashMap<char, char>,
    punct: HashMap<String, String>,
}

impl Rules {
    pub fn from_json(json: &str) -> Result<Self> {
        let r: RulesJson = serde_json::from_str(json)?;
        // Reachable with user-supplied assets, so a ragged entry is reported rather
        // than indexed into.
        let cons = r
            .cons
            .into_iter()
            .map(|p| match p.as_slice() {
                [tok, dev] => Ok((tok.chars().collect(), dev.clone())),
                _ => Err(Error::Parse(format!(
                    "CONS entry must be [token, devanagari]; got {} element(s)",
                    p.len()
                ))),
            })
            .collect::<Result<_>>()?;
        let vow = r
            .vow
            .into_iter()
            .map(|(tok, mv)| {
                let matra = mv.first().cloned().unwrap_or_default();
                let ind = mv.get(1).cloned().unwrap_or_default();
                (tok.chars().collect(), (matra, ind))
            })
            .collect();
        /* Longest match must win and `match_at` returns the first hit, so the tables
        are sorted here rather than trusting the file's order: with `k` ahead of `kh`,
        `kh` would never match. Stable, so same-length order is preserved. */
        let mut cons: Vec<(Vec<char>, String)> = cons;
        let mut vow: Vec<(Vec<char>, (String, String))> = vow;
        cons.sort_by_key(|(token, _)| std::cmp::Reverse(token.len()));
        vow.sort_by_key(|(token, _)| std::cmp::Reverse(token.len()));

        let digits: HashMap<char, char> = r.digits_from.chars().zip(r.digits_to.chars()).collect();
        if digits.len() != r.digits_from.chars().count() {
            return Err(Error::Parse("DIGITS_FROM/DIGITS_TO length mismatch".into()));
        }
        Ok(Self {
            cons,
            vow,
            virama: r.virama,
            digits,
            punct: r.punct,
        })
    }

    fn match_at<'a, T>(
        w: &[char],
        i: usize,
        table: &'a [(Vec<char>, T)],
    ) -> Option<(usize, &'a T)> {
        for (tok, val) in table {
            let n = tok.len();
            if i + n <= w.len() && w[i..i + n] == tok[..] {
                return Some((n, val));
            }
        }
        None
    }

    /// Transliterate a roman word: longest-match consonant, then an optional vowel as a
    /// matra. A bare consonant before another takes the virama; one at the end keeps its
    /// inherent `a`. Unknown characters pass through, so this always returns something.
    pub fn rule_word(&self, word: &str) -> String {
        let w: Vec<char> = word.to_lowercase().chars().collect();
        let mut i = 0;
        let mut out = String::new();
        while i < w.len() {
            if let Some((n, dev)) = Self::match_at(&w, i, &self.cons) {
                i += n;
                if let Some((vn, (matra, _ind))) = Self::match_at(&w, i, &self.vow) {
                    i += vn;
                    out.push_str(dev);
                    out.push_str(matra); // consonant + matra
                } else if i < w.len() && Self::match_at(&w, i, &self.cons).is_some() {
                    out.push_str(dev);
                    out.push_str(&self.virama); // cluster -> half consonant
                } else {
                    out.push_str(dev); // inherent 'a'
                }
                continue;
            }
            if let Some((vn, (_matra, ind))) = Self::match_at(&w, i, &self.vow) {
                i += vn;
                out.push_str(ind); // independent vowel
                continue;
            }
            out.push(w[i]); // unknown char passthrough
            i += 1;
        }
        out
    }

    /// One token: digits and punctuation via their tables, roman words via
    /// [`Rules::rule_word`]. Anything already in Devanagari, or with no letters at all,
    /// passes through unchanged.
    pub fn rule_token(&self, tok: &str) -> String {
        if !tok.is_empty() && tok.chars().all(|c| c.is_ascii_digit()) {
            return tok
                .chars()
                .map(|c| *self.digits.get(&c).unwrap_or(&c))
                .collect();
        }
        if let Some(p) = self.punct.get(tok) {
            return p.clone();
        }
        // Already Devanagari, or no letters to transliterate at all.
        if !tok.is_ascii() || !tok.chars().any(char::is_alphabetic) {
            return tok.to_string();
        }
        self.rule_word(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::Rules;

    /// A table written the wrong way round: `k` before `kh`, `c` before `chh`. In file
    /// order `match_at` would return `k` for `kh`, leaving the longer token unreachable.
    /// The shipped `rules.json` contains no such pair, so only a hand-made table can
    /// exercise the sort.
    const MISORDERED: &str = r#"{
        "CONS": [["k","\u0915"], ["kh","\u0916"], ["c","\u091a"], ["chh","\u091b"]],
        "VOW": [["a", ["", "\u0905"]]],
        "VIRAMA": "\u094d",
        "DIGITS_FROM": "0123456789",
        "DIGITS_TO": "\u0966\u0967\u0968\u0969\u096a\u096b\u096c\u096d\u096e\u096f",
        "PUNCT": {}
    }"#;

    #[test]
    fn longest_token_wins_whatever_the_file_order() {
        let rules = Rules::from_json(MISORDERED).expect("parse");
        assert_eq!(rules.rule_word("kha"), "\u{916}", "kh must beat k");
        assert_eq!(rules.rule_word("chha"), "\u{91b}", "chh must beat c");
        // and the short tokens still work
        assert_eq!(rules.rule_word("ka"), "\u{915}");
        assert_eq!(rules.rule_word("ca"), "\u{91a}");
    }

    #[test]
    fn digits_and_unknown_characters() {
        let rules = Rules::from_json(MISORDERED).expect("parse");
        assert_eq!(rules.rule_token("123"), "\u{967}\u{968}\u{969}");
        assert_eq!(
            rules.rule_token("\u{928}\u{947}\u{92a}\u{93e}\u{932}"),
            "\u{928}\u{947}\u{92a}\u{93e}\u{932}",
            "already Devanagari passes through"
        );
    }

    #[test]
    fn ragged_cons_entry_is_reported_not_panicked() {
        let bad = r#"{"CONS": [["k"]], "VOW": [], "VIRAMA": "", "DIGITS_FROM": "", "DIGITS_TO": "", "PUNCT": {}}"#;
        assert!(Rules::from_json(bad).is_err());
    }
}
