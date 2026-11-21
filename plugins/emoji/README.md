# Emoji Picker

Type `e ` in Waymenu to browse emojis, or type a name or keyword such as
`e fox` or `e rocket`. Select an emoji and press Enter to copy it. Paste it
in the target app with its usual paste shortcut.

Copying requires `wl-copy` from the `wl-clipboard` package. Run
`waymenu --input 'e '` to open the picker directly.

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/emoji/Cargo.toml
install -Dm755 plugins/emoji/target/release/libemoji.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libemoji.so"
```
