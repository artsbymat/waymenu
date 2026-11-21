# Clipboard

Type `cb ` in Waymenu to browse clipboard history. Add part of the copied text
to filter the list, for example `cb example.org`. Select an entry and press Enter
to put it back on the clipboard. Paste it in the target app with its usual paste
shortcut.

Waymenu shows a color swatch for recognized color values and a thumbnail for
supported images. Run `waymenu --input 'cb '` to open clipboard history directly.

## Set up clipboard history

Install `wl-clipboard` and either `clipvault` or `cliphist`. Waymenu uses
`wl-copy` to restore entries. It looks for `clipvault` first and tries
`cliphist` when it cannot run `clipvault`.

Start the watcher for your chosen clipboard manager once per Wayland session.
See the [clipvault setup guide](https://github.com/rolv-apneseth/clipvault#setup)
or [cliphist usage guide](https://github.com/sentriz/cliphist#usage).

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/clipboard/Cargo.toml
install -Dm755 plugins/clipboard/target/release/libclipboard.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libclipboard.so"
```
