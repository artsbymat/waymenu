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

The launcher itself depends only on this small SDK and the dynamic loader. Rink and other
optional functionality belongs in separate `cdylib` crates and is not linked into the
Waymenu executable. Plugins run as trusted native code inside the launcher process.

The Rink example is built separately:

```sh
cargo build --manifest-path plugins/rink/Cargo.toml --release
```
