#!/usr/bin/env bash
# Release parity gate (bead smif.1): fails CI when the distributable surfaces
# disagree about versions. Single source of truth: the root Cargo.toml version;
# the member crates (fmd-font, fmd-math) have their own independent version
# lines (0.1.x — they are versioned as standalone libraries), so those are
# checked for PRESENCE and internal consistency (wasm/package.json must match
# the ROOT version), not for equality with the root.
#
# Usage: scripts/check-release-parity.sh [--tag <tag>]
#   --tag <tag>: additionally verify the tag's version suffix matches the root
#                version (for release-time checks; v<version> form).
# Exit codes: 0 parity; 1 drift; 2 usage error. stdout is data (one line per
# surface), stderr is diagnostics — agent-first contract per AGENTS.md.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

want_tag=""
if [[ "${1:-}" == "--tag" ]]; then
  [[ -n "${2:-}" ]] || { echo "usage: $0 [--tag <tag>]" >&2; exit 2; }
  want_tag="$2"
fi

root_version="$(grep -m1 '^version' Cargo.toml | sed 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/')"
font_version="$(grep -m1 '^version' fmd-font/Cargo.toml | sed 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/')"
math_version="$(grep -m1 '^version' fmd-math/Cargo.toml | sed 's/^version[[:space:]]*=[[:space:]]*"\(.*\)"/\1/')"
wasm_version="$(grep -m1 '"version"' wasm/package.json | sed 's/.*"version"[[:space:]]*:[[:space:]]*"\(.*\)".*/\1/')"

fail=0
echo "root Cargo.toml:      $root_version"
echo "wasm package.json:    $wasm_version"

# The browser package MUST track the root version exactly.
if [[ "$wasm_version" != "$root_version" ]]; then
  echo "DRIFT: wasm/package.json ($wasm_version) != root ($root_version)" >&2
  fail=1
fi

# Member libraries: independent version lines, but they must be present and
# non-empty (a packaging regression could blank them).
for pair in "fmd-font:$font_version" "fmd-math:$math_version"; do
  name="${pair%%:*}"; ver="${pair##*:}"
  [[ -n "$ver" ]] || { echo "DRIFT: $name has no version" >&2; fail=1; }
  echo "$name:              $ver"
done

# Registry-side checks (crates.io / npm published versions) are done by the
# release workflow via the APIs, not here: this gate stays offline-deterministic.

if [[ -n "$want_tag" ]]; then
  tag_version="${want_tag#v}"
  if [[ "$tag_version" != "$root_version" ]]; then
    echo "DRIFT: tag $want_tag implies $tag_version != root $root_version" >&2
    fail=1
  fi
fi

exit "$fail"
