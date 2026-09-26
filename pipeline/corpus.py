"""Readers for the corpora data/fetch.py downloaded, and for the lexicon."""
import functools
import io
import os
import random
import re
import shutil
import sys
import tarfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DATA = os.path.join(ROOT, "data")
ASSETS = os.path.join(ROOT, "assets")
OUT = os.path.join(ROOT, "out")
LEXICON = os.path.join(DATA, "nepali_lm_lexicon.tsv")

DEVANAGARI = re.compile(r"[ऀ-ॿ]+")
DANDAS = ("।", "॥")

NEWS = "nep_news_2020_300K"
LEXICON_CORPUS = "nep_newscrawl_2016_300K"
# The context model breaks count ties by first occurrence, so this order matters.
CORPORA = ["nep-np_web_2015_1M", NEWS, "nep_wikipedia_2021_100K"]
TRAIN_SENTENCES = 240000


def stage_assets():
    """Copy the hand-edited files in, so out/ is a model set the engine can load."""
    os.makedirs(OUT, exist_ok=True)
    for name in os.listdir(ASSETS):
        shutil.copyfile(os.path.join(ASSETS, name), os.path.join(OUT, name))


def leipzig(corpus, part):
    path = os.path.join(DATA, "external", corpus + ".tar.gz")
    if not os.path.exists(path):
        sys.exit(f"{path} is missing; run: python data/fetch.py")
    with tarfile.open(path) as tar:
        member = tar.extractfile(f"{corpus}/{corpus}-{part}.txt")
        if member is None:
            sys.exit(f"{path} holds no {part} file")
        yield from io.TextIOWrapper(member, encoding="utf-8")


def sentences(corpus):
    for line in leipzig(corpus, "sentences"):
        yield line.rstrip("\n").split("\t", 1)[-1]


@functools.lru_cache(maxsize=1)
def news_split():
    """Shuffled once; the tail is held out, so no model is built from it."""
    news = list(sentences(NEWS))
    random.Random(1234).shuffle(news)
    return news[:TRAIN_SENTENCES], news[TRAIN_SENTENCES:]


def lexicon_counts():
    freq = {}
    for line in leipzig(LEXICON_CORPUS, "words"):
        parts = line.rstrip("\n").split("\t")   # id, word, frequency
        if len(parts) >= 3 and DEVANAGARI.fullmatch(parts[1]) and parts[2].isdigit():
            freq[parts[1]] = int(parts[2])
    return freq


def read_lexicon(path=LEXICON):
    if not os.path.exists(path):
        sys.exit(f"{path} is missing; run: python pipeline/build_l.py")
    freq = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) == 2 and parts[1].isdigit():
                freq[parts[0]] = int(parts[1])
    return freq
