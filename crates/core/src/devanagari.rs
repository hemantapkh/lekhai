//! What counts as a previous word for the context model.

const BLOCK: std::ops::RangeInclusive<u32> = 0x900..=0x97F;
const DANDA: u32 = 0x964;
const DOUBLE_DANDA: u32 = 0x965;

/// Whether `w` may be passed as `prev` to the context model.
///
/// Neither danda is in `context-words.tsv`, so letting one through would score as "no
/// context"; rejecting it gives the next token the sentence-start context instead.
///
/// ```
/// # use lekhai::devanagari::is_word;
/// assert!(is_word("नेपाल"));
/// assert!(!is_word("नेपाल।")); // trailing danda: a sentence ended here
/// assert!(!is_word("nepal")); // never transliterated
/// assert!(!is_word(""));
/// ```
pub fn is_word(w: &str) -> bool {
    !w.is_empty()
        && w.chars().all(|c| {
            let v = c as u32;
            BLOCK.contains(&v) && v != DANDA && v != DOUBLE_DANDA
        })
}
