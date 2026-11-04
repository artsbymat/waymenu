# Waymenu Pacman Plugin

A native shared-library plugin for [Waymenu](https://github.com/rahmat/waymenu) that integrates with Arch Linux's `pacman` package manager using the `pac ` prefix.

## Features

- **Fast Package Discovery & Search**: Queries both official sync repositories (`core`, `extra`, `multilib`) and foreign / local packages (`/var/lib/pacman/local/`, AUR) with zero startup lag.
- **Subcommand Modes**:
  - `pac <query>`: Search all packages. Press `Enter` to install, or reinstall/update if already installed. If installed, an uninstall option is also presented.
  - `pac -R <query>` or `pac rm <query>`: Search installed packages only to uninstall (`sudo pacman -Rns <pkg>`).
  - `pac -Q <query>`: View installed package details (`pacman -Qi <pkg>`).
  - `pac -S <query>`: Explicit search for installation.
  - `pac -Syu` or `pac update`: Run full system upgrade.
  - `pac -Sc` or `pac clean`: Clean package cache.
- **Smart Interactive Terminal Execution**: Pacman requires interactive terminal input for root permissions (`sudo`), dependency confirmations, and live progress bars. The plugin automatically detects available Wayland/X11 terminals (`foot`, `kitty`, `alacritty`, `ghostty`, `wezterm`, `gnome-terminal`, etc.) and keeps the window open until a key is pressed so you can view the output and any error messages.
- **AUR Helper Support**: Works with `paru`, `yay`, or custom wrappers via environment variables.

## Building & Installation

Compile the plugin as a release shared object (`cdylib`):

```sh
cargo build --manifest-path plugins/pacman/Cargo.toml --release
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins"
cp plugins/pacman/target/release/libpacman.so "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/"
```

## Usage

In Waymenu, type `pac ` followed by your query:

```text
pac firefox
pac ripgrep
pac -R vlc
pac -Syu
```

## Configuration (Environment Variables)

- `WAYMENU_PACMAN_TERMINAL`: Override the terminal emulator to launch (e.g. `foot`, `kitty`, `alacritty`). Defaults to `$TERMINAL` or the first available in PATH.
- `WAYMENU_PACMAN_WRAPPER`: Use an AUR helper or wrapper instead of `sudo pacman` (e.g. `paru`, `yay`).
- `WAYMENU_PACMAN_CMD`: Custom install command override (e.g. `paru -S`).
- `WAYMENU_PACMAN_NO_HOLD`: Set to `1` to disable the "Press any key to close..." prompt after the command completes.
