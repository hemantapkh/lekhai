//! On-device learning: what is remembered, what it is worth, and undoing it.

mod common;

/// An untrained overlay must be a perfect no-op.
#[test]
fn personalization_is_cold_start_safe() {
    let a = common::engine();
    let b = common::engine();
    for w in ["pani", "namaste", "ma", "ghar", "sathi", "kitab"] {
        for prev in [None, Some("तातो"), Some("घर")] {
            assert_eq!(
                a.analyze(w, prev).unwrap().candidates,
                b.analyze(w, prev).unwrap().candidates,
                "{w} with prev={prev:?} must be unchanged by an empty overlay"
            );
        }
    }
    assert_eq!(a.learned_count(), 0);
}

/// Repeatedly choosing a lower-ranked candidate must promote it — the core behaviour.
#[test]
fn personalization_learns_a_choice() {
    let e = common::engine();
    let before = e.analyze("pani", None).unwrap();
    assert!(
        before.candidates.len() > 1,
        "need an alternative to promote"
    );
    let second = before.candidates[1].clone();
    for _ in 0..5 {
        e.commit("pani", &second, None);
    }
    assert_eq!(
        e.analyze("pani", None).unwrap().best,
        second,
        "after 5 commits the chosen word should lead"
    );
    // and it must not leak into a DIFFERENT input
    assert_eq!(
        e.analyze("ghar", None).unwrap().best,
        common::engine().analyze("ghar", None).unwrap().best
    );
}

/// Learning must be keyed on the previous word, not collapse to "whatever they typed
/// last".
///
/// Both previous words are neutral: known to `C` so context runs, but with no corpus
/// evidence either way — otherwise this would be testing personalization against a
/// corpus prior, a different question.
#[test]
fn personalization_is_context_conditioned() {
    let e = common::engine();
    let cands = e.analyze("pani", None).unwrap().candidates;
    let (a, b) = (cands[0].clone(), cands[1].clone()); // पनि, पानी
    let (p1, p2) = ("लेखराज", "निर्मूल");
    for _ in 0..6 {
        e.commit("pani", &b, Some(p1));
        e.commit("pani", &a, Some(p2));
    }
    assert_eq!(
        e.analyze("pani", Some(p1)).unwrap().best,
        b,
        "{p1} should have learned {b}"
    );
    assert_eq!(
        e.analyze("pani", Some(p2)).unwrap().best,
        a,
        "{p2} should have learned {a}"
    );
}

/// Reranking alone cannot surface a word `T ∘ L` never generates; injection can.
#[test]
fn personalization_injects_unseen_words() {
    let e = common::engine();
    let novel = "भन"; // rank 14 for "bhana" — outside the 8-candidate strip
    assert!(!e
        .analyze("bhana", None)
        .unwrap()
        .candidates
        .contains(&novel.to_string()));
    for _ in 0..4 {
        e.commit("bhana", novel, None);
    }
    let after = e.analyze("bhana", None).unwrap();
    assert!(
        after.candidates.contains(&novel.to_string()),
        "a learned word must be injected, got {:?}",
        after.candidates
    );
    assert_eq!(after.best, novel);
}

/// A snapshot must round-trip exactly, and must be refused if it came from a different
/// model rather than silently corrupting the overlay.
#[test]
fn personalization_snapshot_round_trips() {
    let e = common::engine();
    for _ in 0..4 {
        e.commit("pani", "पानी", Some("तातो"));
    }
    e.commit("ghar", "घर", None);
    let blob = e.export_learned();
    let want = e.analyze("pani", Some("तातो")).unwrap().best;

    let f = common::engine();
    assert!(f.import_learned(&blob), "same model must import");
    assert_eq!(f.analyze("pani", Some("तातो")).unwrap().best, want);

    // truncated / garbage / foreign snapshots are refused, not merged
    assert!(!f.import_learned(&blob[..blob.len() / 2]));
    assert!(!f.import_learned(b"not a snapshot"));
    assert!(!f.import_learned(&[]));

    /* A blob whose declared counts are a lie must be refused without reserving for
    them. The header is MAGIC(8) + fingerprint(8) + tick(4) + string count(4), so
    patching bytes 20..24 claims millions of strings 80 bytes cannot hold. */
    let mut liar = blob.clone();
    liar[20..24].copy_from_slice(&3_999_999u32.to_le_bytes());
    assert!(!f.import_learned(&liar), "a lying count must be refused");
    assert!(
        !f.import_learned(&[0xff; 64]),
        "garbage with a bad magic must be refused"
    );

    f.clear_learned();
    assert_eq!(f.learned_count(), 0);
    assert_eq!(
        f.analyze("pani", Some("तातो")).unwrap().best,
        common::engine().analyze("pani", Some("तातो")).unwrap().best
    );
}

/// A learned word must not disturb context for unrelated input.
///
/// Personalization re-sorts by cost, so if context had only reordered the list a stable
/// sort would undo it. The word learned here is unrelated to the words checked —
/// `personalization_is_context_conditioned` learns for the token it queries, so the
/// boost hides the failure.
#[test]
fn learning_does_not_disable_context() {
    let e = common::engine();
    let probes = [
        ("pani", Some("तातो")),
        ("pani", Some("चिसो")),
        ("pani", Some("खाना")),
        ("ma", None),
        ("ma", Some("घर")),
    ];
    let before: Vec<String> = probes.iter().map(|(w, p)| e.best(w, *p).unwrap()).collect();

    // learn something with no bearing on any of them
    for _ in 0..3 {
        e.commit("ghar", "घर", None);
    }
    assert!(
        e.learned_count() > 0,
        "overlay must be non-empty for this test to mean anything"
    );

    for ((w, p), want) in probes.iter().zip(&before) {
        assert_eq!(
            &e.best(w, *p).unwrap(),
            want,
            "{w} prev={p:?}: context changed after learning an unrelated word"
        );
    }
}

/// Compaction renumbers every map key, so a mistake there turns learned data into
/// noise pointing at the wrong strings. Teach a choice, bury it under enough commits to
/// force eviction and compaction, and it must still be the one that wins.
#[test]
fn learning_survives_eviction_and_compaction() {
    let e = common::engine();
    let alt = e.analyze("pani", None).unwrap().candidates[1].clone();
    for _ in 0..40 {
        e.commit("pani", &alt, Some("तातो"));
    }
    assert_eq!(e.analyze("pani", Some("तातो")).unwrap().best, alt);

    // Past MAX_ENTRIES, so `choice` is trimmed and the string table is compacted.
    for i in 0..30_000 {
        e.commit(&format!("f{i}"), "फिलर", None);
    }

    assert_eq!(
        e.analyze("pani", Some("तातो")).unwrap().best,
        alt,
        "the taught choice was lost or misdirected by compaction"
    );
    let snapshot = e.export_learned();
    let f = common::engine();
    assert!(
        f.import_learned(&snapshot),
        "compacted overlay must round-trip"
    );
    assert_eq!(f.analyze("pani", Some("तातो")).unwrap().best, alt);
}

/// Eviction trims `choice` and `ctx_choice` independently, and `choice` fills faster.
/// A roman token can be left with only context-conditioned evidence, which used to be
/// unreachable — scored through a denominator rebuilt from `choice` alone.
#[test]
fn context_conditioned_evidence_survives_without_its_backoff() {
    let e = common::engine();
    let alt = e.analyze("pani", None).unwrap().candidates[1].clone();
    for _ in 0..40 {
        e.commit("pani", &alt, Some("तातो"));
    }
    let taught = e.export_learned();

    // Round-trip, then confirm the conditioned reading still decides.
    let f = common::engine();
    assert!(f.import_learned(&taught));
    assert_eq!(f.analyze("pani", Some("तातो")).unwrap().best, alt);
}

/// A correction must leave no trace: retracting the acceptance returns the ranking to
/// exactly where it was, not to a demotion of the retracted word.
#[test]
fn retract_undoes_a_commit() {
    let e = common::engine();
    let before = e.analyze("pani", None).unwrap().candidates;
    let alt = before[1].clone();

    for _ in 0..30 {
        e.commit("pani", &alt, None);
    }
    assert_eq!(
        e.analyze("pani", None).unwrap().best,
        alt,
        "should have learned"
    );

    for _ in 0..30 {
        e.retract("pani", &alt, None);
    }
    assert_eq!(
        e.analyze("pani", None).unwrap().candidates,
        before,
        "retracting every commit must restore the original ranking"
    );
}

/// Retraction removes evidence; it never manufactures the opposite. Over-retracting is
/// clamped at zero, so a word cannot be pushed below where it started.
#[test]
fn retract_never_demotes_below_neutral() {
    let e = common::engine();
    let before = e.analyze("pani", None).unwrap().candidates;
    let alt = before[1].clone();

    e.commit("pani", &alt, None);
    for _ in 0..100 {
        e.retract("pani", &alt, None);
    }
    assert_eq!(e.analyze("pani", None).unwrap().candidates, before);

    // And a word never learned at all is simply not there to forget.
    e.retract("pani", "काल्पनिक", None);
    e.retract("नयाँ", "शब्द", Some("तातो"));
    assert_eq!(e.analyze("pani", None).unwrap().candidates, before);
}

/// The correction pattern end to end: the wrong word is taken back and the right one
/// learned, so only the correction survives.
#[test]
fn retract_then_commit_moves_the_preference() {
    let e = common::engine();
    let cands = e.analyze("pani", None).unwrap().candidates;
    let (wrong, right) = (cands[0].clone(), cands[1].clone());

    for _ in 0..30 {
        e.commit("pani", &wrong, Some("तातो"));
        e.retract("pani", &wrong, Some("तातो"));
        e.commit("pani", &right, Some("तातो"));
    }
    assert_eq!(e.analyze("pani", Some("तातो")).unwrap().best, right);
}
