#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

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

echo "fmd wasm-core check: ok"
