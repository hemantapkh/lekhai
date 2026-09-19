//! [`Engine`]: loading, and the pipeline that ranks a roman token.
//!
//! The stages live elsewhere — [`crate::lattice`], [`crate::context`],
//! [`crate::personal`]. What is here is the order they run in.

use crate::context::Context;
use crate::lattice::{self, Candidate, F};
use crate::params::MAX_CANDIDATES;
use crate::personal::Overlay;
use crate::rules::Rules;
use crate::vocab::Vocab;
use crate::{devanagari, Analysis, Error, Explanation, Result};
use rustfst::algorithms::tr_compares::ILabelCompare;
use rustfst::algorithms::tr_sort;
use rustfst::prelude::*;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// A loaded engine. Cheap to share: `T` and `L` sit behind [`Arc`] and every decode
/// method takes `&self`.
pub struct Engine {
    /// Transliteration transducer, arc-sorted by ilabel.
    t: Arc<F>,
    /// Lexicon word-LM acceptor, arc-sorted by ilabel.
    l: Arc<F>,
    vocab: Vocab,
    rules: Rules,
    /// `None` when `context.fst`/`context-words.tsv` are absent or unusable.
    context: Option<Context>,
    /// What this user types. Contributes exactly 0.0 while empty, so a fresh install
    /// ranks as if it did not exist.
    personal: RwLock<Overlay>,
    /// Identifies the model the overlay was learned against, so a snapshot from a
    /// different `T`/`L` cannot be loaded on top of it. See [`fingerprint`].
    fingerprint: u64,
}

impl std::fmt::Debug for Engine {
    /// Takes no lock — `Debug` must not block, and printing mid-commit would.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("context_is_live", &self.context.is_some())
            .field("fingerprint", &self.fingerprint)
            .finish_non_exhaustive()
    }
}

/// Copy a candidate list for [`Explanation`], before the next stage reranks it.
fn pairs(cands: &[Candidate]) -> Vec<(String, f32)> {
    cands.iter().map(|c| (c.word.clone(), c.cost)).collect()
}

/// Cheap identity for a model pair: both state counts packed into one `u64`.
///
/// Not a hash of the files, which would mean reading ~8 MB at every load to guard
/// against a mistake whose cost is some misrankings the user can clear. Two builds with
/// identical state counts in both `T` and `L` would collide; any real change to the
/// lexicon or the alignment moves at least one count.
fn fingerprint(t: &F, l: &F) -> u64 {
    ((t.num_states() as u64) << 32) ^ (l.num_states() as u64)
}

impl Engine {
    /// Build from already-loaded parts.
    ///
    /// Crate-internal: the signature names `rustfst`'s types, so a public version would
    /// put `rustfst` in this crate's API and make a version bump there breaking here.
    pub(crate) fn new(mut t: F, mut l: F, vocab: Vocab, rules: Rules) -> Self {
        // Composition uses the sorted matcher; sort once here, not per keystroke.
        tr_sort(&mut t, ILabelCompare {});
        tr_sort(&mut l, ILabelCompare {});
        let fingerprint = fingerprint(&t, &l);
        Self {
            t: Arc::new(t),
            l: Arc::new(l),
            vocab,
            rules,
            context: None,
            personal: RwLock::new(Overlay::new(fingerprint)),
            fingerprint,
        }
    }

    /// Load from a directory holding `translit.fst`, `lexicon.fst`, `vocab.json` and `rules.json`,
    /// plus `context.fst` + `context-words.tsv` if they are there. Only the four required files can
    /// fail the load; the context pair is dropped on any problem.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(dir: &str) -> Result<Self> {
        let dir = std::path::Path::new(dir);
        let text = |name: &str| {
            std::fs::read_to_string(dir.join(name))
                .map_err(|e| Error::Backend(format!("{name}: {e}")))
        };
        let fst = |name: &str| {
            F::read(dir.join(name)).map_err(|e| Error::Backend(format!("{name}: {e}")))
        };

        let mut engine = Self::new(
            fst("translit.fst")?,
            fst("lexicon.fst")?,
            Vocab::from_json(&text("vocab.json")?)?,
            Rules::from_json(&text("rules.json")?)?,
        );
        engine.context = Context::load(dir);
        Ok(engine)
    }

    // Decoding

    /// Recovers from a poisoned lock: skipping would disable learning for the life of
    /// the process, and make `export_learned` overwrite the user's snapshot with an
    /// empty one. Stale totals are the worst a recovered guard costs.
    fn personal(&self) -> RwLockReadGuard<'_, Overlay> {
        self.personal.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn personal_mut(&self) -> RwLockWriteGuard<'_, Overlay> {
        self.personal
            .write()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Rank one roman token: the winner plus the suggestion strip.
    ///
    /// Stage order matters. Context is a corpus prior, personalization a direct
    /// observation of this user's choice, so the more specific evidence runs last and
    /// can override the more general.
    pub fn analyze(&self, word: &str, prev: Option<&str>) -> Result<Analysis> {
        let lowered = word.to_lowercase();

        // Digits, punctuation and Devanagari never reach the FST.
        if !self.vocab.is_roman(&lowered) {
            let token = self.rules.rule_token(word);
            return Ok(Analysis {
                best: token.clone(),
                candidates: vec![token],
            });
        }

        let mut cands = lattice::candidates(&lowered, &self.t, &self.l, MAX_CANDIDATES);
        if let Some(context) = &self.context {
            context.apply(&mut cands, prev);
        }
        self.personal().apply(&mut cands, &lowered, prev);

        Ok(self.finish(cands, &lowered))
    }

    /// Assemble the answer, appending the rule spelling as a floor.
    ///
    /// The rule layer always produces something, so the strip is never empty — including
    /// for a word `T ∘ L` rejected outright, where it becomes the answer. Appended
    /// rather than ranked because it has no cost to compare.
    fn finish(&self, cands: Vec<Candidate>, lowered: &str) -> Analysis {
        let ruled = self.rules.rule_word(lowered);
        let mut words: Vec<String> = cands.into_iter().map(|c| c.word).collect();
        if !words.contains(&ruled) {
            words.push(ruled);
        }
        words.truncate(MAX_CANDIDATES);

        // Never empty: `analyze` reaches here only for a non-empty roman token, which
        // the rule layer always spells into something.
        let best = words.first().cloned().unwrap_or_default();
        Analysis {
            best,
            candidates: words,
        }
    }

    /// Why `word` ranks the way it does: every stage, with its costs.
    ///
    /// A diagnostic, off the typing path. It repeats `analyze`'s stage order and calls
    /// the same stage functions; `explain_matches_analyze` pins the two together.
    pub fn explain(&self, word: &str, prev: Option<&str>) -> Result<Explanation> {
        let lowered = word.to_lowercase();
        let rule_spelling = self.rules.rule_word(&lowered);

        if !self.vocab.is_roman(&lowered) {
            let token = self.rules.rule_token(word);
            return Ok(Explanation {
                token: lowered,
                prev: prev.map(str::to_string),
                is_roman: false,
                context_is_live: self.context.is_some(),
                after_translit: Vec::new(),
                after_lexicon: Vec::new(),
                after_context: None,
                after_personal: None,
                rule_spelling,
                candidates: vec![token],
            });
        }

        let after_translit = pairs(&lattice::translit_candidates(
            &lowered,
            &self.t,
            MAX_CANDIDATES,
        ));
        let mut cands = lattice::candidates(&lowered, &self.t, &self.l, MAX_CANDIDATES);
        let after_lexicon = pairs(&cands);

        // Context runs with no `prev` too: sentence-initial is a context, not an
        // absent one. `None` means the stage was unavailable, not skipped.
        let mut after_context = None;
        if let Some(context) = &self.context {
            context.apply(&mut cands, prev);
            after_context = Some(pairs(&cands));
        }

        let mut after_personal = None;
        {
            let personal = self.personal();
            let had_entries = !personal.is_empty();
            personal.apply(&mut cands, &lowered, prev);
            if had_entries {
                after_personal = Some(pairs(&cands));
            }
        }

        Ok(Explanation {
            token: lowered.clone(),
            prev: prev.map(str::to_string),
            is_roman: true,
            context_is_live: self.context.is_some(),
            after_translit,
            after_lexicon,
            after_context,
            after_personal,
            rule_spelling,
            candidates: self.finish(cands, &lowered).candidates,
        })
    }

    /// The winning word alone.
    pub fn best(&self, word: &str, prev: Option<&str>) -> Result<String> {
        Ok(self.analyze(word, prev)?.best)
    }

    /// The ranked strip alone, best first. Never empty.
    pub fn candidates(&self, word: &str, prev: Option<&str>) -> Result<Vec<String>> {
        Ok(self.analyze(word, prev)?.candidates)
    }

    /// Transliterate a line, threading context from each token into the next.
    ///
    /// Only a Devanagari word carries forward ([`devanagari::is_word`]); anything else
    /// resets to sentence start. Splits on the space character only, preserving runs of
    /// spaces — tabs and newlines are not separators.
    pub fn transliterate(&self, text: &str) -> Result<String> {
        let mut out: Vec<String> = Vec::new();
        let mut prev: Option<String> = None;
        for token in text.split(' ') {
            if token.is_empty() {
                out.push(String::new()); // a run of spaces is layout, not a boundary
                continue;
            }
            let best = self.analyze(token, prev.as_deref())?.best;
            prev = devanagari::is_word(&best).then(|| best.clone());
            out.push(best);
        }
        Ok(out.join(" "))
    }

    /// Whether previous-word context is in effect.
    ///
    /// [`Engine::load`] drops the context model on any problem rather than failing to
    /// start, so `context.fst` can be present and this still be false.
    pub fn context_is_live(&self) -> bool {
        self.context.is_some()
    }

    // Learning
    /* The ranking half is `Overlay::apply`; what is here is the lock. A poisoned lock
    means "no personalization" rather than an error: refusing to transliterate
    because another thread panicked mid-commit would be a keyboard that stops
    accepting keystrokes. */

    /// Record that the user accepted `word` for input `roman`, following `prev`.
    ///
    /// The app must call this: the engine never sees the UI, so it cannot know which
    /// candidate was taken.
    pub fn commit(&self, roman: &str, word: &str, prev: Option<&str>) {
        self.personal_mut().commit(roman, word, prev);
    }

    /// Undo a [`Engine::commit`], for a word the user corrected right after taking
    /// it. Pair it with a `commit` of what they meant.
    ///
    /// Removes evidence without adding the negative kind, and is a no-op for a word
    /// never learned. Detecting the correction is the host's job.
    pub fn retract(&self, roman: &str, word: &str, prev: Option<&str>) {
        self.personal_mut().retract(roman, word, prev);
    }

    /// Everything learned, for the app to persist. Opaque and fingerprinted.
    pub fn export_learned(&self) -> Vec<u8> {
        self.personal().export()
    }

    /// Restore a snapshot. False, and the overlay untouched, if it is malformed or was
    /// learned against a different model.
    pub fn import_learned(&self, bytes: &[u8]) -> bool {
        self.personal_mut().import(bytes, self.fingerprint)
    }

    /// Forget everything learned about this user.
    pub fn clear_learned(&self) {
        self.personal_mut().clear();
    }

    /// Entries, not words — one commit writes into several maps, so this is a few
    /// times the number of words taught. Not the number for "1,240 words" in a UI.
    pub fn learned_count(&self) -> usize {
        self.personal().len()
    }

    /// Approximate heap bytes those entries occupy.
    pub fn learned_footprint(&self) -> usize {
        self.personal().footprint()
    }
}
