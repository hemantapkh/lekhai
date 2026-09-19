//! Every number in the engine that was chosen rather than derived.
//!
//! Together in one file because they are swept together by `pipeline/eval*.py`.

// Decode

/// Convergence delta for the n-shortest search (OpenFst's default magnitude).
pub const SP_DELTA: f32 = 1.0e-6;

/// Suggestion-strip depth. Eight fits a phone keyboard's candidate bar.
pub const MAX_CANDIDATES: usize = 8;

/// Multiple of the wanted count to ask the n-shortest search for, before deduping by
/// output string. Distinct input paths spell the same word, so a 1x ask under-generates.
pub const NBEST_OVERGEN: usize = 3;

// Context

/// How far behind the best transliteration cost a candidate may be and still be
/// rerankable by context, in nats. Swept on held-out data: mid-sentence 72.4% -> 81.8%,
/// sentence-initial 67.1% -> 70.2%. Tighter excludes contextually right spellings.
pub const CONTEXT_MARGIN: f32 = 6.0;

// Learning

/// Commits after which a choice carries half the weight of a fresh one.
pub const HALF_LIFE_COMMITS: f32 = 2000.0;

/// Cap on how far personalization may shift a candidate, in nats.
pub const BOOST_CAP: f32 = 6.0;

/// Shrinkage constant: a preference backed by `n` choices is trusted `n / (n + K)` of
/// the way.
pub const SHRINK_K: f32 = 2.0;

/// Share of the choice estimate taken from the `(prev, roman)` observation rather than
/// the context-free one, when both exist.
pub const CTX_BLEND: f32 = 0.7;

/// The familiarity term: `FAMILIAR_SCALE * ln(1 + unigram + FAMILIAR_BIGRAM_WEIGHT *
/// bigram)`, clamped to `FAMILIAR_CAP`. Must stay small enough never to decide alone.
pub const FAMILIAR_SCALE: f32 = 0.25;
pub const FAMILIAR_BIGRAM_WEIGHT: f32 = 2.0;
pub const FAMILIAR_CAP: f32 = 1.0;

/// Evidence a learned word needs before injection, so one stray commit cannot put a
/// wrong word in the strip.
pub const INJECT_MIN_EVIDENCE: f32 = 1.5;

// Bounds

/// Entries any single learned map may hold before eviction.
pub const MAX_ENTRIES: usize = 20_000;

/// Denominator of the fraction dropped per eviction, weakest first. Evicting one entry
/// at a time would re-sort on nearly every commit once the cap is reached.
pub const EVICT_FRACTION: usize = 4;

/// Rescale the increment weight once it passes this, to stay well inside `f32` range.
pub const RESCALE_AT: f32 = 1.0e18;

/// Ceiling on any count read from a snapshot.
pub const MAX_SNAPSHOT_COUNT: usize = 4_000_000;
