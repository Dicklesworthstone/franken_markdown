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
  echo "claim-discipline self-test: ok"
  exit 0
fi

echo "=== claim-discipline run=${RUN_ID} readme=${README} ==="
if run_gate "$README" "${ART}/report.txt"; then
  echo "claim-discipline: ok — every marketed capability is backed by a capability flag and a proof artifact."
  exit 0
else
  echo "claim-discipline: FAILED — a marketed claim lacks a matching capability flag or proof (see FAIL rows)."
  exit 1
fi
