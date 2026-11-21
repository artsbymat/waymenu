# Chrome History

Type `h ` in Waymenu to see recent pages from your Google Chrome Stable profiles.
Type part of a page title or URL to search the full HTTP and HTTPS history. Waymenu
shows the 20 most recently visited matches and labels each with its profile folder.
Press Enter to open the selected page in that profile.

Build with the system SQLite development package installed, then install the plugin.
Keep either `libchrome_history.so` or `libbraveorigin_history.so` in the plugin directory,
since both register `h `.

```sh
cargo build --release --locked --manifest-path plugins/chrome-history/Cargo.toml
install -Dm755 plugins/chrome-history/target/release/libchrome_history.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libchrome_history.so"
waymenu --input 'h '
```

The plugin checks Chrome's standard Linux data directory. If you launch Chrome with
a custom `--user-data-dir`, set `WAYMENU_CHROME_USER_DATA_DIR` to that absolute path
when running Waymenu.

On activation the plugin copies each profile's `History` file into a private
directory under `/tmp` and queries the copy. While Chrome is writing, a copy can
miss recent visits or fail to open. Visits stored only in Chrome's write-ahead
log appear once Chrome copies them to the main file. Reopen Waymenu to pick up
newer visits. Waymenu removes the copies when it switches plugins or exits normally.
