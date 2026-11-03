#!/bin/sh
# Builds the Services tab to WebAssembly and puts the module where the
# manifest's "native" says: tabs/services.wasm. Hangar runs the module the kit
# ships and never builds one, so commit it with the source that made it.
#
#   scripts/build-native.sh          build it
#   scripts/build-native.sh --check  build it, and fail if the committed one is not that
#
# Needs rustup: native/rust-toolchain.toml names the compiler and the
# wasm32-unknown-unknown target, which rustup fetches when they are missing.
# Never wasm32-wasip1.
set -eu

kit="$(cd "$(dirname "$0")/.." && pwd)"
module="$kit/tabs/services.wasm"
cd "$kit/native"

if command -v rustup >/dev/null 2>&1; then
  rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install
fi

# Paths of this machine are not in the module: the same source gives the same
# bytes in any folder, with any CARGO_HOME.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="--remap-path-prefix=$kit=/kit --remap-path-prefix=$cargo_home=/cargo"

cargo build --locked --release --target wasm32-unknown-unknown
built="target/wasm32-unknown-unknown/release/kit_services_tab.wasm"

if [ "${1:-}" = "--check" ]; then
  if cmp -s "$built" "$module"; then
    echo "tabs/services.wasm is what native/ builds"
    exit 0
  fi
  echo "tabs/services.wasm is stale: run scripts/build-native.sh and commit the result" >&2
  exit 1
fi

mkdir -p "$kit/tabs"
cp "$built" "$module"
chmod 644 "$module"
echo "$module"
