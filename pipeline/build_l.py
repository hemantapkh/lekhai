"""Build L, the lexicon acceptor, weighting each word LAM * -log P.

Counts the news crawl into data/nepali_lm_lexicon.tsv, which build_c.py and eval.py read
too, then writes out/lexicon.fst. LEXICON=path counts nothing and uses that list instead;
MAX_WORDS=N keeps the N most frequent words, for a quick build.
"""
import math
import os

import pynini

import corpus

LAM = float(os.environ.get("LAM", "0.7"))
MAX_WORDS = int(os.environ.get("MAX_WORDS", "0"))


def word_counts():
    path = os.environ.get("LEXICON")
    if path:
        return corpus.read_lexicon(path)
    freq = corpus.lexicon_counts()
    with open(corpus.LEXICON, "w", encoding="utf-8") as out:
        out.writelines(f"{word}\t{count}\n"
                       for word, count in sorted(freq.items(), key=lambda x: -x[1]))
    return freq


def acceptor(entries):
    """Built as a trie, then minimised: repeated union + optimize is superlinear here."""
    logtot = math.log(sum(c for _, c in entries))
    L = pynini.Fst()
    start = L.add_state()
    L.set_start(start)
    children = {start: {}}
    one = pynini.Weight.one("tropical")
    for word, c in entries:
        st = start
        for ch in word:
            lab = ord(ch)
            nxt = children[st].get(lab)
            if nxt is None:
                nxt = L.add_state()
                L.add_arc(st, pynini.Arc(lab, lab, one, nxt))
                children[st][lab] = nxt
                children[nxt] = {}
            st = nxt
        L.set_final(st, pynini.Weight("tropical", LAM * (logtot - math.log(c))))
    return L.optimize()


def main():
    entries = sorted(((w, c) for w, c in word_counts().items() if w and c > 0),
                     key=lambda p: -p[1])
    if MAX_WORDS:
        entries = entries[:MAX_WORDS]
    L = acceptor(entries)
    corpus.stage_assets()
    L.write(os.path.join(corpus.OUT, "lexicon.fst"))
    print(f"==> data/nepali_lm_lexicon.tsv and out/lexicon.fst: "
          f"{len(entries)} words, {L.num_states()} states (LAM={LAM})")


if __name__ == "__main__":
    main()
