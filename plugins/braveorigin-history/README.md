# Brave Origin History

Type `h ` to search page titles and URLs across Brave Origin profiles. Waymenu
shows the 20 most recent HTTP or HTTPS matches. Enter opens the result in its
original Brave Origin profile.

Build with the system SQLite development package installed. If Chrome History is
installed, move its `libchrome_history.so` out of the plugin directory first.
Both plugins register the same prefix.
When upgrading from `libbrave_history.so`, move that file out of the plugin
directory before installing `libbraveorigin_history.so`.

```sh
cargo build --release --locked --manifest-path plugins/braveorigin-history/Cargo.toml
install -Dm755 plugins/braveorigin-history/target/release/libbraveorigin_history.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libbraveorigin_history.so"
waymenu --input 'h '
```

The default data directory is `$XDG_CONFIG_HOME/BraveSoftware/Brave-Origin` or
`~/.config/BraveSoftware/Brave-Origin`. For a custom `--user-data-dir`, set
`WAYMENU_BRAVE_ORIGIN_USER_DATA_DIR` to its absolute path.

Waymenu copies each profile's `History` file into a private directory under
`/tmp` and reads the copy. Reopen Waymenu to refresh it. If Brave is writing or
keeping recent visits in its write-ahead log, the copy can miss those visits.
