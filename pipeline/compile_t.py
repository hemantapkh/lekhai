"""Compile T, the transliteration transducer, from trained/learned_chunks.tsv.

T is the closure of every (roman chunk : Devanagari chunk) arc, weighted -log P plus the
knobs below. CHUNKS=path reads a different table. Writes out/translit.fst.
"""
import math
import os
import sys

import pynini
from pynini.lib import pynutil

import corpus

CHUNKS = (os.environ.get("CHUNKS")
          or os.path.join(corpus.ROOT, "trained", "learned_chunks.tsv"))

# Without these the decoder drops letters to reach a shorter, more frequent word.
DEL_PEN = float(os.environ.get("DEL_PEN", "4"))
LEN_BONUS = float(os.environ.get("LEN_BONUS", "0.5"))
# Nasalisation is usually not typed (`yaha` for यहाँ), so a vowel chunk also gets a
# nasalised variant and the lexicon picks between them.
NASAL_PEN = float(os.environ.get("NASAL_PEN", "2.5"))
VOWEL_END = set("ािीुूृेैोौअआइईउऊएऐओऔ")
NASALS = ["ँ", "ं"]


def weighted_chunks():
    """(roman, Devanagari, cost) for every learned mapping, plus its nasalised variants."""
    with open(CHUNKS, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")   # roman, Devanagari, count, P
            if len(parts) != 4:
                continue
            xc, yc, prob = parts[0], parts[1], float(parts[3])
            # An arc that reads no roman would let the closure loop without consuming input.
            if xc == "" or prob <= 0:
                continue
            w = -math.log(prob) + (DEL_PEN if yc == "" else -LEN_BONUS * len(yc))
            yield xc, yc, w
            if NASAL_PEN and yc and yc[-1] in VOWEL_END:
                for nasal in NASALS:
                    yield xc, yc + nasal, w + NASAL_PEN


def transducer(chunks):
    def accept(s):
        return pynini.accep(s, token_type="utf8")

    T = None
    for xc, yc, w in chunks:
        arc = pynutil.add_weight(pynini.cross(accept(xc), accept(yc)), w)
        T = arc if T is None else pynini.union(T, arc)
    if T is None:
        sys.exit(f"{CHUNKS} holds no usable mappings")
    return T.closure(1).optimize()


def main():
    arcs = list(weighted_chunks())
    T = transducer(arcs)
    corpus.stage_assets()
    T.write(os.path.join(corpus.OUT, "translit.fst"))
    print(f"==> out/translit.fst: {len(arcs)} arcs, {T.num_states()} states "
          f"(DEL_PEN={DEL_PEN} LEN_BONUS={LEN_BONUS} NASAL_PEN={NASAL_PEN})")


if __name__ == "__main__":
    main()
