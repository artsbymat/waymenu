# Hyprland Caffeine

Type `caf ` in Waymenu and choose **30**, **60**, **90**, or **120 minutes** to
keep the screen on for that duration. Choose **Infinity** to leave Caffeine on
until you turn it off. Press Enter to apply your selection.

For a custom duration, type a positive whole number and press Enter on the
result. For example, `caf 45` and `caf 45m` both mean 45 minutes. Use `s` for
seconds, `m` for minutes, `h` for hours, and `d` for days.

While Caffeine is active, `caf ` shows **Continue** and **Off**. Continue closes
Waymenu and leaves the current timer running. Off releases the plugin's idle
inhibitor. A timed session releases it automatically when the duration ends.
Select Off before starting a new duration.

The plugin uses a transient systemd user service and Hypridle's support for
systemd idle inhibitors. Hypridle must be running and `ignore_systemd_inhibit`
must be `false` (the default). [Hypridle configuration](https://github.com/hyprwm/hypridle#configuration)

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/hyprland-caffeine/Cargo.toml
install -Dm755 plugins/hyprland-caffeine/target/release/libhyprland_caffeine.so \
  "${WAYMENU_PLUGIN_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins}/libhyprland_caffeine.so"
```

Run `waymenu --input 'caf '` to open the plugin directly.
