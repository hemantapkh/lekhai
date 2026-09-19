//! The word-bigram context model `C`: Katz backoff over the previous word.
//!
//! Optional at run time. `context.fst`'s arc labels are the ids in `context-words.tsv`,
//! so the two are one unit — halves from different builds rerank against the wrong
//! words without erroring.

use crate::lattice::{best_cost, sort_by_cost, Candidate, F};
use crate::params::CONTEXT_MARGIN;
use rustfst::algorithms::tr_compares::ILabelCompare;
use rustfst::algorithms::tr_sort;
use rustfst::prelude::*;
use std::collections::HashMap;
use std::path::Path;

/// A loaded `context.fst` + `context-words.tsv` pair.
pub struct Context {
    c: F,
    word_id: HashMap<String, Label>,
    /// Arcs leaving the unigram/backoff state: label -> (weight, destination).
    unigram: HashMap<Label, (f32, StateId)>,
    /// The weight every unigram arc carries, used for a word with no arc at all so that
    /// being unmodelled never looks cheaper than being modelled.
    neutral: f32,
    /// The `<s>` sentence-start history state.
    start: StateId,
}

impl Context {
    /// Load the pair from `dir`, or `None` on any problem — missing, unreadable,
    /// malformed, or a `context-words.tsv` that parsed to nothing. Context is an enhancement,
    /// never a reason to fail to start.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(dir: &Path) -> Option<Self> {
        let mut c = F::read(dir.join("context.fst")).ok()?;
        tr_sort(&mut c, ILabelCompare {});

        let words = std::fs::read_to_string(dir.join("context-words.tsv")).ok()?;
        let word_id: HashMap<String, Label> = words
            .lines()
            .filter_map(|line| {
                let (id, word) = line.split_once('\t')?;
                Some((word.to_string(), id.parse().ok()?))
            })
            .collect();

        let start = c.start()?;
        /* A model built with sentence-initial counts starts at `<s>`, whose epsilon arc
        backs off to the unigram state. Older models start at the unigram state
        directly, detected by that arc's absence, so both vintages load. */
        let unigram_state = c
            .get_trs(start)
            .ok()?
            .trs()
            .iter()
            .find(|tr| tr.ilabel == 0)
            .map_or(start, |tr| tr.nextstate);

        let mut unigram = HashMap::new();
        let mut neutral = 0.0f32;
        for tr in c.get_trs(unigram_state).ok()?.trs() {
            unigram.insert(tr.ilabel, (*tr.weight.value(), tr.nextstate));
            neutral = *tr.weight.value(); // uniform across these arcs by construction
        }

        (!word_id.is_empty() && !unigram.is_empty()).then_some(Self {
            c,
            word_id,
            unigram,
            neutral,
            start,
        })
    }

    /// Cost of emitting `wid` from history state `state`.
    ///
    /// A direct bigram arc is the Katz estimate; without one, or for a word absent from
    /// `context-words.tsv`, the backoff path applies. Katz weights can be positive, so an
    /// unknown word must not score as free — that would rank it above one merely
    /// lacking a bigram.
    fn cost(&self, state: StateId, wid: Option<Label>) -> f32 {
        let mut direct = None;
        let mut backoff = None;
        if let Ok(trs) = self.c.get_trs(state) {
            for tr in trs.trs() {
                if tr.ilabel == 0 {
                    backoff = Some(*tr.weight.value());
                } else if Some(tr.ilabel) == wid {
                    direct = Some(*tr.weight.value());
                }
            }
        }
        if let Some(d) = direct {
            return d;
        }
        let unigram = wid
            .and_then(|w| self.unigram.get(&w))
            .map_or(self.neutral, |(w, _)| *w);
        backoff.unwrap_or(0.0) + unigram
    }

    /// The history state to score against, or `None` when there is no context to apply.
    ///
    /// No previous word means sentence start, which is a real context rather than the
    /// absence of one. An *unknown* previous word is the genuine absence.
    fn history(&self, prev: Option<&str>) -> Option<StateId> {
        match prev {
            None => Some(self.start),
            Some(p) => self
                .word_id
                .get(p)
                .and_then(|id| self.unigram.get(id))
                .map(|(_, dest)| *dest),
        }
    }

    /// Rerank `cands` by transliteration cost plus the context cost of following `prev`.
    ///
    /// Only candidates within [`CONTEXT_MARGIN`] of the best are eligible, so context
    /// breaks close calls without overriding a clearly better spelling; the rest keep
    /// their cost rather than being penalised.
    ///
    /// The adjustment goes into the cost, not the list order: personalization re-sorts
    /// by cost, and a stable sort would restore the pre-context ranking.
    pub fn apply(&self, cands: &mut [Candidate], prev: Option<&str>) {
        if cands.len() < 2 {
            return;
        }
        let (Some(history), Some(best)) = (self.history(prev), best_cost(cands)) else {
            return;
        };
        for candidate in cands.iter_mut() {
            if candidate.cost <= best + CONTEXT_MARGIN {
                candidate.cost += self.cost(history, self.word_id.get(&candidate.word).copied());
            }
        }
        sort_by_cost(cands);
    }
}
