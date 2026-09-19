# lekhai

English to Nepali transliteration engine.

## 📦 Install

```sh
pip install lekhai
```

Python 3.8+.

## 🚀 Usage

```python
from lekhai import Engine

e = Engine()  # loads the bundled model
e.best("namaste")  # 'नमस्ते'
e.transliterate("mero naam ke ho")  # 'मेरो नाम के हो'
```

## 🙌 Contributing

Contributions are welcome! If you'd like to help, please feel free to fork the
repository, create a feature branch, and open a pull request.

## 📄 License

This project is distributed under the MIT License. See
[LICENSE](https://github.com/hemantapkh/lekhai/blob/main/LICENSE) for more information.

Author/Maintainer: Hemanta Pokharel ([GitHub](https://github.com/hemantapkh))
