#!/usr/bin/env bash
# DSR-invoked profile build/verification. No network publishing, cleanup or Actions.
# Produces separate PRIVATE local packages; does not change the released npm package.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export RCH_CARGO_WRAPPER_BYPASS=1
export CARGO_HTTP_USER_AGENT='OpenAI File Downloader, XaiImageApiFetch/1.0'
for tool in cargo rustc rustup wasm-bindgen node git; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'WASM profiles: missing required tool %s\n' "$tool" >&2
    exit 3
  fi
done
if ! rustup target list --installed | grep -qx 'wasm32-unknown-unknown'; then
  printf 'WASM profiles: install the wasm32-unknown-unknown target on this DSR host\n' >&2
  exit 3
fi
SOURCE_COMMIT="$(git rev-parse HEAD)"
source_fence() {
  test "$(git rev-parse HEAD)" = "$SOURCE_COMMIT" &&
    git diff --quiet && git diff --cached --quiet &&
    test -z "$(git ls-files --others --exclude-standard)"
}
if ! source_fence; then
  printf 'WASM profiles: a clean committed source tree is required\n' >&2
  exit 1
fi
mkdir -p "$ROOT/tests/artifacts/wasm"
ART="$(mktemp -d "$ROOT/tests/artifacts/wasm/profiles.XXXXXXXX")"
printf '%s\n' "$SOURCE_COMMIT" > "$ART/source-commit.txt"
printf 'WASM profiles: retained artifacts %s\n' "$ART"
node --experimental-vm-modules --test scripts/assemble-wasm-profile.test.mjs \
  wasm/flow_profile_probe.test.mjs wasm/compare_profiles.test.mjs
cargo fmt --check
for profile in render full; do
  case "$profile" in
    render) feature=wasm-bindgen ;;
    full) feature=wasm-full ;;
  esac
  # Independent target trees: neither profile can accidentally reuse the other binary.
  target_dir="$ART/target-$profile"
  CARGO_TARGET_DIR="$target_dir" cargo check --locked --no-default-features --features "$feature" --lib
  CARGO_TARGET_DIR="$target_dir" cargo clippy --locked --no-default-features --features "$feature" --lib -- -D warnings
  CARGO_TARGET_DIR="$target_dir" cargo test --locked --no-default-features --features "$feature" --lib
  CARGO_TARGET_DIR="$target_dir" cargo build --locked --release --no-default-features --features "$feature" \
    --target wasm32-unknown-unknown --lib
  generated="$ART/generated-$profile"
  mkdir "$generated"
  wasm-bindgen "$target_dir/wasm32-unknown-unknown/release/franken_markdown.wasm" \
    --target web --out-dir "$generated"
  node --experimental-vm-modules scripts/assemble-wasm-profile.mjs \
    "$ROOT/wasm" "$generated" "$ART/$profile" "$profile"
  if ! source_fence; then
    printf 'WASM profiles: source changed during build; artifacts are not verified\n' >&2
    exit 1
  fi
done
node wasm/compare_profiles.mjs "$ART/render" "$ART/full" "$ART/profile-report.json" "$SOURCE_COMMIT"
if ! source_fence; then
  printf 'WASM profiles: source changed during comparison; discard the verification claim\n' >&2
  exit 1
fi
printf 'Verified local profiles: %s/render and %s/full\n' "$ART" "$ART"
