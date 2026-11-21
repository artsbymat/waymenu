# Waymenu

Waymenu is a low-overhead native Wayland launcher. Running `waymenu` opens a centered
`wlr-layer-shell` surface rendered through shared memory; no GTK, Qt, web view, or
background daemon is used. The launcher provides application discovery, fuzzy ranking,
usage history, PATH binary search, explicit shell execution, and a scriptable dmenu mode.

## Install from GitHub Releases

Download the Linux x86-64 archive from [GitHub Releases](https://github.com/artsbymat/waymenu/releases) and extract it. From the extracted directory, install the executable and plugins:

```sh
install -Dm755 waymenu "$HOME/.local/bin/waymenu"
plugin_dir="${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
mkdir -p "$plugin_dir"
install -m755 plugins/*.so "$plugin_dir/"
```

Add `~/.local/bin` to `PATH` if needed. The release build uses Ubuntu 24.04
and requires `libxkbcommon` at runtime. Plugins can require external tools
such as `wl-copy` and `clipvault`.

Pushing a version tag such as `v0.1.0` triggers the release workflow. GitHub
Actions builds and tests Waymenu and every bundled plugin before publishing
the archive.

## Build from source

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
Press F1 in the launcher to view the active keybindings and installed plugin names,
prefixes, IDs, and descriptions. The help key can be changed in `[keybindings] help`.

Optional native plugins are loaded from `$XDG_CONFIG_HOME/waymenu/plugins` (or
`~/.config/waymenu/plugins`). Dynamic loading is enabled by default and can be disabled
with `[plugins] dynamic = false`. No `.so` is needed for the normal launcher or shell
plugin. Native plugins execute in the launcher process, so only install plugins you trust.

Use the [Web Search plugin](plugins/web-search/README.md) with the `s ` prefix to
search websites or open bookmarks from your own TOML files.

The Rink calculator and unit-conversion plugin is built separately and uses the `=`
prefix. It is not a dependency of the Waymenu executable:

```sh
cargo build --manifest-path plugins/rink/Cargo.toml --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
cp plugins/rink/target/release/librink.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/"
```

The Clipboard plugin integrates with `clipvault` (or `cliphist`) using the `cb ` prefix:

```sh
cargo build --manifest-path plugins/clipboard/Cargo.toml --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
cp plugins/clipboard/target/release/libclipboard.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/"
```

It features:
- **Color previews** in rounded squares for HEX, RGB, HSL, HSV, CMYK, and OKLCH color codes.
- **Link indicator** (`🌍`) for URLs.
- **Image preview & dimensions** for copied images.

You can bind it in Hyprland directly:
```ini
bind = SUPER, V, exec, waymenu --input 'cb '
```

The Kill Process plugin allows searching and terminating running processes using the `k ` prefix:

```sh
cargo build --manifest-path plugins/kill/Cargo.toml --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
cp plugins/kill/target/release/libkill.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/"
```

It features:
- **Fast fuzzy process search** by command name, PID, user, or full command-line arguments.
- **Signal specification**: Supports flags like `k -15 firefox`, `k -term code`, `k -HUP nginx`, defaulting to `SIGKILL` (`-9`).
- **Native POSIX syscall execution**: Direct `kill` syscall for zero overhead.
- **Process filtering**: Distinguishes user processes from kernel threads, prioritizing the current user's processes.

You can also bind it in Hyprland directly:
```ini
bind = SUPER, Escape, exec, waymenu --input 'k '
```

The Pacman plugin allows searching, installing, and managing Arch Linux packages using the `pac ` prefix:

```sh
cargo build --manifest-path plugins/pacman/Cargo.toml --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
cp plugins/pacman/target/release/libpacman.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/"
```

It features:
- **Fast package search**: Queries sync repositories (`core`, `extra`, `multilib`) and local/AUR packages (`/var/lib/pacman/local`).
- **Subcommands**: Supports `pac <query>` (install/reinstall/remove), `pac -R <query>` (remove), `pac -Q <query>` (info), `pac -Syu` (upgrade), and `pac -Sc` (clean cache).
- **Interactive terminal execution**: Automatically detects terminal emulators (`foot`, `kitty`, `alacritty`, `ghostty`, etc.) and keeps the terminal open so user confirmations, sudo passwords, and installation logs remain visible.
- **AUR helper compatibility**: Supports `paru`, `yay`, or custom commands via `WAYMENU_PACMAN_WRAPPER`.

You can also bind it in Hyprland directly:
```ini
bind = SUPER, P, exec, waymenu --input 'pac '
```

Then enter expressions such as `=2+2` or `=1 mile -> km`; Enter copies the result with
`wl-copy`. For development, `WAYMENU_PLUGIN_DIR=/path/to/plugins` overrides the plugin
directory. The plugin SDK and ABI v2 contract are in `plugin-api/`.

```sh
cargo run --release -- --mode binary
printf 'Apple\nBanana\nOrange\n' | cargo run --release -- --dmenu --prompt 'Fruit' --placeholder 'Filter choices…'
cargo run --release -- --monitor HDMI-A-1
```

`--prompt` displays a label to the left of the input. `--placeholder` displays
text inside the input while it is empty and defaults to `Select…`.

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

## Configuration

Configuration is located at `$XDG_CONFIG_HOME/waymenu/config.toml` (or `~/.config/waymenu/config.toml`). Below is a complete annotated example configuration:

```toml
[general]
theme = "dark"            # "dark", "light", or "custom"
width = 640               # Window width in pixels (240 - 4096)
height = 574              # Optional fixed height in pixels (omitted for auto-fit)
max_results = 8           # Maximum visible results (1 - 100)
fuzzy_search = true       # Enable fuzzy search engine
remember_history = true   # Rank frequently and recently launched applications
# monitor = "DP-1"        # Optional target monitor (name, description, or index)
# terminal = "foot"       # Terminal emulator used for terminal desktop apps

[appearance]
font_family = "sans-serif" # System font name
font_size = 14.0           # Base font size
border_radius = 12.0       # Rounded corner radius (0 - 64)
show_icons = true          # Display application and plugin icons
show_description = true    # Display secondary description lines

[colors]
# Optional color overrides (Hex: #RGB, #RGBA, #RRGGBB, #RRGGBBAA, or rgb/rgba)
background = "#191b22"     # Window background (supports alpha transparency)
input      = "#2b2e39"     # Search bar background
selected   = "#3f4557"     # Selected result row (alias: selection)
primary    = "#f5f5f8"     # Input text, titles, caret (alias: text)
secondary  = "#a6abb9"     # Descriptions, placeholders, footer (alias: subtext)
highlight  = "#69aaff"     # Fuzzy search matching characters (alias: match, accent)
# border   = "#5e81ac"     # Optional window border stroke

[plugins]
shell_prefix = "!"         # Prefix to trigger shell command execution
# shell = "/bin/bash"      # Shell path (defaults to $SHELL or /bin/sh)
dynamic = true             # Enable loading .so plugins from plugins/ directory

[binary]
prefix = "$ "              # Prefix to trigger $PATH binary executable search
show_path = true           # Show executable directory path in results

[history]
enabled = true             # Persist launch frequency and recency
max_entries = 500          # Maximum entries stored in history.json

[keybindings]
execute             = "Return"
close               = "Escape"
autocomplete        = "Tab"
next                = "Ctrl+n"
previous            = "Ctrl+p"
next_arrow          = "Down"
previous_arrow      = "Up"
left_arrow          = "Left"
right_arrow         = "Right"
clear               = "Ctrl+u"
backspace           = "BackSpace"
delete_word         = "Ctrl+w"
delete_word_back    = "Ctrl+BackSpace"
delete_forward      = "Delete"
delete_word_forward = "Ctrl+Delete"
word_left           = "Ctrl+Left"
word_right          = "Ctrl+Right"
beginning           = "Ctrl+a"
end                 = "Ctrl+e"
help                = "F1"
```

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
