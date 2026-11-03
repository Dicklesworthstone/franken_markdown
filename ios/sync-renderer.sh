#!/usr/bin/env bash
# Synchronizes and verifies the bundled WASM renderer inside ios/Renderer/
# Usage:
#   ios/sync-renderer.sh          # build, sync, and update manifest
#   ios/sync-renderer.sh --check  # verify the entire package without modifying
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

# Preserve the verified package's complete module/resource closure, including
# root .mjs helpers and nested demo/pkg modules. Do not follow package links or
# let a package replace the native app's private bridge or generated manifest.
# Globbing includes hidden files and works with the system Bash on macOS.
shopt -s dotglob nullglob
package_files=()
package_dirs=(".")
collect_package_tree() {
  local directory="$1"
  local entry relative reserved
  if [[ ! -r "$directory" || ! -x "$directory" ]]; then
    echo "ERROR: renderer package directory is not readable: $directory" >&2
    exit 1
  fi
  for entry in "$directory"/*; do
    relative="${entry#"$pkg_dir/"}"
    reserved="$(printf '%s' "$relative" | LC_ALL=C tr '[:upper:]' '[:lower:]')"
    case "$reserved" in
      bridge.html|renderermanifest.json)
        echo "ERROR: renderer package collides with reserved app-owned file: $relative" >&2
        exit 1
        ;;
    esac
    if [[ -L "$entry" ]]; then
      echo "ERROR: renderer package contains a symbolic link: $relative" >&2
      exit 1
    elif [[ -d "$entry" ]]; then
      package_dirs+=("$relative")
      collect_package_tree "$entry"
    elif [[ -f "$entry" && -r "$entry" ]]; then
      package_files+=("$relative")
    else
      echo "ERROR: unsupported or unreadable renderer package entry: $relative" >&2
      exit 1
    fi
  done
}
collect_package_tree "$pkg_dir"

pkg_wrapper_sha="$(compute_sha256 "$wrapper_src")"
pkg_wasm_sha="$(compute_sha256 "$wasm_src")"

# Preflight every destination before copying anything. In particular, a stale
# symlink must not redirect a package write into an app-owned or external file.
if [[ -L "$dest_dir" || ( -e "$dest_dir" && ! -d "$dest_dir" ) ]]; then
  echo "ERROR: renderer destination is not a regular directory: $dest_dir" >&2
  exit 1
fi
for relative in "${package_dirs[@]}"; do
  destination="$dest_dir/$relative"
  if [[ -L "$destination" || ( -e "$destination" && ! -d "$destination" ) ]]; then
    echo "ERROR: renderer directory destination is a symbolic link or conflicting file: $destination" >&2
    exit 1
  fi
done
for relative in "${package_files[@]}" "RendererManifest.json"; do
  destination="$dest_dir/$relative"
  if [[ -L "$destination" || ( -e "$destination" && ! -f "$destination" ) ]]; then
    echo "ERROR: renderer file destination is a symbolic link or conflicting entry: $destination" >&2
    exit 1
  fi
done

verify_package_copy() {
  local relative expected actual
  for relative in "${package_files[@]}"; do
    if [[ ! -f "$dest_dir/$relative" ]]; then
      echo "ERROR: bundled renderer package file is missing: $dest_dir/$relative" >&2
      exit 1
    fi
    expected="$(compute_sha256 "$pkg_dir/$relative")"
    actual="$(compute_sha256 "$dest_dir/$relative")"
    if [[ "$actual" != "$expected" ]]; then
      echo "ERROR: $dest_dir/$relative SHA ($actual) != package ($expected)" >&2
      exit 1
    fi
  done
}

if [[ "$check_only" -eq 1 ]]; then
  if [[ ! -f "$manifest" ]]; then
    echo "ERROR: $manifest does not exist" >&2
    exit 1
  fi
  verify_package_copy
  echo "ios/sync-renderer.sh --check: OK — all ${#package_files[@]} packaged files match (wasm sha: $pkg_wasm_sha)"
  exit 0
fi

# Overlay the complete verified package. Native-only files and resources no
# longer present in the package are retained; synchronization never deletes.
for relative in "${package_dirs[@]}"; do
  mkdir -p "$dest_dir/$relative"
done
for relative in "${package_files[@]}"; do
  cp -f "$pkg_dir/$relative" "$dest_dir/$relative"
done
verify_package_copy

cat << JSON > "$manifest"
{
  "schema": "frankenmarkdown-ios-renderer-v1",
  "wrapperSha256": "$pkg_wrapper_sha",
  "wasmSha256": "$pkg_wasm_sha"
}
JSON

echo "ios/sync-renderer.sh: OK — synced ${#package_files[@]} packaged files (wasm sha: $pkg_wasm_sha)"
