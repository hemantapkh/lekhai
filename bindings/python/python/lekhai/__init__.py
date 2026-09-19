"""English to Nepali transliteration engine.

from lekhai import Engine
e = Engine()                          # the bundled model, loaded once
e.best("namaste")                     # 'नमस्ते'
e.transliterate("mero naam ke ho")    # 'मेरो नाम के हो'

"""

from __future__ import annotations

import os
from importlib import metadata
from typing import Any

from ._lekhai import Engine as _RustEngine

__version__ = metadata.version("lekhai")

__all__ = ["ASSETS_DIR", "Engine", "__version__"]

ASSETS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "assets")


class Engine:
    """A loaded transliteration engine.

    Args:
        assets: directory holding the model files. Defaults to the bundled model.
    """

    def __init__(self, assets: str | None = None):
        self._assets = assets or ASSETS_DIR
        self._inner = _RustEngine(self._assets)

    @property
    def context_is_live(self) -> bool:
        """Whether previous-word context is in effect.

        Answered by the engine, not guessed from filenames: the model can be present
        and still have failed to load, since context is dropped on any problem rather
        than refusing to start.
        """
        return self._inner.context_is_live

    def _check(self, method: str, word: str, prev: str | None) -> None:
        """Reject the two mistakes that otherwise succeed and return something wrong."""
        if any(c.isspace() for c in word):
            raise ValueError(
                f"{method}() takes one token, but {word!r} contains whitespace. Use transliterate() for a line."
            )
        if prev and prev.isascii():
            # Only ASCII is checked: the full rule is `devanagari::is_word` in the
            # engine, and a third copy would be a third thing to keep in step.
            raise ValueError(
                f"prev must be the previously committed Devanagari word, not roman: {prev!r}"
            )

    def best(self, word: str, prev: str | None = None) -> str:
        """The committed transliteration of one token.

        Args:
            word: one roman token, no whitespace.
            prev: the previously committed **Devanagari** word, not the roman that
                produced it. None at a sentence start.

        Raises:
            ValueError: if `word` contains whitespace, or `prev` is roman.
        """
        self._check("best", word, prev)
        return self._inner.best(word, prev)

    def analyze(self, word: str, prev: str | None = None) -> tuple[str, list[str]]:
        """The winner and the strip together, from one decode.

        Args:
            word: one roman token, no whitespace.
            prev: as in `best`.

        Returns:
            `(best, candidates)`. `best` is always `candidates[0]`.
        """
        self._check("analyze", word, prev)
        return self._inner.analyze(word, prev)

    def candidates(self, word: str, prev: str | None = None) -> list[str]:
        """The ranked suggestion strip for one token.

        Args:
            word: one roman token, no whitespace.
            prev: as in `best`.

        Returns:
            Best first, never empty — a token the engine cannot transliterate comes
            back as itself.
        """
        self._check("candidates", word, prev)
        return self._inner.candidates(word, prev)

    def explain(self, word: str, prev: str | None = None) -> dict[str, Any]:
        """Why one token ranked the way it did, stage by stage.

        A diagnostic; it costs a second decode. Costs are in nats, lower first, so the
        gap between the top two is how close the call was. Compare within a stage, not
        across one.

        Args:
            word: one roman token, no whitespace.
            prev: as in `best`.

        Returns:
            A dict with keys:

                token, prev (str)             as the pipeline saw them
                is_roman (bool)               False when the token never reached the FST
                context_is_live (bool)
                after_translit (list)         [(word, cost)] from the transducer alone
                after_lexicon (list)          [(word, cost)] after the lexicon
                after_context (list | None)   None if no context model is loaded
                after_personal (list | None)  None if nothing has been learned
                rule_spelling (str)           what rules.json alone spells
                candidates (list[str])        the strip a caller receives
        """
        self._check("explain", word, prev)
        return self._inner.explain(word, prev)

    def transliterate(self, text: str) -> str:
        """Transliterate a whole line, threading context between tokens.

        Args:
            text: a line. Splits on the space character only.

        Returns:
            Each token replaced by its best candidate. A result that is not a
            Devanagari word resets the context to sentence-start.
        """
        return self._inner.transliterate(text)

    def commit(self, roman: str, word: str, prev: str | None = None) -> None:
        """Record that the user accepted `word` for input `roman`.

        Repeated choices promote a word and never demote anything, and the boost is
        capped: learning breaks close calls, it does not override a confident answer.
        In memory — nothing is persisted until `export_learned()`.

        Args:
            roman: the token the user typed, no whitespace.
            word: the Devanagari they accepted.
            prev: the previously committed Devanagari word.

        Raises:
            ValueError: if `roman` contains whitespace, or `prev` is roman.
        """
        self._check("commit", roman, prev)
        self._inner.commit(roman, word, prev)

    def retract(self, roman: str, word: str, prev: str | None = None) -> None:
        """Undo a `commit`, for a word corrected right after it was taken.

        Removes evidence without adding the negative kind, and does nothing for a word
        never learned. Pair it with a `commit` of what the user meant.

        Args:
            roman: the token the user typed, no whitespace.
            word: the Devanagari to take back.
            prev: the previously committed Devanagari word.

        Raises:
            ValueError: if `roman` contains whitespace, or `prev` is roman.
        """
        self._check("retract", roman, prev)
        self._inner.retract(roman, word, prev)

    @property
    def learned_count(self) -> int:
        """Entries held, not words — one commit writes into several maps."""
        return self._inner.learned_count

    @property
    def learned_footprint(self) -> int:
        """Approximate heap bytes those entries occupy."""
        return self._inner.learned_footprint

    def export_learned(self) -> bytes:
        """An opaque snapshot of everything learned, for the caller to persist.

        Nothing in this package writes it anywhere, and nothing survives the process
        otherwise. The blob is fingerprinted against the loaded model.
        """
        return self._inner.export_learned()

    def import_learned(self, data: bytes) -> bool:
        """Restore a snapshot, replacing everything held.

        Replaces rather than merges, so two processes writing one file clobber each
        other.

        Args:
            data: a blob from `export_learned()`.

        Returns:
            False, and the engine untouched, if the blob is truncated, corrupt, or was
            learned against a different model.
        """
        return self._inner.import_learned(data)

    def clear_learned(self) -> None:
        """Forget everything learned, in memory. Deleting a snapshot is the caller's."""
        self._inner.clear_learned()

    def __repr__(self) -> str:
        bundled = " (bundled)" if self._assets == ASSETS_DIR else ""
        return f"Engine(assets={self._assets!r}{bundled})"
