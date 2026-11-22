# Google Translate

Type `tr ` followed by text, then press Enter to open Google Translate in your
browser. Google Translate detects the source language and defaults to
Indonesian as the target. Add `@` and a language code to change the target.

Type `tr @` to browse suggested languages, or keep typing a name such as
`tr @jepang` or `tr @English`. Select a language and press **Tab** to fill its
code. Then type or paste the text and press **Enter** to open the translation.
You can also type a language code directly.

| Input | Target language |
| --- | --- |
| `tr good morning` | Indonesian (`id`) |
| `tr @en selamat pagi` | English (`en`) |
| `tr @ja how are you?` | Japanese (`ja`) |
| `tr @zh-CN good morning` | Simplified Chinese (`zh-CN`) |

Waymenu opens the browser only after you select the result. The translation
appears on the Google Translate page. No API key is required.

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/google-translate/Cargo.toml
install -Dm755 plugins/google-translate/target/release/libgoogle_translate.so \
  "${WAYMENU_PLUGIN_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins}/libgoogle_translate.so"
```

Run `waymenu --input 'tr '` to open the plugin directly.
