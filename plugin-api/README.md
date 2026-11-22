# Waymenu Plugin SDK

Dynamic plugins are Linux shared libraries (`.so`) that export
`waymenu_plugin_v2() -> PluginApiV2`. Copy a built library into
`$XDG_CONFIG_HOME/waymenu/plugins/` (default `~/.config/waymenu/plugins/`). Waymenu
loads every `.so` there at startup. `WAYMENU_PLUGIN_DIR` can override that directory.

The API uses `#[repr(C)]` function pointers and JSON C strings rather than sharing Rust
trait objects or allocator-owned `String` values across libraries. The host keeps each
library loaded as long as its plugin state exists. Plugins return `PluginResult` arrays
from `search` and one `PluginAction` from `action`. Exported callbacks must not unwind
across the C ABI. `ABI_VERSION` changes when the function table contract changes.

To supply custom text for Tab, set result `metadata` to `autocomplete:<query>`.
Waymenu inserts that query after the plugin prefix. Other results use their
title for completion.

Set result `metadata` to `plugin-pending` to show a temporary status row while a
search is in progress. Waymenu repeats the active search until the plugin stops
returning pending rows. Return the final results or an error row when ready.

To support the Delete selected result shortcut (Shift+Delete by default), export
`waymenu_plugin_delete_result_v2` with the `DeleteResultFn` signature. Waymenu
passes the plugin state and the selected result's NUL-terminated ID once per
keypress. Return `true` after deleting the item so Waymenu refreshes the list,
or `false` to keep the list as it is. This export is optional for ABI v2 plugins.

The launcher itself depends only on this small SDK and the dynamic loader. Rink and other
optional functionality belongs in separate `cdylib` crates and is not linked into the
Waymenu executable. Plugins run as trusted native code inside the launcher process.

The Rink example is built separately:

```sh
cargo build --manifest-path plugins/rink/Cargo.toml --release
```
