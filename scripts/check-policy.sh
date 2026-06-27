#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"

fail() {
  printf 'fmd policy check: %s\n' "$*" >&2
  exit 1
}

root_name="$(cargo metadata --no-deps --format-version 1 \
  | tr -d '\n' \
  | sed -n 's/.*"packages":\[{"name":"\([^"]*\)".*/\1/p')"

if [ "$root_name" != "franken_markdown" ]; then
  fail "unexpected root package '$root_name' (expected franken_markdown)"
fi

echo "fmd policy check: no-default core dependency graph"
core_tree="$(cargo tree --no-default-features --prefix none --edges normal)"
core_lines="$(printf '%s\n' "$core_tree" | sed '/^[[:space:]]*$/d' | wc -l | tr -d ' ')"
if [ "$core_lines" != "1" ]; then
  printf '%s\n' "$core_tree" >&2
  fail "the --no-default-features core must have zero third-party normal dependencies"
fi

case "$core_tree" in
  franken_markdown\ v*) ;;
  *)
    printf '%s\n' "$core_tree" >&2
    fail "unexpected --no-default-features cargo tree root"
    ;;
esac

echo "fmd policy check: banned dependency forest scan"
all_feature_crates="$(cargo tree --all-features --prefix none --edges normal \
  | awk '{print $1}' \
  | sort -u)"

banned=(
  blitz
  chromiumoxide
  comrak
  cosmic-text
  fantoccini
  fontconfig
  fontconfig-sys
  headless_chrome
  ironpress
  krilla
  onig
  onig_sys
  pulldown-cmark
  reqwest
  syntect
  thirtyfour
  tokio
  typst
  yeslogic-fontconfig-sys
)

for crate in "${banned[@]}"; do
  if printf '%s\n' "$all_feature_crates" | grep -Fxq "$crate"; then
    fail "banned crate '$crate' is present in the all-features dependency graph"
  fi
done

echo "fmd policy check: no native build script"
if [ -e build.rs ]; then
  fail "build.rs is not allowed without an explicit architecture decision"
fi

echo "fmd policy check: unsafe policy is enforced by Rust lints"
grep -q 'unsafe_code = "forbid"' Cargo.toml \
  || fail 'Cargo.toml must keep [lints.rust] unsafe_code = "forbid"'
grep -q '#!\[forbid(unsafe_code)\]' src/lib.rs \
  || fail 'src/lib.rs must keep #![forbid(unsafe_code)]'

cargo check --no-default-features --lib >/dev/null

echo "fmd policy check: ok"
