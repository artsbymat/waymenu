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

## Convert currencies

Enter a currency expression such as `= 100 USD -> IDR` or `= 50 EUR -> USD`.
Press Enter to copy the result once it appears.

Waymenu uses `curl` to fetch rates on demand and refreshes the display when the
download finishes. It reuses a cache younger than one hour and falls back to the
last saved rates if the download fails. The cache lives at
`$XDG_CACHE_HOME/waymenu/rink/currency.json` or `~/.cache/waymenu/rink/currency.json`.
[Rink exchange-rate feed](https://rinkcalc.app/data/currency.json)

Update both Waymenu and `librink.so` to use automatic refresh.

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/rink/Cargo.toml
install -Dm755 plugins/rink/target/release/librink.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/librink.so"
```
