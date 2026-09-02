#!/usr/bin/env bash
# Synchronizes and verifies the bundled WASM renderer inside ios/Renderer/
# Usage:
#   ios/sync-renderer.sh          # build, sync, and update manifest
#   ios/sync-renderer.sh --check  # verify existing manifest matches package without modifying
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

check_only=0
if [[ "${1:-}" == "--check" ]]; then
  check_only=1
fi

pkg_dir="target/fmd-checks/wasm-package"
dest_dir="ios/Renderer"
manifest="$dest_dir/RendererManifest.json"

build_verified_package() {
  bash scripts/check-wasm-package.sh local
}

# A pre-existing target/ package is not proof that it was built from the current
# source fence. The mutating sync command therefore rebuilds and verifies the
# package every time before copying it. Check mode stays read-only and refuses
# to compare against a missing package instead of manufacturing one.
if [[ "$check_only" -eq 0 ]]; then
  build_verified_package
elif [[ ! -d "$pkg_dir" || ! -f "$pkg_dir/pkg/franken_markdown_bg.wasm" ]]; then
  echo "ERROR: verified renderer package is missing; run ios/sync-renderer.sh first" >&2
  exit 1
fi

wrapper_src="$pkg_dir/franken_markdown.js"
wasm_src="$pkg_dir/pkg/franken_markdown_bg.wasm"

compute_sha256() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

pkg_wrapper_sha="$(compute_sha256 "$wrapper_src")"
pkg_wasm_sha="$(compute_sha256 "$wasm_src")"

if [[ "$check_only" -eq 1 ]]; then
  if [[ ! -f "$manifest" ]]; then
    echo "ERROR: $manifest does not exist" >&2
    exit 1
  fi
  cur_wrapper_sha="$(compute_sha256 "$dest_dir/franken_markdown.js")"
  cur_wasm_sha="$(compute_sha256 "$dest_dir/pkg/franken_markdown_bg.wasm")"
  
  if [[ "$cur_wrapper_sha" != "$pkg_wrapper_sha" ]]; then
    echo "ERROR: ios/Renderer/franken_markdown.js SHA ($cur_wrapper_sha) != package ($pkg_wrapper_sha)" >&2
    exit 1
  fi
  if [[ "$cur_wasm_sha" != "$pkg_wasm_sha" ]]; then
    echo "ERROR: ios/Renderer/pkg/franken_markdown_bg.wasm SHA ($cur_wasm_sha) != package ($pkg_wasm_sha)" >&2
    exit 1
  fi
  echo "ios/sync-renderer.sh --check: OK — renderer bundle matches package (wasm sha: $cur_wasm_sha)"
  exit 0
fi

# Copy verified package files into ios/Renderer
mkdir -p "$dest_dir/pkg" "$dest_dir/demo"
cp -f "$pkg_dir/franken_markdown.js" "$dest_dir/"
cp -f "$pkg_dir/franken_markdown.d.ts" "$dest_dir/"
cp -f "$pkg_dir/package.json" "$dest_dir/"
cp -f "$pkg_dir/README.md" "$dest_dir/"
cp -f "$pkg_dir/demo/"* "$dest_dir/demo/"
cp -f "$pkg_dir/pkg/"* "$dest_dir/pkg/"

cat << JSON > "$manifest"
{
  "schema": "frankenmarkdown-ios-renderer-v1",
  "wrapperSha256": "$pkg_wrapper_sha",
  "wasmSha256": "$pkg_wasm_sha"
}
JSON

echo "ios/sync-renderer.sh: OK — synced renderer bundle (wasm sha: $pkg_wasm_sha)"
