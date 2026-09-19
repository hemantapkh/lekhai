"""Run `maturin develop` before this suite. `lekhai` imports a compiled Rust extension,
so it cannot be imported straight from the source tree."""

import lekhai
import pytest


def is_devanagari(text: str) -> bool:
    """True if every character is Nepali script, or a space.

    Tests check the script rather than comparing against an expected word, so that
    retraining the model cannot break them.
    """
    return all("\u0900" <= ch <= "\u097f" or ch.isspace() for ch in text)


@pytest.fixture(scope="module")
def e():
    return lekhai.Engine()
