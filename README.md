# lekhai

English to Nepali transliteration engine.

You type `namaste`, you get `नमस्ते`.

A roman token is decoded through three composed FSTs — a learned chunk transducer, a word
lexicon, and a word-bigram context model — so the previous word settles spellings a
per-word transliterator has to guess at. Rust core, Python bindings, one bundled model set.

## 📦 Install

```sh
pip install lekhai      # Python 3.8+
cargo add lekhai        # Rust 1.74+
```

## 🚀 Usage

### Python

```python
from lekhai import Engine

e = Engine()                           # the bundled model, loaded once
e.best("namaste")                      # 'नमस्ते'
e.transliterate("mero naam ke ho")     # 'मेरो नाम के हो'
```

### Rust

```rust
use lekhai::Engine;

let engine = Engine::load(lekhai::BUNDLED_ASSETS)?;   // build once, keep it

engine.best("namaste", None)?;                        // "नमस्ते"
engine.transliterate("mero naam ke ho")?;             // "मेरो नाम के हो"
```

## 🙌 Contributing

Contributions are welcome! If you'd like to help, please feel free to fork the
repository, create a feature branch, and open a pull request.

## 📄 License

This project is distributed under the MIT License. See
[LICENSE](https://github.com/hemantapkh/lekhai/blob/main/LICENSE) for more information.

Author/Maintainer: Hemanta Pokharel ([GitHub](https://github.com/hemantapkh))
