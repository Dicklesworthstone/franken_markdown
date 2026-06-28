#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"

target="wasm32-unknown-unknown"
package_dir="$CARGO_TARGET_DIR/wasm-package"
pkg_dir="$package_dir/pkg"

if ! rustup target list --installed | grep -qx "$target"; then
  cat >&2 <<MSG
fmd wasm-package check: missing Rust target '$target'.

Install it with:
  rustup target add $target

Then rerun:
  scripts/check-wasm-package.sh
MSG
  exit 3
fi

echo "fmd wasm-package check: no-default core"
cargo check --no-default-features --lib

echo "fmd wasm-package check: wasm-bindgen adapter"
cargo build --target "$target" --no-default-features --features wasm-bindgen --lib

wasm_in="$CARGO_TARGET_DIR/$target/debug/franken_markdown.wasm"
if [ ! -s "$wasm_in" ]; then
  printf 'fmd wasm-package check: expected wasm artifact missing: %s\n' "$wasm_in" >&2
  exit 1
fi

if ! command -v wasm-bindgen >/dev/null 2>&1; then
  cat >&2 <<MSG
fmd wasm-package check: missing 'wasm-bindgen' CLI.

Install the CLI matching Cargo.toml:
  cargo install wasm-bindgen-cli --version 0.2.126 --locked

Then rerun:
  scripts/check-wasm-package.sh
MSG
  exit 3
fi

mkdir -p "$pkg_dir"
wasm-bindgen "$wasm_in" --target web --out-dir "$pkg_dir"
cp wasm/franken_markdown.js "$package_dir/franken_markdown.js"
cp wasm/franken_markdown.d.ts "$package_dir/franken_markdown.d.ts"
cp wasm/package.json "$package_dir/package.json"
mkdir -p "$package_dir/demo"
cp wasm/demo/index.html "$package_dir/demo/index.html"
cp wasm/demo/demo.js "$package_dir/demo/demo.js"

for artifact in \
  "$package_dir/franken_markdown.js" \
  "$package_dir/franken_markdown.d.ts" \
  "$package_dir/package.json" \
  "$package_dir/demo/index.html" \
  "$package_dir/demo/demo.js" \
  "$pkg_dir/franken_markdown.js" \
  "$pkg_dir/franken_markdown_bg.wasm"; do
  if [ ! -s "$artifact" ]; then
    printf 'fmd wasm-package check: expected package artifact missing: %s\n' "$artifact" >&2
    exit 1
  fi
done

echo "fmd wasm-package check: package artifact ready at $package_dir"
