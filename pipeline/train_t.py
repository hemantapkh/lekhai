"""Learn the chunk mappings compile_t.py turns into T, by EM alignment of the pairs.

Each iteration runs forward-backward over every monotonic alignment of a pair (roman
chunks up to MX chars against Devanagari chunks up to MY), then renormalises the expected
counts into P(deva|roman).

Writes out/learned_chunks.tsv; promote it to trained/ by hand. The defaults are the shipped
model's settings, and a full run takes hours in pure Python.
"""
import collections
import os
import random

import corpus

DATA = corpus.DATA
OUT = corpus.OUT

MX = int(os.environ.get("MX", "2"))
MY = int(os.environ.get("MY", "2"))
N_PAIRS = int(os.environ.get("N_PAIRS", "700000"))
ITERS = int(os.environ.get("ITERS", "5"))
MIN_COUNT = float(os.environ.get("MIN_COUNT", "0.5"))
# Casual spellings are a sliver of the data next to Aksharantar's formal romanisation;
# repeating them is what makes EM learn them.
COLLOQ_DUP = int(os.environ.get("COLLOQ_DUP", "15"))
USE_AUG = os.environ.get("USE_AUG", "1") == "1"
MAXLEN = 24

ASPIRATES = [("chh", "ch"), ("kh", "k"), ("gh", "g"), ("jh", "j"),
             ("th", "t"), ("dh", "d"), ("ph", "p"), ("bh", "b")]
VOWELS = [("aa", "a"), ("ii", "i"), ("ee", "i"), ("oo", "u"), ("uu", "u")]


def replace_all(word, pairs):
    for a, b in pairs:
        word = word.replace(a, b)
    return word


def informal(word):
    v = {replace_all(word, ASPIRATES), replace_all(word, VOWELS),
         replace_all(replace_all(word, ASPIRATES), VOWELS)}
    if "v" in word:
        v.add(word.replace("v", "w"))
    if "w" in word:
        v.add(word.replace("w", "v"))
    v.discard(word)
    # Sorted: sampling a set would depend on the run's string hash seed.
    return sorted(x for x in v if x and x.isascii())


def augmented(lines, sample=1_000_000, max_variants=3):
    """Every pair again under its informal spellings, so EM sees both ways of typing it."""
    lines = list(lines)
    random.Random(1).shuffle(lines)
    rng = random.Random(0)
    seen = set()
    for line in lines[:sample]:
        roman, deva = line.split("\t")[:2]
        roman = roman.lower()
        variants = informal(roman)
        for r in [roman] + rng.sample(variants, min(max_variants, len(variants))):
            if (r, deva) not in seen:
                seen.add((r, deva))
                yield f"{r}\t{deva}"


def clean(lines):
    for line in lines:
        p = line.rstrip("\n").split("\t")
        if len(p) < 2:
            continue
        r, d = p[0].strip().lower(), p[1].strip()
        if r and d and r.isascii() and len(r) <= MAXLEN and len(d) <= MAXLEN:
            yield r, d


def load_pairs():
    with open(os.path.join(DATA, "aksharantar", "train.tsv"), encoding="utf-8") as f:
        train = f.read().splitlines()
    pairs = list(clean(train))
    if USE_AUG:
        pairs += list(clean(augmented(train)))
    random.Random(0).shuffle(pairs)
    pairs = pairs[:N_PAIRS]
    with open(os.path.join(DATA, "colloquial_pairs.tsv"), encoding="utf-8") as f:
        colloquial = list(clean(f))
    pairs.extend(colloquial * COLLOQ_DUP)
    print(f"    {len(pairs)} pairs, of which {len(colloquial)} colloquial x{COLLOQ_DUP}")
    return pairs


def align_counts(r, d, prob, acc):
    """Expected counts for one pair, over states (i, j) of consumed prefixes. Returns the
    pair's total probability, 0 when no alignment exists."""
    R, D = len(r), len(d)

    def steps(i, j):
        for xl in range(1, MX + 1):
            if i + xl > R:
                break
            for yl in range(MY + 1):
                if j + yl > D:
                    break
                p = prob.get((r[i:i + xl], d[j:j + yl]), 0.0)
                if p > 0.0:
                    yield xl, yl, p

    alpha = [[0.0] * (D + 1) for _ in range(R + 1)]
    alpha[0][0] = 1.0
    for i in range(R + 1):
        for j in range(D + 1):
            if alpha[i][j]:
                for xl, yl, p in steps(i, j):
                    alpha[i + xl][j + yl] += alpha[i][j] * p
    Z = alpha[R][D]
    if Z <= 0.0:
        return 0.0

    beta = [[0.0] * (D + 1) for _ in range(R + 1)]
    beta[R][D] = 1.0
    for i in range(R, -1, -1):
        for j in range(D, -1, -1):
            if (i, j) != (R, D):
                beta[i][j] = sum(p * beta[i + xl][j + yl] for xl, yl, p in steps(i, j))

    for i in range(R + 1):
        for j in range(D + 1):
            if alpha[i][j]:
                for xl, yl, p in steps(i, j):
                    acc[(r[i:i + xl], d[j:j + yl])] += alpha[i][j] * p * beta[i + xl][j + yl] / Z
    return Z


def normalize(counts):
    by_x = collections.defaultdict(float)
    for (xc, _), c in counts.items():
        by_x[xc] += c
    return {k: c / by_x[k[0]] for k, c in counts.items() if by_x[k[0]] > 0}


def init_prob(pairs):
    """Uniform over every chunk pair that co-occurs anywhere."""
    prob = {}
    for r, d in pairs:
        xs = {r[i:i + xl] for i in range(len(r)) for xl in range(1, MX + 1) if i + xl <= len(r)}
        ys = {d[j:j + yl] for j in range(len(d)) for yl in range(MY + 1) if j + yl <= len(d)}
        for xc in xs:
            for yc in ys:
                prob[(xc, yc)] = 1.0
    return normalize(prob)


def main():
    pairs = load_pairs()
    prob = init_prob(pairs)
    print(f"    {len(prob)} initial chunk pairs")
    for it in range(ITERS):
        acc = collections.defaultdict(float)
        reach = sum(align_counts(r, d, prob, acc) > 0.0 for r, d in pairs)
        prob = normalize({k: c for k, c in acc.items() if c >= MIN_COUNT})
        print(f"    iter {it + 1}/{ITERS}: {reach}/{len(pairs)} alignable, {len(prob)} chunk pairs")
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "learned_chunks.tsv"), "w", encoding="utf-8") as f:
        f.writelines(f"{xc}\t{yc}\t{c:.1f}\t{prob.get((xc, yc), 0):.4f}\n"
                     for (xc, yc), c in sorted(acc.items(), key=lambda kv: -kv[1]))
    print(f"==> {OUT}/learned_chunks.tsv")


if __name__ == "__main__":
    main()
