"""Download the corpora into data/external/, and unpack Aksharantar into TSV.

Archives are pinned by sha256, so a changed upstream file fails the run instead of
quietly changing a model. Sources and their licences are in NOTICE.
"""
import hashlib
import io
import json
import os
import shutil
import sys
import urllib.request
import zipfile

DATA = os.path.dirname(os.path.abspath(__file__))
EXTERNAL = os.path.join(DATA, "external")
LEIPZIG = "https://downloads.wortschatz-leipzig.de/corpora/"

SOURCES = {
    "nep.zip": (
        "https://huggingface.co/datasets/ai4bharat/Aksharantar/resolve/main/nep.zip",
        "8bdcc5957d701ea36b63c00c713b43bc30902774b70736737d480e77929de92c",
    ),
    "nep-np_web_2015_1M.tar.gz": (
        LEIPZIG + "nep-np_web_2015_1M.tar.gz",
        "bfe6d12893fcded01909059ee4703aec9617e6b4839a21c5e47dfb6dc0c26e83",
    ),
    "nep_news_2020_300K.tar.gz": (
        LEIPZIG + "nep_news_2020_300K.tar.gz",
        "0ab72f0c91bc10b65ad30b95d0dd92766f306747c52d9f93c556cd78634cad78",
    ),
    "nep_wikipedia_2021_100K.tar.gz": (
        LEIPZIG + "nep_wikipedia_2021_100K.tar.gz",
        "e551dae86688e8dfcaa4c83442194744a287f4c1af8aaf4d41f7b99625b186aa",
    ),
    "nep_newscrawl_2016_300K.tar.gz": (
        LEIPZIG + "nep_newscrawl_2016_300K.tar.gz",
        "77e64d1fb5ef292c5fbccb3870131b54838da9ee4c95e0108f75aa7894943047",
    ),
}


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def fetch(name):
    url, want = SOURCES[name]
    path = os.path.join(EXTERNAL, name)
    if os.path.exists(path) and sha256(path) == want:
        return path
    os.makedirs(EXTERNAL, exist_ok=True)
    print(f"downloading {name}")
    part = path + ".part"
    with urllib.request.urlopen(url) as r, open(part, "wb") as f:
        shutil.copyfileobj(r, f, 1 << 20)
    got = sha256(part)
    if got != want:
        os.remove(part)
        sys.exit(f"{name}: sha256 {got}, expected {want}")
    os.replace(part, path)
    return path


def aksharantar():
    """test.tsv keeps the source tag: AK-Freq is a native word, AK-NE* a name."""
    out = os.path.join(DATA, "aksharantar")
    os.makedirs(out, exist_ok=True)
    with zipfile.ZipFile(fetch("nep.zip")) as z:
        for split in ("train", "test"):
            with z.open(f"nep_{split}.json") as src, \
                 open(os.path.join(out, f"{split}.tsv"), "w", encoding="utf-8") as dst:
                for line in io.TextIOWrapper(src, encoding="utf-8"):
                    row = json.loads(line)
                    tag = "\t" + row["source"] if split == "test" else ""
                    dst.write(f"{row['english word']}\t{row['native word']}{tag}\n")


def main():
    for name in SOURCES:
        fetch(name)
    aksharantar()


if __name__ == "__main__":
    main()
