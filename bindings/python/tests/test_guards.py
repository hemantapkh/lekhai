"""Tests for the argument checks in `__init__.py`.

The Rust engine has no equivalent, so Python is the only place they can be tested."""

import pytest


@pytest.mark.parametrize("method", ["best", "analyze", "candidates", "explain"])
@pytest.mark.parametrize("token", ["mero naam", "tato pani", "a\tb", "a\nb"])
def test_whitespace_in_a_token_method_raises(e, method, token):
    with pytest.raises(ValueError, match="transliterate"):
        getattr(e, method)(token)


@pytest.mark.parametrize("method", ["best", "analyze", "candidates", "explain"])
def test_roman_prev_raises(e, method):
    with pytest.raises(ValueError, match="Devanagari"):
        getattr(e, method)("pani", "tato")


def test_the_learning_methods_are_guarded_too(e):
    with pytest.raises(ValueError, match="transliterate"):
        e.commit("tato pani", "पानी", None)
    with pytest.raises(ValueError, match="Devanagari"):
        e.retract("pani", "पानी", "tato")


def test_transliterate_is_exempt(e):
    assert e.transliterate("tato pani ho")
