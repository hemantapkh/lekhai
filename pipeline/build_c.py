"""Build C, the word-context stage of T ∘ L ∘ C, from the corpora.

C is a Katz word bigram with absolute discounting over integer word ids. An arc weight is
the adjustment to L's unigram term, not a probability:

    d(w|h) = SCALE * ( log P_uni(w) - log P_katz(w|h) )

States are the previous word, plus U for "no evidence" and <s> for a sentence start.
Backing off to U costs -SCALE * log(bow(h)) rather than nothing.

out/context.fst and out/context-words.tsv must ship together: the arc labels in one are
the ids in the other.
"""
import collections
import itertools
import math
import os

import pynini

import corpus

EPS = 0

DISCOUNT = float(os.environ.get("DISCOUNT", "0.75"))
SCALE = float(os.environ.get("SCALE", "2.0"))
# The log ratio is unbounded: uncapped, one arc built from a handful of co-occurrences
# outweighs the transliteration cost, and C decides spellings instead of breaking ties.
KCAP = float(os.environ.get("KCAP", "3.0"))
# Two floors: below MIN_BIGRAM a pair is dropped entirely, below MINBI it still counts
# towards its history's total but gets no arc of its own.
MIN_BIGRAM = int(os.environ.get("MIN_BIGRAM", "3"))
MINBI = float(os.environ.get("MINBI", "5"))


def count_bigrams(id_of):
    """Adjacent in-lexicon word pairs, commonest first. A danda tokenizes as a word but is
    a clause boundary, so no pair may span it."""
    counts = collections.Counter()
    news_train, _ = corpus.news_split()
    for name in corpus.CORPORA:
        for sentence in (news_train if name == corpus.NEWS else corpus.sentences(name)):
            words = [None if t in corpus.DANDAS else t
                     for t in corpus.DEVANAGARI.findall(sentence)]
            for a, b in itertools.pairwise(words):
                if a in id_of and b in id_of:
                    counts[a, b] += 1
    return sorted(((k, c) for k, c in counts.items() if c >= MIN_BIGRAM), key=lambda kv: -kv[1])


def count_starts(id_of):
    """How often each in-lexicon word opens a sentence. Without it the start state is
    neutral, and sentence-initial `ma` decodes as मा rather than म."""
    news_train, _ = corpus.news_split()
    starts = collections.Counter()
    for sentence in news_train:
        first = sentence.split()[:1]
        if first and corpus.DEVANAGARI.fullmatch(first[0]) and first[0] in id_of:
            starts[first[0]] += 1
    return starts.most_common(), len(news_train)


def histories(id_of):
    following = collections.defaultdict(dict)
    total = collections.defaultdict(int)
    for (a, b), c in count_bigrams(id_of):
        following[a][b] = c
        total[a] += c
    return following, total


def clamp(x):
    return max(-KCAP, min(KCAP, x))


def weight(x):
    return pynini.Weight("tropical", float(x))


def build(uni, id_of, following, total, starts, nsent):
    corpus_total = sum(uni.values())

    C = pynini.Fst()
    U = C.add_state()
    S = C.add_state() if nsent else U
    C.set_start(S)
    state = {h: C.add_state() for h in following}

    # No bigram evidence means L already has it right, so the adjustment is zero. Only
    # words owning a history need an arc: it is how the decoder reaches that state.
    for h in following:
        C.add_arc(U, pynini.Arc(id_of[h], id_of[h], weight(0.0), state[h]))

    def add_history(from_state, counts, seen):
        if seen <= 0:
            C.add_arc(from_state, pynini.Arc(EPS, EPS, weight(0.0), U))
            return
        bow = max(DISCOUNT * sum(1 for w in counts if w in id_of) / seen, 1e-12)
        for w, c in counts.items():
            if w not in id_of or c < MINBI:
                continue
            p_uni = uni[w] / corpus_total
            p_katz = max(c - DISCOUNT, 0.0) / seen + bow * p_uni
            if p_katz > 0:
                d = clamp(SCALE * (math.log(p_uni) - math.log(p_katz)))
                C.add_arc(from_state, pynini.Arc(id_of[w], id_of[w], weight(d),
                                                 state.get(w, U)))
        C.add_arc(from_state, pynini.Arc(EPS, EPS, weight(clamp(-SCALE * math.log(bow))), U))

    for h, counts in following.items():
        add_history(state[h], counts, total[h])
    if nsent:
        add_history(S, dict(starts), nsent)   # P(w|<s>) = starts / sentences

    for s in range(C.num_states()):           # a sentence may end anywhere
        C.set_final(s, pynini.Weight.one("tropical"))
    C.arcsort("ilabel")
    return C


def referenced(id_of, following, starts):
    """Only the words C can reach. A lookup miss is already "no context" to the decoder,
    so shipping the rest of the lexicon would be dead weight."""
    words = set(following)
    for counts in following.values():
        words.update(w for w, c in counts.items() if c >= MINBI and w in id_of)
    words.update(w for w, c in starts if c >= MINBI and w in id_of)
    return words


def main():
    uni = corpus.read_lexicon()
    # Ids run over the whole lexicon so they stay stable as counts change.
    id_of = {w: i for i, w in enumerate(uni, start=1)}

    following, total = histories(id_of)
    starts, nsent = count_starts(id_of)
    print(f"    vocab {len(id_of)}, histories {len(following)}, "
          f"sentence starts {len(starts)} over {nsent} sentences")

    C = build(uni, id_of, following, total, starts, nsent)
    words = referenced(id_of, following, starts)

    corpus.stage_assets()
    with open(os.path.join(corpus.OUT, "context-words.tsv"), "w", encoding="utf-8") as f:
        f.writelines(f"{id_of[w]}\t{w}\n" for w in sorted(words, key=lambda x: id_of[x]))
    C.write(os.path.join(corpus.OUT, "context.fst"))
    print(f"==> out/context.fst: {C.num_states()} states, {len(words)} words "
          f"(DISCOUNT={DISCOUNT} SCALE={SCALE} KCAP={KCAP} MINBI={MINBI})")


if __name__ == "__main__":
    main()
