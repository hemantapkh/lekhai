//! English to Nepali transliteration engine.
//!
//! Per token, a linear acceptor over the roman codepoints is composed with the
//! transliteration transducer `T` and the lexicon word-LM `L`; the n-shortest paths are
//! the ranked real-word candidates, and `rules.json` is the fallback for anything `L`
//! rejects. No neural network.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod devanagari;

mod context;
mod engine;
mod lattice;
mod params;
mod personal;
mod rules;
mod vocab;

pub use engine::Engine;

/// Absolute path to the model files bundled with this crate.
pub const BUNDLED_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");

/* The rule tables, vocabulary and learned overlay stay private on purpose. Exporting
them would semver-lock the learning internals, and a public `Engine::new` taking
them as arguments would put `rustfst::VectorFst` in this crate's signature, making a
`rustfst` bump a breaking change here. */

/// The result of analysing one roman token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Analysis {
    /// What a space key would commit.
    pub best: String,
    /// The suggestion strip, `best` first. Never empty.
    pub candidates: Vec<String>,
}

/// Every ranking stage for one token, with its costs. A diagnostic, off the typing
/// path: [`Engine::explain`] re-runs the same stage functions and records each list.
#[derive(Clone, Debug, PartialEq)]
pub struct Explanation {
    /// The token as the pipeline saw it, lowercased.
    pub token: String,
    /// The previous committed word the context stage was conditioned on.
    pub prev: Option<String>,
    /// False when the token left the FST entirely and only `rule_spelling` ran — digits,
    /// punctuation, Devanagari, or anything outside the roman alphabet.
    pub is_roman: bool,
    /// Whether the word-bigram model is loaded at all.
    pub context_is_live: bool,
    /// `acceptor ∘ T` alone: what the transliteration transducer proposes before the
    /// lexicon is consulted. Mostly not Nepali words — `L` is what makes them words.
    /// Computed only for this diagnostic; the decode path composes straight through.
    pub after_translit: Vec<(String, f32)>,
    /// `acceptor ∘ T ∘ L`, n-shortest, before any reranking. Lower cost ranks higher.
    pub after_lexicon: Vec<(String, f32)>,
    /// After the word-bigram rerank. `None` only when no context model is loaded — it
    /// runs with a `prev` of `None` too, where sentence-initial position is itself a
    /// context the model was trained on.
    pub after_context: Option<Vec<(String, f32)>>,
    /// After the learned overlay. `None` when the overlay is empty.
    pub after_personal: Option<Vec<(String, f32)>>,
    /// What the `rules.json` table alone spells, always computed and appended as a floor.
    pub rule_spelling: String,
    /// The strip a caller receives — the rule spelling folded in, truncated.
    pub candidates: Vec<String>,
}

/// Load-time failures. Non-exhaustive: match with a `_` arm.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A JSON resource failed to parse.
    Parse(String),
    /// A resource was missing, unreadable, or a malformed FST.
    Backend(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(m) => write!(f, "parse error: {m}"),
            Error::Backend(m) => write!(f, "load error: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Parse(e.to_string())
    }
}

/// Shorthand for this crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
