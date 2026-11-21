#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Build for the current host. GNU/Linux archives require compatible glibc.
package_host="$(rustc -vV | sed -n 's/^host: //p')"
case "$package_host" in
  *-unknown-linux-gnu) ;;
  *) echo 'Packaging supports GNU/Linux hosts only.' >&2; exit 1 ;;
esac
if [[ -n "${CARGO_BUILD_TARGET:-}" && "$CARGO_BUILD_TARGET" != "$package_host" ]]; then
  echo 'Cross-compiled releases are not supported by this script.' >&2
  exit 1
fi

package_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"

echo "==> Building waymenu (v${package_version}) in release mode..."
cargo build --release --locked

package_binary="${CARGO_TARGET_DIR:-target}"
if [[ -n "${CARGO_BUILD_TARGET:-}" ]]; then package_binary="$package_binary/$package_host"; fi
package_binary="$package_binary/release/waymenu"

echo "==> Building native plugins in release mode..."
for plugin_cargo in plugins/*/Cargo.toml; do
  plugin_dir="$(dirname "$plugin_cargo")"
  plugin_name="$(basename "$plugin_dir")"
  echo "    -> Building plugin: $plugin_name"
  cargo build --manifest-path "$plugin_cargo" --release --locked
done

package_name="waymenu-${package_version}-${package_host}"
package_stage="$(mktemp -d)"
trap 'rm -rf -- "$package_stage"' EXIT

mkdir -p "dist" "$package_stage/$package_name/plugins"

install -m755 "$package_binary" "$package_stage/$package_name/waymenu"

for plugin_cargo in plugins/*/Cargo.toml; do
  plugin_dir="$(dirname "$plugin_cargo")"
  plugin_name="$(basename "$plugin_dir")"
  library="lib${plugin_name//-/_}.so"

  so_file=""
  if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
    if [[ -n "${CARGO_BUILD_TARGET:-}" && -f "$CARGO_TARGET_DIR/$package_host/release/$library" ]]; then
      so_file="$CARGO_TARGET_DIR/$package_host/release/$library"
    elif [[ -f "$CARGO_TARGET_DIR/release/$library" ]]; then
      so_file="$CARGO_TARGET_DIR/release/$library"
    fi
  fi

  if [[ -z "$so_file" ]]; then
    if [[ -n "${CARGO_BUILD_TARGET:-}" && -f "$plugin_dir/target/$package_host/release/$library" ]]; then
      so_file="$plugin_dir/target/$package_host/release/$library"
    elif [[ -f "$plugin_dir/target/release/$library" ]]; then
      so_file="$plugin_dir/target/release/$library"
    fi
  fi

  if [[ -n "$so_file" && -f "$so_file" ]]; then
    install -m755 "$so_file" "$package_stage/$package_name/plugins/$library"
  else
    echo "Error: plugin binary not found for $plugin_name" >&2
    exit 1
  fi
done

cp LICENSE README.md "$package_stage/$package_name/"
if [[ -f PERFORMANCE.md ]]; then
  cp PERFORMANCE.md "$package_stage/$package_name/"
fi
if [[ -f CHANGELOG.md ]]; then
  cp CHANGELOG.md "$package_stage/$package_name/"
fi

tar -czf "dist/$package_name.tar.gz" -C "$package_stage" "$package_name"
(cd dist && sha256sum "$package_name.tar.gz" > "$package_name.tar.gz.sha256")
printf 'Created dist/%s.tar.gz and checksum\n' "$package_name"
