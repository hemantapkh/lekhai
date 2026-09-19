"""Checks that the installed package contains every file it needs to work.

Paths come from `lekhai.__file__`, so this covers the source tree during development,
and a real wheel if you run the suite after `pip install`."""

from pathlib import Path

import lekhai
from conftest import is_devanagari


def test_models_ship_with_the_package():
    d = Path(lekhai.ASSETS_DIR)
    for name in (
        "translit.fst",
        "lexicon.fst",
        "vocab.json",
        "rules.json",
        "context.fst",
    ):
        f = d / name
        assert f.is_file(), f"{name} missing from the installed package"
        assert f.stat().st_size > 0, f"{name} is empty"


def test_the_typing_files_ship():
    """`py.typed` is an empty file, but PEP 561 says type checkers must ignore a
    package's annotations unless it is present. Do not delete it."""
    d = Path(lekhai.__file__).parent
    for name in ("py.typed", "_lekhai.pyi"):
        assert (d / name).is_file(), f"{name} missing from the installed package"


def test_engine_loads_with_no_arguments():
    out = lekhai.Engine().best("namaste")
    assert out and is_devanagari(out)
