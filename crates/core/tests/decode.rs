//! Decoding one token and one line, against the shipped assets.

mod common;

#[test]
fn returns_ranked_candidates() {
    let e = common::engine();
    // The shipped L is size-capped and yields a short strip, so assert the invariant
    // rather than a count: `best` is always the head of the list.
    for w in ["pani", "ramro", "kitab"] {
        let a = e.analyze(w, None).expect("analyze");
        assert!(!a.best.is_empty(), "{w}: best must not be empty");
        assert_eq!(
            a.candidates.first(),
            Some(&a.best),
            "{w}: best must lead candidates"
        );
    }
}

#[test]
fn digits_map_to_devanagari() {
    // Digits are localized by the rules layer, not passed through.
    let e = common::engine();
    assert_eq!(e.best("123", None).expect("digits"), "१२३");
}

#[test]
fn transliterates_a_sentence() {
    let e = common::engine();
    let out = e.transliterate("mero naam ke ho").expect("sentence");
    assert_eq!(out.split(' ').count(), 4, "word count preserved: {out}");
    assert!(
        out.chars().any(|c| ('\u{900}'..='\u{97F}').contains(&c)),
        "expected Devanagari output, got {out}"
    );
}

/// Each token's result must thread into the next as `prev`. Without it the two lines
/// below, which share a final token, would resolve it identically.
#[test]
fn sentence_threads_context_between_tokens() {
    let e = common::engine();

    let a = e.transliterate("tato pani").unwrap();
    let b = e.transliterate("khana pani").unwrap();
    assert_ne!(
        a.split(' ').nth(1),
        b.split(' ').nth(1),
        "{a} / {b}: pani must differ after different words"
    );

    // One token appearing twice with different answers proves the context is moving
    // between positions, not merely present.
    let out = e.transliterate("ma ghar ma").unwrap();
    let t: Vec<&str> = out.split(' ').collect();
    assert_ne!(
        t[0], t[2],
        "{out}: ma must differ initially and after a word"
    );
}

/// Spacing must survive the round trip: runs of spaces rejoin unchanged.
#[test]
fn sentence_preserves_spacing() {
    let e = common::engine();
    for s in ["", " ", "  ", " tato pani ", "tato  pani"] {
        let out = e.transliterate(s).expect("sentence");
        assert_eq!(
            out.split(' ').count(),
            s.split(' ').count(),
            "{s:?} -> {out:?}: token count must be preserved"
        );
    }
    let out = e.transliterate("tato  pani").unwrap();
    assert!(out.contains("  "), "{out}: the double space must survive");
}

/// Sentence punctuation is localized by the rules layer, the way digits are.
#[test]
fn punctuation_maps_to_devanagari() {
    let e = common::engine();
    assert_eq!(e.best(".", None).expect("danda"), "।");
}

/// The rules floor is what makes the strip total: a token `T ∘ L` rejects outright still
/// gets a spelling, so no caller has to handle an empty list.
#[test]
fn the_rules_floor_keeps_the_strip_non_empty() {
    let e = common::engine();
    for w in ["xyzqw", "zzzz", "qqqq"] {
        let c = e.candidates(w, None).expect("candidates");
        assert!(!c.is_empty(), "{w}: the rules floor must supply a spelling");
    }
}
