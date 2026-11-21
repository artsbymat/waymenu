# Kill Process

Type `k ` to list running processes. Search by process name, PID, command line,
or username. For example, `k firefox` filters the list and shows the PID and
owner for each match. Run `waymenu --input 'k '` to open the list directly.

Enter sends the signal shown on the selected row immediately. The default is
`SIGKILL`. To request a graceful exit, type `k -TERM firefox` or
`k -15 firefox` and select the intended PID before pressing Enter.

## Install from source

Build and install the plugin from the repository root.

```sh
cargo build --release --locked --manifest-path plugins/kill/Cargo.toml
install -Dm755 plugins/kill/target/release/libkill.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libkill.so"
```
