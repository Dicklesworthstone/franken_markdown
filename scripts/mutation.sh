#!/usr/bin/env bash
# scripts/mutation.sh — mutation-testing ratchet with cargo-mutants (bead grn.5.4).
#
# Line/branch coverage proves a line RAN; mutation testing proves a test FAILS when
# the code is wrong. cargo-mutants rewrites the source (negate a condition, swap an
# operator, replace a return value, ...) and reruns the suite; a "missed" mutant is
# one no test caught — a hole in test EFFECTIVENESS, not just reachability.
#
# Running it over the whole 16k-line engine is hours of work, so this gates a
# curated, well-tested scope (override with FMD_MUTANTS_FILES="a.rs b.rs") and
# enforces a RATCHETED survivor ceiling: the number of surviving mutants can only
# go down. A survivor ledger records exactly which mutants escaped so they can be
# triaged into new tests.
#
# Usage:
#   scripts/mutation.sh [run-id]      # run, write ledger, enforce the ceiling
#   scripts/mutation.sh --update-ceiling   # run + set the ceiling to current survivors
#   scripts/mutation.sh --self-test   # verify the parser/ratchet logic (fast, no run)
#   scripts/mutation.sh --help
#
# Exit: 0 ok · 2 missing prerequisite/usage · 3 cargo-mutants run failed ·
#       5 survivors exceeded the committed ceiling.
set -uo pipefail
cd "$(dirname "$0")/.." || exit
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$PWD/target/fmd-checks}"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

# Curated scope: small, logic-bearing, thoroughly-tested utility modules where a
# survivor ceiling of 0 is achievable and the full run is fast. The large engine
# modules (pdf/layout/text/parse/html/compress/highlight) are NOT mutation-tested
# here — they each carry hundreds of mutants (hours of CI) and known survivors —
# and are instead guarded by the golden, differential, metamorphic, fuzz, and
# conformance suites. Expand this list only with files proven to reach 0 survivors.
DEFAULT_FILES="src/span.rs src/error.rs src/fonts.rs"
FILES="${FMD_MUTANTS_FILES:-$DEFAULT_FILES}"
CEILING_FILE="tests/fixtures/mutation/survivor-ceiling.txt"
LEDGER_FILE="tests/fixtures/mutation/survivors.txt"
TIMEOUT="${FMD_MUTANTS_TIMEOUT:-180}"

MODE="run"
RUN_ID="local"
case "${1:-}" in
  --help|-h)        sed -n '2,30p' "$0"; exit 0 ;;
  --self-test)      MODE="self-test" ;;
  --update-ceiling) MODE="update" ;;
  "")               ;;
  --*)              echo "mutation: unknown flag '$1' (try --help)" >&2; exit 2 ;;
  *)                RUN_ID="$1" ;;
esac
fmd_validate_run_id "mutation" "$RUN_ID"

# Parse a cargo-mutants outcomes.json: print "<missed> <caught> <unviable> <timeout>"
# then the survivor "file:line  desc" lines. Shared by run + self-test.
parse_outcomes() {
  python3 - "$1" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(f"{d.get('missed',0)} {d.get('caught',0)} {d.get('unviable',0)} {d.get('timeout',0)}")
for o in d.get("outcomes", []):
    if o.get("summary") == "MissedMutant" or o.get("summary") == "Missed":
        scn = o.get("scenario", {})
        m = scn.get("Mutant", scn) if isinstance(scn, dict) else {}
        f = m.get("file", "?"); ln = m.get("line", "?")
        desc = m.get("function", {}).get("function_name", "") if isinstance(m.get("function"), dict) else ""
        rep = m.get("replacement", "")
        print(f"SURVIVOR\t{f}:{ln}\t{desc}\t{rep}")
PY
}

if [ "$MODE" = "self-test" ]; then
  tmp="$(mktemp)"
  cat >"$tmp" <<'JSON'
{"missed":1,"caught":4,"unviable":2,"timeout":0,"outcomes":[
 {"summary":"CaughtMutant"},
 {"summary":"MissedMutant","scenario":{"Mutant":{"file":"src/x.rs","line":42,"replacement":"true","function":{"function_name":"foo"}}}}
]}
JSON
  out="$(parse_outcomes "$tmp")"
  rm -f "$tmp"
  counts="$(printf '%s\n' "$out" | head -1)"
  survs="$(printf '%s\n' "$out" | grep -c '^SURVIVOR')"
  if [ "$counts" = "1 4 2 0" ] && [ "$survs" = "1" ]; then
    echo "mutation: self-test ok (parsed missed=1, caught=4, 1 survivor at src/x.rs:42)"
    exit 0
  fi
  echo "mutation: self-test FAILED — parser returned '$counts' survivors=$survs" >&2
  exit 5
fi

command -v cargo-mutants >/dev/null 2>&1 || { echo "mutation: cargo-mutants not installed (cargo install cargo-mutants)" >&2; exit 2; }
mkdir -p "$(dirname "$CEILING_FILE")"

ART="tests/artifacts/mutation/${RUN_ID}"
mkdir -p "$ART"
FARGS=()
for f in $FILES; do FARGS+=(-f "$f"); done

echo "mutation: running cargo-mutants over: $FILES (timeout ${TIMEOUT}s/mutant)"
# cargo-mutants exits non-zero when mutants survive; we ratchet on the parsed count,
# so don't let that abort the script.
cargo mutants "${FARGS[@]}" --output "$ART" --timeout "$TIMEOUT" >"$ART/run.log" 2>&1 || true
OUTCOMES="$ART/mutants.out/outcomes.json"
[ -f "$OUTCOMES" ] || { echo "mutation: cargo-mutants produced no outcomes.json (see $ART/run.log)" >&2; exit 3; }

PARSED="$(parse_outcomes "$OUTCOMES")"
read -r MISSED CAUGHT UNVIABLE TIMEOUT_N <<<"$(printf '%s\n' "$PARSED" | head -1)"
printf '%s\n' "$PARSED" | grep '^SURVIVOR' | cut -f2- >"$ART/survivors.tsv" || true

echo "mutation: scope $FILES — caught=$CAUGHT missed(survived)=$MISSED unviable=$UNVIABLE timeout=$TIMEOUT_N"

# Write the committed survivor ledger (human-reviewable list of escaped mutants).
{
  echo "# Mutation-testing survivor ledger (grn.5.4) — mutants no test caught."
  echo "# Scope: $FILES. Regenerate: scripts/mutation.sh --update-ceiling"
  echo "# survivors=$MISSED"
  if [ "$MISSED" -gt 0 ]; then
    printf '%s\n' "$PARSED" | grep '^SURVIVOR' | cut -f2-
  else
    echo "# (none — every mutant in scope was caught by a test)"
  fi
} >"$LEDGER_FILE"

if [ "$MODE" = "update" ]; then
  echo "$MISSED" >"$CEILING_FILE"
  echo "mutation: ceiling updated to $MISSED survivor(s); ledger at $LEDGER_FILE"
  exit 0
fi

CEILING=0
[ -s "$CEILING_FILE" ] && CEILING="$(tr -dc '0-9' <"$CEILING_FILE")"
if [ "$MISSED" -gt "$CEILING" ]; then
  echo "mutation: FAILED — $MISSED survivor(s) > committed ceiling $CEILING (test effectiveness regressed)." >&2
  echo "mutation: triage the new survivors in $ART/survivors.tsv into tests, or justify + --update-ceiling." >&2
  exit 5
fi
echo "mutation: ok — $MISSED survivor(s) <= ceiling $CEILING."
