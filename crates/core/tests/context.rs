//! The word-bigram context stage.

mod common;

/// Context must be selective: a model that reranks every ambiguous token is as wrong as
/// one that never does. The cases below pair one input with previous words that carry
/// strong bigram evidence and with previous words that carry none; only the first group
/// may move it off the frequency default.
#[test]
fn context_reranks_selectively() {
    let e = common::engine();
    let default = e.best("pani", None).expect("no ctx");

    for prev in ["तातो", "चिसो"] {
        assert_ne!(
            e.best("pani", Some(prev)).expect("ctx"),
            default,
            "{prev} carries bigram evidence and should rerank pani"
        );
    }

    for prev in ["खाना", "मा", "तिमी", "एउटा", "कोही"] {
        assert_eq!(
            e.best("pani", Some(prev)).expect("ctx"),
            default,
            "{prev} carries none and should leave pani alone"
        );
    }

    // A word absent from context-words.tsv misses the lookup entirely, which is a
    // different path from having a weak bigram, and must be just as harmless.
    assert_eq!(e.best("pani", Some("ज्झक्क")).expect("unknown prev"), default);
}

/// Sentence position is a real context, not the absence of one: the same token resolves
/// one way opening a sentence and another way after a noun. Nothing here is a rule for
/// that token; it falls out of the sentence-initial counts in `C`.
#[test]
fn sentence_start_is_a_context() {
    let e = common::engine();
    let initial = e.best("ma", None).expect("start");

    for prev in ["घर", "नेपाल", "काठमाडौं"] {
        assert_ne!(
            e.best("ma", Some(prev)).expect("mid"),
            initial,
            "{prev} + ma must not resolve as sentence-initial ma does"
        );
    }
}
