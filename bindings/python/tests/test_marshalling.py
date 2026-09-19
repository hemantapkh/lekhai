"""Checks the type and structure of what each method returns after crossing from Rust.

Whether the ranking is *correct* is tested in Rust, in `crates/core/tests/`."""

import lekhai
from conftest import is_devanagari

EXPLAIN_KEYS = {
    "token",
    "prev",
    "is_roman",
    "context_is_live",
    "after_translit",
    "after_lexicon",
    "after_context",
    "after_personal",
    "rule_spelling",
    "candidates",
}


def test_decoding_produces_devanagari(e):
    """Checks the script, not the spelling: retraining the model must not break this."""
    out = e.transliterate("mero naam ke ho")
    assert len(out.split()) == 4
    assert is_devanagari(out)


def test_a_real_prev_crosses_and_comes_back(e):
    """Passing a real `prev` is the only case where Rust receives `Some(...)` instead of
    None. `explain` returns `prev` unchanged, so this checks it survives the round trip."""
    x = e.explain("pani", "तातो")
    assert x["prev"] == "तातो"
    assert x["is_roman"] is True


def test_candidates_is_a_non_empty_list_of_str(e):
    c = e.candidates("pani")
    assert isinstance(c, list) and c
    assert all(isinstance(w, str) for w in c)


def test_analyze_is_the_pair_of_the_other_two(e):
    a = e.analyze("pani")
    assert isinstance(a, tuple) and len(a) == 2
    assert a == (e.best("pani"), e.candidates("pani"))


def test_explain_is_a_dict_with_every_documented_key(e):
    """Rust spells these keys out by hand, so a typo still compiles and only shows up
    here as a missing key."""
    x = e.explain("pani")
    assert isinstance(x, dict)
    assert set(x) == EXPLAIN_KEYS
    assert x["candidates"] == e.candidates("pani")
    assert all(
        isinstance(w, str) and isinstance(c, float) for w, c in x["after_lexicon"]
    )


def test_rust_none_arrives_as_none_not_an_empty_list(e):
    x = e.explain("pani")
    assert isinstance(x["after_context"], list)
    assert x["after_personal"] is None


def test_snapshots_cross_the_boundary_as_bytes():
    e = lekhai.Engine()
    for _ in range(50):
        e.commit("kaam", "काम", None)

    blob = e.export_learned()
    assert isinstance(blob, bytes) and blob
    assert isinstance(e.learned_count, int) and e.learned_count > 0
    assert isinstance(e.learned_footprint, int)

    f = lekhai.Engine()
    assert f.import_learned(blob) is True
    assert f.learned_count == e.learned_count
    assert f.import_learned(b"not a snapshot") is False
    assert f.learned_count == e.learned_count, "a refused blob must change nothing"

    f.clear_learned()
    assert f.learned_count == 0
