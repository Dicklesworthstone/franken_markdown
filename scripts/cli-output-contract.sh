#!/usr/bin/env bash
# cli-output-contract.sh — e2e proof of the fmd output-path contract (bead mwm.8).
#
# Exercises {file|stdin|text} x {html|pdf|both} x {--out path | --out - | omitted}
# against the real binary and logs (argv | resolved_path | exit | first_bytes) for
# every cell, asserting against the documented matrix. Detailed log + artifacts
# land under tests/artifacts/cli/<run-id>/.
#
# Usage: scripts/cli-output-contract.sh [run-id]
set -uo pipefail
cd "$(dirname "$0")/.."
RUN_ID="${1:-local}"
ART="tests/artifacts/cli/${RUN_ID}"
WORK="${ART}/work"
rm -rf "$WORK"; mkdir -p "$WORK"
LEDGER="${ART}/contract.txt"
: >"$LEDGER"
log() { printf '%s\n' "$*" | tee -a "$LEDGER"; }

cargo build --release --quiet
BIN="${CARGO_TARGET_DIR:-target}/release/fmd"
[ -x "$BIN" ] || BIN="target/release/fmd"
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
log "=== cli-output-contract run=${RUN_ID} bin=${BIN} ==="
log "$(printf '%-52s | %-20s | %-4s | %s' argv expect_path exit result)"

printf '# Title\n\nbody\n' >"${WORK}/report.md"
fail=0

# check <desc> <expect_exit> <expect_file|-> -- <args...>
check() {
  local desc="$1" eexit="$2" efile="$3"; shift 3; shift # drop the literal --
  ( cd "$WORK" && rm -f report.html report.pdf document.html document.pdf out.html out.pdf )
  local out rc
  out="$(cd "$WORK" && SOURCE_DATE_EPOCH=1700000000 "$BIN" "$@" --json 2>&1)"; rc=$?
  local fileok=ok
  if [ "$efile" != "-" ]; then
    [ -f "${WORK}/${efile}" ] || fileok="MISSING(${efile})"
  fi
  local res=ok
  { [ "$rc" -eq "$eexit" ] && [ "$fileok" = ok ]; } || { res="FAIL(rc=$rc want=$eexit file=$fileok)"; fail=1; }
  log "$(printf '%-52s | %-20s | %-4s | %s' "$desc" "$efile" "$rc" "$res")"
}

# file input
check "report.md --to pdf"           0 report.pdf  -- report.md --to pdf
check "report.md --to both"          0 report.pdf  -- report.md --to both
check "report.md --to html"          0 -           -- report.md --to html        # html -> stdout, no file
check "report.md --to html --out out.html" 0 out.html -- report.md --to html --out out.html
check "report.md --to pdf --out out.pdf"   0 out.pdf  -- report.md --to pdf --out out.pdf
# text input
check "--text '# Hi' --to pdf"        0 document.pdf -- --text "# Hi" --to pdf
# stdout refusals
check "report.md --to pdf --out -"    64 -          -- report.md --to pdf --out -
check "report.md --to both --out -"   64 -          -- report.md --to both --out -

log ""
if [ "$fail" -eq 0 ]; then
  log "cli-output-contract: ok — every output-path cell matches the documented matrix."
else
  log "cli-output-contract: FAILED — see FAIL rows above."
fi
exit "$fail"
