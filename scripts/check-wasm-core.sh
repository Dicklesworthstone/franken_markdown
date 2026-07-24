#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"

target="wasm32-unknown-unknown"

if ! rustup target list --installed | grep -qx "$target"; then
  cat >&2 <<MSG
fmd wasm-core check: missing Rust target '$target'.

Install it with:
  rustup target add $target

Then rerun:
  scripts/check-wasm-core.sh
MSG
  exit 3
fi

echo "fmd wasm-core check: native std-only library"
cargo check --no-default-features --lib

echo "fmd wasm-core check: $target std-only library"
cargo check --target "$target" --no-default-features --lib

# fmd-math has no in-repo consumer yet, so the root check does not reach
# it; hold it to the same WASM-clean bar directly.
echo "fmd wasm-core check: fmd-math std-only library"
cargo check -p fmd-math --lib

echo "fmd wasm-core check: $target fmd-math std-only library"
cargo check -p fmd-math --target "$target" --lib

echo "fmd wasm-core check: ok"
