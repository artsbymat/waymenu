# Performance measurements

Measurements are made from an optimized build. They are evidence for a specific machine,
not universal guarantees.

## 2026-11-03

- Host: Linux 7.2.11-arch1-1, x86_64
- Compiler: rustc 1.99.1
- Build: `cargo build --release`
- Stripped release executable: 3,350,432 bytes (includes SVG icon rasterization)
- Headless application discovery + query: 100 fresh processes completed in 0.718 s,
  or about 7.18 ms per invocation.

The documented `/usr/bin/time -v` RSS measurement could not be collected on this host
because `/usr/bin/time` is not installed. Interactive RSS was sampled from `/proc` during
a live Hyprland session. With the release launcher showing discovered applications and
visible icons, RSS was **20,120 KiB** and high-water RSS was **21,064 KiB**. A previous
idle session measured about **7,624 KiB** RSS and **8,576 KiB** high-water RSS. Both
scenarios are below the 50 MiB target.

`systemd-run --user --wait -p MemoryAccounting=yes` was also attempted, but the managed
environment denies access to the user systemd bus. The release test suite includes a
5,000-entry fuzzy-filter gate with a 10 ms release budget; the debug build uses a relaxed
100 ms ceiling so instrumentation does not create false failures.

Linux integration tests now use `wait4(2)`/`rusage` directly, avoiding an external timing
utility. Conservative debug-binary measurements on the same host:

- Application discovery plus query: **8,244 KiB peak RSS**.
- Dmenu with 100,000 input rows: **29,544 KiB peak RSS**.

Both are enforced below the 50 MiB target by `tests/memory.rs`. These measurements cover
real child processes. The interactive measurement above includes decoding the icons
visible in the application list; it does not cover arbitrary large plugin-provided images.

Live compositor smoke checks on 2026-11-03 also confirmed Escape closes the surface,
Down plus Enter returns the second dmenu item when `--lines 1` limits the viewport, Tab
completes a partial dmenu query, `--monitor eDP-1` opens on the requested output, a held
Down key repeats through 35 dmenu entries, the scrolloff keeps one result row below the
selection, and a custom Steam game icon resolves from the user hicolor directory even
without an `index.theme` file.

Reproduce the available startup measurement with:

```sh
cargo build --release
TIMEFORMAT='total_seconds=%R'
time for i in $(seq 1 100); do
  target/release/waymenu --query firefox >/dev/null
done
```
