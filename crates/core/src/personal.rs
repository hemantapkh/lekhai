//! On-device learning: a bounded, decaying model of what this user types.
//!
//! Keyed `(prev, roman) -> word`, backing off to `roman -> word`: one input resolves
//! differently after different previous words. Learned words are also injected, since
//! reweighting cannot surface what `T ∘ L` never generated.
//!
//! Invariants, all covered by tests:
//!
//!   * Cold-start safe — an empty overlay contributes 0.0 and injects nothing.
//!   * Boost-only — evidence promotes; absence never demotes.
//!   * Bounded — capped entry count, weakest-first eviction.
//!   * Decaying — recent choices outweigh old, in O(1) per commit.
//!   * Fingerprinted — a snapshot from another model is refused, never merged.

mod score;
mod snapshot;
mod strings;

use crate::params::{EVICT_FRACTION, HALF_LIFE_COMMITS, MAX_ENTRIES, RESCALE_AT};
use std::collections::HashMap;
use strings::StringTable;

/// Index into the interned string table.
type Id = u32;

/// Remove up to `w` from one entry, deleting it at zero. Returns the amount removed,
/// which is what a paired denominator must lose.
fn take<K: std::hash::Hash + Eq>(m: &mut HashMap<K, f32>, k: K, w: f32) -> f32 {
    let Some(v) = m.get_mut(&k) else { return 0.0 };
    let taken = w.min(*v);
    *v -= taken;
    if *v <= f32::EPSILON {
        m.remove(&k);
    }
    taken
}

/// Rebuild a map with every key rewritten by `f`.
fn remap<K, K2>(m: &HashMap<K, f32>, f: impl Fn(&K) -> K2) -> HashMap<K2, f32>
where
    K2: std::hash::Hash + Eq,
{
    m.iter().map(|(k, &v)| (f(k), v)).collect()
}

/// Approximate per-entry heap cost for [`Overlay::footprint`]: keys, value and slot
/// overhead. Accurate enough for a settings screen and nothing else.
const COST_CTX_CHOICE: usize = 20;
const COST_PAIR: usize = 16;
const COST_UNIGRAM: usize = 12;

/// Everything learned about one user, against one model.
#[derive(Default)]
pub struct Overlay {
    /// Each distinct string stored once; every map below keys on its [`Id`].
    strings: StringTable,

    /// `(prev, roman) -> word`: the context-conditioned choice, the primary signal.
    ctx_choice: HashMap<(Id, Id, Id), f32>,
    /// `roman -> word`: the context-free choice, the backoff.
    choice: HashMap<(Id, Id), f32>,
    /// `prev -> word`: this user's own word bigram.
    bigram: HashMap<(Id, Id), f32>,
    /// General vocabulary preference.
    unigram: HashMap<Id, f32>,

    /// Denominators for the choice probabilities. Derived, never stored: maintained on
    /// commit, rebuilt on import and after eviction.
    roman_total: HashMap<Id, f32>,
    ctx_total: HashMap<(Id, Id), f32>,

    /// Commits recorded, and the resulting increment weight `2^(tick / half-life)`.
    tick: f32,
    weight: f32,
    /// The model this overlay was learned against.
    fingerprint: u64,
}

impl Overlay {
    pub fn new(fingerprint: u64) -> Self {
        Self {
            weight: 1.0,
            fingerprint,
            ..Default::default()
        }
    }

    /// The cold-start guard every scoring path checks first. Covers every map: an
    /// imported snapshot need not have the unigram entry `commit` always writes.
    pub fn is_empty(&self) -> bool {
        self.choice.is_empty()
            && self.ctx_choice.is_empty()
            && self.bigram.is_empty()
            && self.unigram.is_empty()
    }

    /// Entries across every map.
    pub fn len(&self) -> usize {
        self.ctx_choice.len() + self.choice.len() + self.bigram.len() + self.unigram.len()
    }

    /// Approximate heap bytes held: the string table plus the maps.
    pub fn footprint(&self) -> usize {
        self.strings.footprint()
            + self.ctx_choice.len() * COST_CTX_CHOICE
            + self.choice.len() * COST_PAIR
            + self.bigram.len() * COST_PAIR
            + self.unigram.len() * COST_UNIGRAM
    }

    pub fn clear(&mut self) {
        *self = Self::new(self.fingerprint);
    }

    // Learning

    /// Record that the user accepted `word` for input `roman`, following `prev`.
    pub fn commit(&mut self, roman: &str, word: &str, prev: Option<&str>) {
        if word.is_empty() {
            return;
        }
        let w = self.weight;
        let wid = self.strings.intern(word);
        *self.unigram.entry(wid).or_insert(0.0) += w;

        let prev = prev
            .filter(|p| !p.is_empty())
            .map(|p| self.strings.intern(p));
        if !roman.is_empty() {
            let rid = self.strings.intern(&roman.to_lowercase());
            *self.choice.entry((rid, wid)).or_insert(0.0) += w;
            *self.roman_total.entry(rid).or_insert(0.0) += w;
            if let Some(pid) = prev {
                *self.ctx_choice.entry((pid, rid, wid)).or_insert(0.0) += w;
                *self.ctx_total.entry((pid, rid)).or_insert(0.0) += w;
            }
        }
        if let Some(pid) = prev {
            *self.bigram.entry((pid, wid)).or_insert(0.0) += w;
        }

        self.tick += 1.0;
        self.weight = (self.tick / HALF_LIFE_COMMITS).exp2();
        if self.weight > RESCALE_AT {
            self.rescale();
        }
        self.evict_if_needed();
    }

    /// Undo a [`Overlay::commit`]. An entry taken to zero is deleted, so the word
    /// returns to neutral rather than being demoted.
    ///
    /// Subtracts the current increment weight rather than what `commit` added: a
    /// correction lands a keystroke or two later, so the error is under a part in a
    /// thousand. `tick` is not rewound — that would shift every other entry's decay.
    pub fn retract(&mut self, roman: &str, word: &str, prev: Option<&str>) {
        // Lookups, never interning: forgetting must not create strings.
        let Some(wid) = self.strings.lookup(word) else {
            return;
        };
        let w = self.weight;
        let prev = prev
            .filter(|p| !p.is_empty())
            .and_then(|p| self.strings.lookup(p));

        take(&mut self.unigram, wid, w);
        if let Some(pid) = prev {
            take(&mut self.bigram, (pid, wid), w);
        }

        if let Some(rid) = self.strings.lookup(&roman.to_lowercase()) {
            // The denominator loses exactly what the numerator did.
            let taken = take(&mut self.choice, (rid, wid), w);
            take(&mut self.roman_total, rid, taken);
            if let Some(pid) = prev {
                let taken = take(&mut self.ctx_choice, (pid, rid, wid), w);
                take(&mut self.ctx_total, (pid, rid), taken);
            }
        }
    }

    /// Bring weights back into `f32` range without changing their ratios.
    fn rescale(&mut self) {
        // A generic fn, not a closure: a closure fixes one key type, and these six maps
        // have four between them.
        fn scale<K>(m: &mut HashMap<K, f32>, by: f32) {
            m.values_mut().for_each(|v| *v *= by);
        }
        let by = 1.0 / self.weight;
        scale(&mut self.ctx_choice, by);
        scale(&mut self.choice, by);
        scale(&mut self.bigram, by);
        scale(&mut self.unigram, by);
        scale(&mut self.roman_total, by);
        scale(&mut self.ctx_total, by);
        self.tick = 0.0;
        self.weight = 1.0;
    }

    fn evict_if_needed(&mut self) {
        /* Drop the weakest by RANK, never by comparing against a cutoff value: weights
        tie constantly early on, and every entry tied at a cutoff fails a strict
        comparison at once, emptying most of the map instead of trimming it. */
        fn trim<K: std::hash::Hash + Eq>(m: &mut HashMap<K, f32>) {
            if m.len() <= MAX_ENTRIES {
                return;
            }
            let keep = m.len() - m.len() / EVICT_FRACTION;
            let mut items: Vec<(K, f32)> = m.drain().collect();
            items.sort_by(|a, b| b.1.total_cmp(&a.1));
            items.truncate(keep);
            m.extend(items);
        }
        let before = self.len();
        trim(&mut self.ctx_choice);
        trim(&mut self.choice);
        trim(&mut self.bigram);
        trim(&mut self.unigram);
        if self.len() == before {
            return;
        }
        // The denominators still carry the weight of everything just dropped, which
        // would understate every survivor's probability.
        self.rebuild_totals();
        self.compact();
    }

    /// Drop interned strings no map references, renumbering the rest.
    ///
    /// Ids are positional, so this rewrites every key — hence eviction-time, not
    /// commit-time. Without it the table keeps the strings of everything evicted.
    fn compact(&mut self) {
        let live = self.live_ids();
        if live.len() == self.strings.len() {
            return;
        }
        let map = StringTable::dense_remap(&live);
        let id = |old: &Id| map[old];

        self.ctx_choice = remap(&self.ctx_choice, |&(p, r, w)| (id(&p), id(&r), id(&w)));
        self.choice = remap(&self.choice, |&(r, w)| (id(&r), id(&w)));
        self.bigram = remap(&self.bigram, |&(p, w)| (id(&p), id(&w)));
        self.unigram = remap(&self.unigram, |w| id(w));
        self.strings.retain(&live);
        self.rebuild_totals();
    }

    /// Rewrite a map's keys under a renumbering. The values are untouched.
    fn rebuild_totals(&mut self) {
        self.roman_total.clear();
        self.ctx_total.clear();
        for (&(r, _), &v) in &self.choice {
            *self.roman_total.entry(r).or_insert(0.0) += v;
        }
        for (&(p, r, _), &v) in &self.ctx_choice {
            *self.ctx_total.entry((p, r)).or_insert(0.0) += v;
        }
    }
}
