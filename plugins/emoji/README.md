# Emoji Picker

Type `e ` in Waymenu to browse emojis, or type a name or keyword such as
`e fox` or `e rocket`. Select an emoji and press Enter to copy it. Paste it
in the target app with its usual paste shortcut.

Copying requires `wl-copy` from the `wl-clipboard` package. Run
`waymenu --input 'e '` to open the picker directly.

## Unicode symbols

Type `e unicode` to browse arrows, math and currency symbols, punctuation,
shapes, keyboard and music symbols, and Greek letters. Add a name or category
to filter the list, such as `e unicode infinity` or `e unicode currency`.

If you know the code point, enter it directly, such as `e U+2192` for `→`.
Code point lookup also works for characters outside the browse list. Press
Enter to copy the selected character.

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/emoji/Cargo.toml
install -Dm755 plugins/emoji/target/release/libemoji.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libemoji.so"
```
