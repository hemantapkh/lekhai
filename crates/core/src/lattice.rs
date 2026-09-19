//! `accep(roman) ∘ T ∘ L`: build the acceptor, compose it, read paths back.
//!
//! Free functions over `rustfst` values, holding no engine state.

use crate::params::{NBEST_OVERGEN, SP_DELTA};
use rustfst::algorithms::compose::compose;
use rustfst::algorithms::rm_epsilon::rm_epsilon;
use rustfst::algorithms::tr_compares::OLabelCompare;
use rustfst::algorithms::{
    project, shortest_path_with_config, tr_sort, ProjectType, ShortestPathConfig,
};
use rustfst::prelude::*;
use std::sync::Arc;

pub type F = VectorFst<TropicalWeight>;

/// One candidate spelling and its cost, in nats. Lower is better: stages subtract to
/// promote and add to demote.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub word: String,
    pub cost: f32,
}

/// Linear acceptor over a word's codepoints, labelled as pynini labelled `T` and `L`.
fn acceptor(word: &str) -> F {
    let mut a = F::new();
    let s0 = a.add_state();
    let _ = a.set_start(s0);
    let mut prev = s0;
    for ch in word.chars() {
        let n = a.add_state();
        let label = ch as Label;
        let _ = a.add_tr(prev, Tr::new(label, label, TropicalWeight::one(), n));
        prev = n;
    }
    let _ = a.set_final(prev, TropicalWeight::one());
    a
}

/// `accep(word) ∘ T ∘ L`, or `None` when the compose is empty — which is what a word
/// outside the lexicon looks like, since `L` accepts real words only.
///
/// `t` and `l` are behind [`Arc`] so each compose clones a refcount, not the states.
fn lattice(word: &str, t: &Arc<F>, l: &Arc<F>) -> Option<F> {
    let mut a = acceptor(word);
    tr_sort(&mut a, OLabelCompare {});
    let mut at: F = compose::<TropicalWeight, F, F, F, F, Arc<F>>(a, t.clone()).ok()?;
    if at.num_states() == 0 {
        return None;
    }
    tr_sort(&mut at, OLabelCompare {});
    let atl: F = compose::<TropicalWeight, F, F, F, F, Arc<F>>(at, l.clone()).ok()?;
    (atl.num_states() != 0).then_some(atl)
}

fn path_word(olabels: impl IntoIterator<Item = Label>) -> String {
    olabels.into_iter().filter_map(char::from_u32).collect()
}

/// The cheapest path, from a dedicated `nshortest = 1` search.
///
/// Separate from [`nbest`] because the n-shortest transform perturbs path weights, so
/// its head is not reliably the true best.
fn best_path(atl: &F) -> Option<Candidate> {
    let sp = shortest_path_with_config::<TropicalWeight, F, F>(
        atl,
        ShortestPathConfig::new(SP_DELTA, 1, false),
    )
    .ok()?;
    let p = sp.paths_iter().next()?;
    let word = path_word(p.olabels);
    (!word.is_empty()).then(|| Candidate {
        word,
        cost: *p.weight.value(),
    })
}

/// The n-shortest paths of the projected lattice, for the suggestion strip. Order below
/// the head is approximate.
///
/// Projecting to output labels and removing epsilons before searching is what makes the
/// results distinct words: on the raw transducer many input paths spell one output and
/// crowd everything else out.
fn nbest(atl: &F, k: usize) -> Vec<Candidate> {
    let mut proj = atl.clone();
    project(&mut proj, ProjectType::ProjectOutput);
    let _ = rm_epsilon(&mut proj);

    let sp = match shortest_path_with_config::<TropicalWeight, F, F>(
        &proj,
        ShortestPathConfig::new(SP_DELTA, k * NBEST_OVERGEN, false),
    ) {
        Ok(sp) => sp,
        Err(_) => return Vec::new(),
    };

    let mut scored: Vec<Candidate> = sp
        .paths_iter()
        .map(|p| Candidate {
            word: path_word(p.olabels),
            cost: *p.weight.value(),
        })
        .filter(|c| !c.word.is_empty())
        .collect();
    sort_by_cost(&mut scored);
    scored
}

/// Append `cands` to `out`, skipping a spelling already there. Several input alignments
/// spell one output at different costs; cheapest arrives first, so the first wins.
fn push_new(out: &mut Vec<Candidate>, cands: Vec<Candidate>) {
    for c in cands {
        if !out.iter().any(|seen| seen.word == c.word) {
            out.push(c);
        }
    }
}

/// `accep(word) ∘ T` alone, n-shortest — what `T` proposes before `L` restricts it to
/// real words.
///
/// Diagnostic only: [`candidates`] composes straight through to `L` and never searches
/// this intermediate, so recovering it costs its own n-shortest pass.
pub fn translit_candidates(word: &str, t: &Arc<F>, k: usize) -> Vec<Candidate> {
    let mut a = acceptor(word);
    tr_sort(&mut a, OLabelCompare {});
    let at = match compose::<TropicalWeight, F, F, F, F, Arc<F>>(a, t.clone()) {
        Ok(at) if at.num_states() != 0 => at,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    push_new(&mut out, nbest(&at, k));
    out
}

/// Every real-word candidate for `word`, cheapest first, deduped by spelling. Empty
/// when the word is outside the lexicon.
pub fn candidates(word: &str, t: &Arc<F>, l: &Arc<F>, k: usize) -> Vec<Candidate> {
    let Some(atl) = lattice(word, t, l) else {
        return Vec::new();
    };

    let mut out: Vec<Candidate> = best_path(&atl).into_iter().collect();
    if k > 1 {
        push_new(&mut out, nbest(&atl, k));
    }
    out
}

/// Sort cheapest first. Costs come from tropical path weights and are finite, so
/// `total_cmp` is an exact ordering rather than a fallback.
pub fn sort_by_cost(cands: &mut [Candidate]) {
    cands.sort_by(|a, b| a.cost.total_cmp(&b.cost));
}

/// The cheapest cost, or `None` if there are no candidates.
pub fn best_cost(cands: &[Candidate]) -> Option<f32> {
    cands.iter().map(|c| c.cost).reduce(f32::min)
}
