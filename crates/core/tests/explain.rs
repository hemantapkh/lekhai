//! `explain`: the per-stage diagnostic.

mod common;

/// `explain` repeats `analyze`'s stage order, so the two must agree on the answer.
/// If a stage is ever added to one and not the other, this is what catches it.
#[test]
fn explain_matches_analyze() {
    let e = common::engine();
    for word in ["pani", "namaste", "ghar", "ramro", "2081", ".", "xyzqw"] {
        for prev in [None, Some("तातो")] {
            let a = e.analyze(word, prev).unwrap();
            let x = e.explain(word, prev).unwrap();
            assert_eq!(x.candidates, a.candidates, "{word} prev={prev:?}");
            assert_eq!(x.candidates.first().unwrap(), &a.best, "{word}");
        }
    }
}

/// The costs are the point: a diagnostic that cannot say how close the call was is not
/// diagnosing anything.
#[test]
fn explain_reports_ranked_costs() {
    let e = common::engine();
    let x = e.explain("pani", None).unwrap();
    assert!(x.is_roman);
    assert!(x.after_lexicon.len() > 1, "expected a strip to compare");
    let costs: Vec<f32> = x.after_lexicon.iter().map(|(_, c)| *c).collect();
    assert!(
        costs.windows(2).all(|w| w[0] <= w[1]),
        "after_lexicon must be ranked, got {costs:?}"
    );
    assert!(!x.rule_spelling.is_empty());
    // Context still ran: no `prev` is the sentence-initial context, not an absent one.
    assert!(x.after_context.is_some());
}

/// A token the FST never sees reports that, rather than an empty stage list a caller
/// would have to interpret.
#[test]
fn explain_marks_the_non_roman_path() {
    let e = common::engine();
    let x = e.explain("2081", None).unwrap();
    assert!(!x.is_roman);
    assert!(x.after_lexicon.is_empty());
    assert_eq!(x.candidates, vec!["२०८१".to_string()]);
}

/// `T` alone proposes spellings that are not words; `L` is what makes them words.
/// The two stages must be visibly different or the diagnostic is not showing a stage.
#[test]
fn explain_separates_the_transducer_from_the_lexicon() {
    let e = common::engine();
    let x = e.explain("mero", None).unwrap();

    assert!(!x.after_translit.is_empty());
    let translit: Vec<&str> = x.after_translit.iter().map(|(w, _)| w.as_str()).collect();
    let lexical: Vec<&str> = x.after_lexicon.iter().map(|(w, _)| w.as_str()).collect();
    assert_ne!(
        translit, lexical,
        "T and T∘L should not produce the same list"
    );

    // Deduped, like the strip a caller sees.
    let mut seen = translit.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen.len(),
        translit.len(),
        "after_translit repeats a spelling"
    );

    // Both stages agree on the winner here; the lexicon reorders what follows it.
    assert_eq!(translit[0], lexical[0]);
}
