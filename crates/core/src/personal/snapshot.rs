//! The binary snapshot format — the only part of the overlay that outlives the process.
//!
//! Strings are interned into a table and referenced by index; export writes only the
//! ids still referenced, reclaiming what eviction freed.
//!
//! Every read is bounds-checked: this blob comes back from the app's storage, so a
//! truncated or hostile one must be refused, never panic and never half-apply.

use super::strings::StringTable;
use super::{Id, Overlay};
use crate::params::{HALF_LIFE_COMMITS, MAX_SNAPSHOT_COUNT};
use std::collections::HashMap;
use std::hash::Hash;

/// Format tag. Bump the digits and older snapshots are refused rather than misread.
const MAGIC: &[u8; 8] = b"LEKHAI02";

/// Smallest a record can encode to: a string is at least its `u16` length prefix, a map
/// entry at least a `u32` id plus its `f32`. Bounds what a declared count may reserve,
/// since a blob cannot hold more records than it has bytes to spell them.
const MIN_STRING_BYTES: usize = 2;
const MIN_ENTRY_BYTES: usize = 8;

/// Export buffer-sizing estimates. Being wrong costs a realloc.
const BYTES_PER_ENTRY: usize = 14;
const BYTES_PER_STRING: usize = 10;
const BYTES_HEADER: usize = 64;

impl Overlay {
    /// A compact, fingerprinted snapshot of everything learned.
    pub fn export(&self) -> Vec<u8> {
        let used = self.live_ids();
        let remap = StringTable::dense_remap(&used);

        let mut b = Vec::with_capacity(
            self.len() * BYTES_PER_ENTRY + used.len() * BYTES_PER_STRING + BYTES_HEADER,
        );
        b.extend_from_slice(MAGIC);
        b.extend_from_slice(&self.fingerprint.to_le_bytes());
        b.extend_from_slice(&self.tick.to_le_bytes());

        b.extend_from_slice(&(used.len() as u32).to_le_bytes());
        for &old in &used {
            let s = self.strings.get(old).unwrap_or_default();
            b.extend_from_slice(&(s.len() as u16).to_le_bytes());
            b.extend_from_slice(s.as_bytes());
        }

        put_map(&mut b, &self.ctx_choice, |b, &(p, r, w)| {
            put_id(b, remap[&p]);
            put_id(b, remap[&r]);
            put_id(b, remap[&w]);
        });
        put_map(&mut b, &self.choice, |b, &(r, w)| {
            put_id(b, remap[&r]);
            put_id(b, remap[&w]);
        });
        put_map(&mut b, &self.bigram, |b, &(p, w)| {
            put_id(b, remap[&p]);
            put_id(b, remap[&w]);
        });
        put_map(&mut b, &self.unigram, |b, &w| put_id(b, remap[&w]));
        b
    }

    /// The string ids still referenced by some map, sorted. Anything absent was
    /// orphaned by eviction — export drops it, and [`Overlay::compact`] reclaims it.
    pub(super) fn live_ids(&self) -> Vec<Id> {
        let mut seen = vec![false; self.strings.len()];
        let mut used = Vec::new();
        let mut note = |id: Id| {
            if let Some(flag) = seen.get_mut(id as usize) {
                if !*flag {
                    *flag = true;
                    used.push(id);
                }
            }
        };
        for &(p, r, w) in self.ctx_choice.keys() {
            note(p);
            note(r);
            note(w);
        }
        for &(r, w) in self.choice.keys() {
            note(r);
            note(w);
        }
        for &(p, w) in self.bigram.keys() {
            note(p);
            note(w);
        }
        for &w in self.unigram.keys() {
            note(w);
        }
        used.sort_unstable();
        used
    }

    /// Restore a snapshot, replacing everything held.
    ///
    /// False, and the overlay untouched, if the blob is malformed, truncated, or was
    /// learned against a different model — a mismatch is refused rather than merged. The
    /// work happens in a fresh overlay swapped in only on success.
    pub fn import(&mut self, bytes: &[u8], fingerprint: u64) -> bool {
        match Self::parse(bytes, fingerprint) {
            Some(fresh) => {
                *self = fresh;
                true
            }
            None => false,
        }
    }

    fn parse(bytes: &[u8], fingerprint: u64) -> Option<Self> {
        let mut c = Cursor::new(bytes);
        if c.take(MAGIC.len())? != MAGIC.as_slice() || c.u64()? != fingerprint {
            return None;
        }
        let tick = c.f32()?;

        let mut fresh = Overlay::new(fingerprint);
        let count = c.count()?;
        fresh.strings.reserve(c.bounded(count, MIN_STRING_BYTES));
        for _ in 0..count {
            let len = c.u16()? as usize;
            let s = std::str::from_utf8(c.take(len)?).ok()?.to_string();
            fresh.strings.push(s);
        }
        let n = count as Id;

        fresh.ctx_choice = read_map(&mut c, |c| Some((c.id(n)?, c.id(n)?, c.id(n)?)))?;
        fresh.choice = read_map(&mut c, |c| Some((c.id(n)?, c.id(n)?)))?;
        fresh.bigram = read_map(&mut c, |c| Some((c.id(n)?, c.id(n)?)))?;
        fresh.unigram = read_map(&mut c, |c| c.id(n))?;

        fresh.tick = tick;
        fresh.weight = (tick / HALF_LIFE_COMMITS).exp2();
        fresh.rebuild_totals();
        Some(fresh)
    }
}

fn put_id(b: &mut Vec<u8>, id: Id) {
    b.extend_from_slice(&id.to_le_bytes());
}

/// Length-prefixed map: count, then each key via `put_key` followed by its `f32`.
fn put_map<K>(b: &mut Vec<u8>, map: &HashMap<K, f32>, put_key: impl Fn(&mut Vec<u8>, &K)) {
    b.extend_from_slice(&(map.len() as u32).to_le_bytes());
    for (k, v) in map {
        put_key(b, k);
        b.extend_from_slice(&v.to_le_bytes());
    }
}

/// The inverse of [`put_map`]. `read_key` returning `None` — a malformed key, or one
/// pointing outside the string table — aborts the whole parse.
fn read_map<K: Eq + Hash>(
    c: &mut Cursor,
    read_key: impl Fn(&mut Cursor) -> Option<K>,
) -> Option<HashMap<K, f32>> {
    let count = c.count()?;
    let mut map = HashMap::with_capacity(c.bounded(count, MIN_ENTRY_BYTES));
    for _ in 0..count {
        let key = read_key(c)?;
        map.insert(key, c.f32()?);
    }
    Some(map)
}

/// Bounds-checked reader: every accessor returns `None` past the end, so a truncated
/// blob cannot panic or over-read.
struct Cursor<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, i: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i.checked_add(n)?)?;
        self.i += n;
        Some(s)
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    /// An entry count, refused above [`MAX_SNAPSHOT_COUNT`].
    fn count(&mut self) -> Option<usize> {
        let n = self.u32()? as usize;
        (n <= MAX_SNAPSHOT_COUNT).then_some(n)
    }

    /// A string-table index, refused if it points outside the table.
    fn id(&mut self, len: Id) -> Option<Id> {
        let id = self.u32()?;
        (id < len).then_some(id)
    }

    /// `count` clamped to how many records the remaining bytes could hold. The declared
    /// count is attacker-controlled; the bytes remaining are not.
    fn bounded(&self, count: usize, min_bytes_each: usize) -> usize {
        count.min(self.b.len().saturating_sub(self.i) / min_bytes_each)
    }
}
