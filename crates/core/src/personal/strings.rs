//! String interning for the overlay.
//!
//! Ids are positional — the id *is* the index — which is what makes them cheap, and also
//! why they cannot be renumbered in place: every map key would have to be rewritten.
//! Compaction happens on export instead.

use super::Id;
use std::collections::HashMap;

/// `String`'s header, on top of its bytes. Part of the footprint estimate.
const COST_STRING: usize = 24;

#[derive(Default)]
pub struct StringTable {
    by_id: Vec<String>,
    by_str: HashMap<String, Id>,
}

impl StringTable {
    /// The id for `s`, adding it if unseen.
    pub fn intern(&mut self, s: &str) -> Id {
        if let Some(&id) = self.by_str.get(s) {
            return id;
        }
        let id = self.by_id.len() as Id;
        self.by_id.push(s.to_string());
        self.by_str.insert(s.to_string(), id);
        id
    }

    /// The id for `s`, or `None` if never interned. The read-side counterpart to
    /// [`StringTable::intern`], for the scoring path, which must not mutate.
    pub fn lookup(&self, s: &str) -> Option<Id> {
        self.by_str.get(s).copied()
    }

    pub fn get(&self, id: Id) -> Option<&str> {
        self.by_id.get(id as usize).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Approximate heap bytes held.
    pub fn footprint(&self) -> usize {
        self.by_id.iter().map(|s| s.len() + COST_STRING).sum()
    }

    /// Append a string whose id is its position. The import path, where the table is
    /// rebuilt in the order the snapshot wrote it.
    pub fn push(&mut self, s: String) {
        self.by_str.insert(s.clone(), self.by_id.len() as Id);
        self.by_id.push(s);
    }

    pub fn reserve(&mut self, n: usize) {
        self.by_id.reserve(n);
    }

    /// Keep only `used`, in order, renumbered to their new positions. The in-memory
    /// counterpart to what export does on the way out.
    pub fn retain(&mut self, used: &[Id]) {
        let mut kept = Vec::with_capacity(used.len());
        for &id in used {
            if let Some(s) = self.by_id.get(id as usize) {
                kept.push(s.clone());
            }
        }
        self.by_str.clear();
        for (new, s) in kept.iter().enumerate() {
            self.by_str.insert(s.clone(), new as Id);
        }
        self.by_id = kept;
    }

    /// A dense old -> new renumbering over `used`, which must be sorted. Export writes
    /// only those strings, so their ids close up over the gaps eviction left.
    pub fn dense_remap(used: &[Id]) -> HashMap<Id, Id> {
        used.iter()
            .enumerate()
            .map(|(new, &old)| (old, new as Id))
            .collect()
    }
}
