#!/usr/bin/env bash
# scripts/test-all.sh — run the whole verification gauntlet and report (bead grn.6.1).
#
# One command that runs every test tier — format, lint, unit/integration tests,
# the batch feature, the property + fuzz + golden suites (via cargo test), the
# clean-room/determinism/conformance checks, the ratcheted coverage floor, and the
# comprehensive e2e suite — and prints a single combined pass/fail report with
# per-gate timing. This is the "did everything pass?" entry point for humans and
# agents; CI runs the same gates split across parallel jobs.
#
# Usage:
#   scripts/test-all.sh           # full gauntlet (slow: coverage + e2e + asupersync)
#   scripts/test-all.sh --fast    # skip the heavy gates (coverage floor + e2e run-all)
#   scripts/test-all.sh --help
#
# Exit: 0 all gates passed · 70 one or more gates failed · 2 usage error.
set -uo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$PWD/target/fmd-checks}"

FAST=0
case "${1:-}" in
  --fast) FAST=1 ;;
  --help|-h) sed -n '2,20p' "$0"; exit 0 ;;
  "") ;;
  *) echo "test-all: unknown argument '$1' (try --help)" >&2; exit 2 ;;
esac

RESULTS=()
FAILED=0

run_gate() {
  # run_gate <name> <command...>
  local name="$1"; shift
  printf '\n──▶ %s\n' "$name"
  local start end rc
  start=$(date +%s 2>/dev/null || echo 0)
  if "$@"; then rc=0; else rc=$?; fi
  end=$(date +%s 2>/dev/null || echo 0)
  local secs=$((end - start))
  if [ "$rc" -eq 0 ]; then
    RESULTS+=("ok    ${secs}s  ${name}")
    printf '   ✓ %s (%ss)\n' "$name" "$secs"
  else
    RESULTS+=("FAIL  ${secs}s  ${name} (rc=$rc)")
    FAILED=$((FAILED + 1))
    printf '   ✗ %s FAILED (rc=%s, %ss)\n' "$name" "$rc" "$secs"
  fi
}

echo "test-all: running the verification gauntlet ($([ "$FAST" = 1 ] && echo "fast" || echo "full"))"

# --- static + unit gates (always) -------------------------------------------
run_gate "fmt"                cargo fmt --check
run_gate "check-all-targets"  cargo check --all-targets
run_gate "check-no-default"   cargo check --no-default-features --lib
run_gate "clippy"             cargo clippy --all-targets -- -D warnings
run_gate "unit+integration (cargo test)" cargo test
run_gate "batch feature"      cargo test --features batch --lib batch::
run_gate "clippy (batch)"     cargo clippy --features batch --lib -- -D warnings

# --- clean-room / determinism / contract checks -----------------------------
run_gate "clean-room policy"  scripts/check-policy.sh
run_gate "wasm core build"    scripts/check-wasm-core.sh
run_gate "determinism"        scripts/check-determinism.sh
run_gate "test-doubles gate"  scripts/check-test-doubles.sh
run_gate "e2e harness self-test" scripts/e2e/lib.sh --self-test
run_gate "commonmark conformance floor" scripts/commonmark-conformance.sh test-all

# --- heavy gates (skipped under --fast) -------------------------------------
if [ "$FAST" = 1 ]; then
  RESULTS+=("skip  --    coverage floor (--fast)")
  RESULTS+=("skip  --    e2e run-all (--fast)")
else
  run_gate "coverage floor (line/region/branch)" scripts/coverage.sh --check test-all
  run_gate "e2e run-all" scripts/e2e/run-all.sh test-all
fi

# --- combined report --------------------------------------------------------
echo ""
echo "════════════════════════ test-all summary ════════════════════════"
for r in "${RESULTS[@]}"; do printf '  %s\n' "$r"; done
echo "═══════════════════════════════════════════════════════════════════"
if [ "$FAILED" -ne 0 ]; then
  echo "test-all: FAILED — $FAILED gate(s) did not pass."
  exit 70
fi
echo "test-all: ok — every gate passed."
exit 0
