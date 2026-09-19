//! Turning what was learned into a change in the ranking.
//!
//! Separate from the accumulation side: `personal.rs` decides what to remember, this
//! decides what remembering is worth. Everything here reads; nothing mutates.

use super::{Id, Overlay};
use crate::lattice::{self, Candidate};
use crate::params::{
    BOOST_CAP, CTX_BLEND, FAMILIAR_BIGRAM_WEIGHT, FAMILIAR_CAP, FAMILIAR_SCALE,
    INJECT_MIN_EVIDENCE, MAX_CANDIDATES, SHRINK_K,
};

impl Overlay {
    /// How much to subtract from a candidate's cost: always `>= 0`, and exactly 0
    /// without evidence for the candidate.
    ///
    /// `P(word | roman)` for this user, shrunk by the observation count behind it, with
    /// [`Overlay::familiarity`] on top. `roman` must already be lowercased.
    fn boost(&self, roman: &str, word: &str, prev: Option<&str>) -> f32 {
        if self.is_empty() {
            return 0.0;
        }
        let Some(wid) = self.strings.lookup(word) else {
            return 0.0;
        };
        let inv_weight = 1.0 / self.weight.max(f32::MIN_POSITIVE);
        let rid = self.strings.lookup(roman);
        let pid = prev.and_then(|p| self.strings.lookup(p));

        let (p_pick, observations) = self.choice_estimate(rid, pid, wid, inv_weight);
        let confidence = observations / (observations + SHRINK_K);
        let primary = BOOST_CAP * p_pick * confidence;

        (primary + self.familiarity(pid, wid, inv_weight)).min(BOOST_CAP)
    }

    /// `P(word | roman)` blended toward the `(prev, roman)` estimate, with the
    /// observation count behind it.
    ///
    /// The halves are looked up independently: eviction trims `choice` and `ctx_choice`
    /// separately, so conditioned evidence can outlive its backoff and must still count.
    fn choice_estimate(
        &self,
        rid: Option<Id>,
        pid: Option<Id>,
        wid: Id,
        inv_weight: f32,
    ) -> (f32, f32) {
        let Some(rid) = rid else { return (0.0, 0.0) };

        let context_free = {
            let total = self.roman_total.get(&rid).copied().unwrap_or(0.0);
            let hits = self.choice.get(&(rid, wid)).copied().unwrap_or(0.0);
            (total > 0.0).then_some((hits / total, hits))
        };

        let conditioned = pid.and_then(|pid| {
            let total = self.ctx_total.get(&(pid, rid)).copied().unwrap_or(0.0);
            let hits = self
                .ctx_choice
                .get(&(pid, rid, wid))
                .copied()
                .unwrap_or(0.0);
            (total > 0.0).then_some((hits / total, hits))
        });

        match (conditioned, context_free) {
            (Some((p_ctx, ctx_hits)), Some((p_free, roman_hits))) => (
                CTX_BLEND * p_ctx + (1.0 - CTX_BLEND) * p_free,
                (ctx_hits + roman_hits) * inv_weight,
            ),
            (Some((p_ctx, ctx_hits)), None) => (p_ctx, ctx_hits * inv_weight),
            (None, Some((p_free, roman_hits))) => (p_free, roman_hits * inv_weight),
            (None, None) => (0.0, 0.0),
        }
    }

    /// The weak "this user's vocabulary" term. A nudge, never enough to decide alone.
    fn familiarity(&self, pid: Option<Id>, wid: Id, inv_weight: f32) -> f32 {
        let unigram = self.unigram.get(&wid).copied().unwrap_or(0.0) * inv_weight;
        let bigram = pid
            .and_then(|p| self.bigram.get(&(p, wid)))
            .copied()
            .unwrap_or(0.0)
            * inv_weight;
        let raw = 1.0 + unigram + FAMILIAR_BIGRAM_WEIGHT * bigram;
        (FAMILIAR_SCALE * raw.ln()).min(FAMILIAR_CAP)
    }

    /// Rerank `cands` by what this user has learned, then trim to the strip.
    ///
    /// Reweighting promotes only what `T ∘ L` produced, which is what makes an empty
    /// overlay a no-op; injection adds words it never produced, just under the best.
    pub fn apply(&self, cands: &mut Vec<Candidate>, roman: &str, prev: Option<&str>) {
        if self.is_empty() {
            return;
        }
        // Once here, not once per candidate inside `boost`.
        let roman = roman.to_lowercase();

        for candidate in cands.iter_mut() {
            candidate.cost -= self.boost(&roman, &candidate.word, prev);
        }

        let floor = lattice::best_cost(cands)
            .filter(|c| c.is_finite())
            .unwrap_or(0.0);
        for (word, boost) in self.injections(&roman, prev) {
            if !cands.iter().any(|seen| seen.word == word) {
                cands.push(Candidate {
                    word,
                    cost: floor - boost,
                });
            }
        }

        lattice::sort_by_cost(cands);
        cands.truncate(MAX_CANDIDATES);
    }

    /// Words chosen for `roman` that `T ∘ L` did not generate, strongest first — the
    /// only path by which the overlay grows vocabulary the lexicon lacks.
    ///
    /// `roman` must already be lowercased, as in [`Overlay::boost`].
    fn injections(&self, roman: &str, prev: Option<&str>) -> Vec<(String, f32)> {
        if self.is_empty() || roman.is_empty() {
            return Vec::new();
        }
        let Some(rid) = self.strings.lookup(roman) else {
            return Vec::new();
        };
        let inv_weight = 1.0 / self.weight.max(f32::MIN_POSITIVE);
        let pid = prev.and_then(|p| self.strings.lookup(p));

        let mut out: Vec<(String, f32)> = self
            .choice
            .iter()
            .filter(|((r, _), _)| *r == rid)
            .filter_map(|((_, wid), weight)| {
                let conditioned = pid
                    .and_then(|p| self.ctx_choice.get(&(p, rid, *wid)))
                    .copied()
                    .unwrap_or(0.0);
                if (weight + conditioned) * inv_weight < INJECT_MIN_EVIDENCE {
                    return None;
                }
                let word = self.strings.get(*wid)?;
                let boost = self.boost(roman, word, prev);
                (boost > 0.0).then(|| (word.to_string(), boost))
            })
            .collect();
        out.sort_by(|a, b| b.1.total_cmp(&a.1));
        out
    }
}
