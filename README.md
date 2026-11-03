# Waymenu

Waymenu is a low-overhead native Wayland launcher. Running `waymenu` opens a centered
`wlr-layer-shell` surface rendered through shared memory; no GTK, Qt, web view, or
background daemon is used. The launcher provides application discovery, fuzzy ranking,
usage history, PATH binary search, explicit shell execution, and a scriptable dmenu mode.

```sh
cargo run -- --query fire
cargo run -- --query '$cargo'
printf 'Apple\nBanana\nOrange\n' | cargo run -- --dmenu --query ban
```

Run without arguments inside a supported Wayland compositor to open the interactive UI:

```sh
cargo run --release
```

Use `$` for executables and `!` for the shell plugin. Both prefixes are configurable.
Executable discovery is performed off the UI thread and cached only for the current
process. Shell commands run only after Enter; their stdout/stderr is shown inside the
launcher and Esc cancels a running child.

```sh
cargo run --release -- --mode binary
printf 'Apple\nBanana\nOrange\n' | cargo run --release -- --dmenu --prompt 'Fruit'
cargo run --release -- --monitor HDMI-A-1
```

Shell input is executed only when passed explicitly as a submitted query:

```sh
cargo run -- --query '!pwd'
```

Headless application results are emitted as tab-separated ID, title, and description.
A result can also be launched explicitly with `WAYMENU_EXECUTE_ID`.

```sh
WAYMENU_EXECUTE_ID=firefox.desktop cargo run -- --query firefox
```

Configuration is created at `$XDG_CONFIG_HOME/waymenu/config.toml` (falling back to
`~/.config/waymenu/config.toml`). History is stored atomically as JSON under
`$XDG_STATE_HOME/waymenu/history.json`. Missing, inaccessible, or corrupt history never
prevents the launcher from starting.

Waymenu requires `wlr-layer-shell`; compositors without it receive an explicit error.
It does not silently fall back to X11 or a heavyweight toolkit.

See [PERFORMANCE.md](PERFORMANCE.md) for reproducible measurements and outstanding
environment-specific RSS verification.

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
