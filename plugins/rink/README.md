# Rink Calculator

Type `= ` (an equals sign followed by a space) and enter an expression. Waymenu
shows the result as you type. Press Enter to copy it with `wl-copy` from the
`wl-clipboard` package.

| Type in Waymenu              | Result               |
| ---------------------------- | -------------------- |
| `= (2 + 3) * 4`              | `20`                 |
| `= 1 mile -> km`             | `1.609344 kilometer` |
| `= 2 hours -> minutes`       | `120 minute`         |
| `= 20 celsius -> fahrenheit` | `68 °F`              |

Run `waymenu --input '= '` to open Waymenu with the calculator prefix filled in.

## Calculation history

Leave the input at `= ` to see up to 50 recent calculations. Use the arrow keys
to select an entry. Enter copies the saved answer, and Tab restores the
expression so you can edit or recalculate it.

Press **Shift+Delete** to delete the selected history entry. Waymenu stays open
and selects the next entry, or the previous one if no entry follows it.
To change the shortcut, edit `delete_result` under `[keybindings]` in
`$XDG_CONFIG_HOME/waymenu/config.toml` or `~/.config/waymenu/config.toml`.

Rink saves a calculation when you press Enter on its live result. Repeating an
expression moves it to the top and updates its answer. An expression and its
answer must fit within 4 KiB combined to be saved. The history lives at
`$XDG_STATE_HOME/waymenu/rink/history.json` or
`~/.local/state/waymenu/rink/history.json`.

## Convert currencies

Enter a currency expression such as `= 100 USD -> IDR` or `= 50 EUR -> USD`.
Press Enter to copy the result once it appears.

Waymenu uses `curl` to fetch rates on demand and refreshes the display when the
download finishes. It reuses a cache younger than one hour and falls back to the
last saved rates if the download fails. The cache lives at
`$XDG_CACHE_HOME/waymenu/rink/currency.json` or `~/.cache/waymenu/rink/currency.json`.
[Rink exchange-rate feed](https://rinkcalc.app/data/currency.json)

## Install from source

Build Waymenu and the plugin from the repository root. History deletion and
automatic exchange rate refresh require both updated files.

```sh
cargo build --release --locked
cargo build --release --locked --manifest-path plugins/rink/Cargo.toml
install -Dm755 plugins/rink/target/release/librink.so \
  "${WAYMENU_PLUGIN_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins}/librink.so"
```

Run `./target/release/waymenu --input '= '` to try the new build. If you
already have Waymenu installed, use `command -v waymenu` to find the executable
your shell uses and replace it with `target/release/waymenu`. Reopen the
launcher after updating both files.
