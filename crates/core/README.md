# lekhai

Lekhai core — English to Nepali transliteration engine.

You type `namaste`, you get `नमस्ते`.

## 📦 Install

```sh
cargo add lekhai
```

## 🚀 Usage

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
