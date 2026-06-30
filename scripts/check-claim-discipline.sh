#!/usr/bin/env bash
# check-claim-discipline.sh — README <-> capabilities claim-discipline gate (mwm.9).
#
# Every marketed capability in README.md must be backed by (a) a matching
# `capabilities --json` feature flag with an acceptable status, and (b) a
# registered proof artifact that exists in the repo. The registry is
# scripts/claims.tsv. A claim is only enforced when its README pattern is
# actually present, so the gate never invents claims.
#
# This is the structural antidote to overclaiming (e.g. "first-class WASM" before
# the browser package ships): the WASM-package row is enforced the moment the
# README markets a shipped package (an `npm install` snippet), and fails until
# capabilities reports `wasm_browser_package: available` (flipped by bead 3i5.6).
#
# Usage:
#   scripts/check-claim-discipline.sh [--readme PATH] [run-id]
#   scripts/check-claim-discipline.sh --self-test
set -uo pipefail
cd "$(dirname "$0")/.."

README="README.md"
RUN_ID="local"
SELF_TEST=0
while [ $# -gt 0 ]; do
  case "$1" in
    --readme) README="$2"; shift 2 ;;
    --self-test) SELF_TEST=1; shift ;;
    *) RUN_ID="$1"; shift ;;
  esac
done

REGISTRY="scripts/claims.tsv"
ART="tests/artifacts/claims/${RUN_ID}"
mkdir -p "$ART"

# Build once and capture the capabilities contract.
cargo build --quiet
BIN="${CARGO_TARGET_DIR:-target}/debug/fmd"
[ -x "$BIN" ] || BIN="target/debug/fmd"
CAPS="$("$BIN" capabilities --json 2>/dev/null)"

# Extract a feature flag value from the capabilities JSON.
cap_value() {
  printf '%s' "$CAPS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('features',{}).get('$1','<absent>'))"
}

# Run the gate against a README file; returns 0 if all enforced claims hold.
run_gate() {
  local readme="$1" report="$2"
  : >"$report"
  printf '%-22s | %-7s | %-26s | %-26s | %-8s | %s\n' claim enforced capability_value expected proof result | tee -a "$report"
  local rc=0
  # Skip comment lines and the header row (first field == "label").
  while IFS=$'\t' read -r label pat key expected proof; do
    case "$label" in ''|\#*|label) continue ;; esac
    if grep -qiF -- "$pat" "$readme"; then
      local val; val="$(cap_value "$key")"
      local has_cap=NO has_proof=NO res=FAIL
      case "$val" in *"$expected"*) has_cap=yes ;; esac
      [ -e "$proof" ] && has_proof=yes
      if [ "$has_cap" = yes ] && [ "$has_proof" = yes ]; then res=ok; else res=FAIL; rc=1; fi
      printf '%-22s | %-7s | %-26s | %-26s | %-8s | %s\n' \
        "$label" yes "$val" "$expected" "$has_proof" "$res" | tee -a "$report"
    else
      printf '%-22s | %-7s | %-26s | %-26s | %-8s | %s\n' \
        "$label" no "-" "$expected" "-" "n/a" | tee -a "$report"
    fi
  done <"$REGISTRY"
  return "$rc"
}

# Test-rigor claim-discipline (bead grn.6.2): a "tested/covered" marketing claim in
# the README must be backed by the committed evidence — the ratcheted coverage
# floor, the mutation survivor ceiling, and the e2e suite. Like the registry gate,
# it never invents a claim: it is a no-op unless the README actually markets one.
check_test_rigor() {
  local readme="$1" rc=0
  # A coverage-percentage claim, e.g. "95% line coverage" / "97% code coverage".
  local claimed
  claimed="$(grep -oiE '[0-9]{1,3}% (line |code |test |unit )*coverage' "$readme" \
    | grep -oE '^[0-9]+' | head -1)"
  if [ -n "$claimed" ]; then
    if [ ! -f tests/fixtures/coverage/coverage-floor.txt ]; then
      echo "  test-rigor: README markets coverage but no committed floor exists" >&2; rc=1
    fi
    local measured
    measured="$(grep -F '| lines |' tests/artifacts/coverage/baseline.md 2>/dev/null \
      | grep -oE '[0-9]+\.[0-9]+' | head -1 | grep -oE '^[0-9]+')"
    if [ -n "$measured" ] && [ "$claimed" -gt "$measured" ]; then
      echo "  test-rigor: README claims ${claimed}% coverage but the measured baseline is ${measured}%" >&2
      rc=1
    fi
  fi
  # Mutation-testing marketing must be backed by the committed survivor ceiling.
  if grep -qiE 'mutation[ -](tested|testing|coverage)' "$readme"; then
    [ -f tests/fixtures/mutation/survivor-ceiling.txt ] \
      || { echo "  test-rigor: README markets mutation testing but no committed ceiling exists" >&2; rc=1; }
  fi
  # End-to-end marketing must be backed by the e2e suite.
  if grep -qiE 'end-to-end[ -]tested|e2e[ -]tested|comprehensive(ly)? e2e' "$readme"; then
    [ -f scripts/e2e/run-all.sh ] \
      || { echo "  test-rigor: README markets e2e testing but no run-all suite exists" >&2; rc=1; }
  fi
  return "$rc"
}

if [ "$SELF_TEST" -eq 1 ]; then
  echo "=== claim-discipline self-test ==="
  echo "-- (1) real README must PASS --"
  if run_gate "README.md" "${ART}/self-real.txt"; then echo "  real README: PASS (ok)"; else echo "  real README: unexpectedly FAILED"; exit 1; fi
  echo "-- (2) overclaimed README must FAIL --"
  FAKE="${ART}/overclaimed-README.md"
  cat README.md >"$FAKE"
  printf '\n## Install\n\n```\nnpm install @franken-suite/franken-markdown\n```\n' >>"$FAKE"
  if run_gate "$FAKE" "${ART}/self-fake.txt"; then
    echo "  overclaimed README: UNEXPECTEDLY PASSED — gate has no teeth"; exit 1
  else
    echo "  overclaimed README: correctly FAILED (gate has teeth)"
  fi
  echo "-- (3) real README must pass the test-rigor cross-check --"
  if check_test_rigor "README.md"; then echo "  real README test-rigor: PASS"; else echo "  real README test-rigor: unexpectedly FAILED"; exit 1; fi
  echo "-- (4) an over-claimed coverage % must FAIL the test-rigor cross-check --"
  RIGOR_FAKE="${ART}/rigor-overclaim-README.md"
  printf 'This renderer has 100%% line coverage and is exhaustively tested.\n' >"$RIGOR_FAKE"
  if check_test_rigor "$RIGOR_FAKE"; then
    echo "  over-claimed coverage: UNEXPECTEDLY PASSED — test-rigor gate has no teeth"; exit 1
  else
    echo "  over-claimed coverage: correctly FAILED (test-rigor gate has teeth)"
  fi
  echo "claim-discipline self-test: ok"
  exit 0
fi

echo "=== claim-discipline run=${RUN_ID} readme=${README} ==="
gate_rc=0
run_gate "$README" "${ART}/report.txt" || gate_rc=1
check_test_rigor "$README" || gate_rc=1
if [ "$gate_rc" -eq 0 ]; then
  echo "claim-discipline: ok — every marketed capability is backed by a flag/proof, and every test-rigor claim by committed evidence."
  exit 0
else
  echo "claim-discipline: FAILED — a marketed claim lacks a matching capability flag, proof artifact, or test-rigor evidence (see above)."
  exit 1
fi
