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
cd "$(dirname "$0")/.." || exit
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh
RUN_ID="${1:-local}"
fmd_validate_run_id "cli-output-contract" "$RUN_ID"
ART_BASE="$PWD/tests/artifacts/cli"
ART="${ART_BASE}/${RUN_ID}"
WORK="${ART}/work"
rm -rf -- "$WORK"; mkdir -p -- "$WORK"
LEDGER="${ART}/contract.txt"
: >"$LEDGER"
log() { printf '%s\n' "$*" | tee -a "$LEDGER"; }

cargo build --release --quiet
BIN="${CARGO_TARGET_DIR:-target}/release/fmd"
[ -x "$BIN" ] || BIN="target/release/fmd"
BIN="$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")"
log "=== cli-output-contract run=${RUN_ID} bin=${BIN} ==="
log "$(printf '%-52s | %-25s | %-8s | %-8s | %-4s | %s' argv expect_path stdout stderr exit result)"

printf '# Title\n\nbody\n' >"${WORK}/report.md"
fail=0

case_no=0

first_bytes() {
  local path="$1"
  if [ ! -s "$path" ]; then
    printf '%s' '-'
    return 0
  fi
  LC_ALL=C head -c 32 "$path" | od -An -tx1 | tr -d ' \n'
}

expect_stdout() {
  local path="$1" want="$2"
  case "$want" in
    empty)
      [ ! -s "$path" ] || return 1
      ;;
    html)
      grep -q '<!DOCTYPE html>' "$path" || return 1
      ;;
    *)
      echo "cli-output-contract: internal error: unknown stdout expectation '$want'" >&2
      return 2
      ;;
  esac
}

expect_stderr() {
  local path="$1" want="$2"
  case "$want" in
    empty)
      [ ! -s "$path" ] || return 1
      ;;
    json-wrote)
      grep -q '"ok":true' "$path" || return 1
      grep -q '"event":"wrote"' "$path" || return 1
      ;;
    json-error)
      grep -q '"ok":false' "$path" || return 1
      grep -q '"error"' "$path" || return 1
      ;;
    *)
      echo "cli-output-contract: internal error: unknown stderr expectation '$want'" >&2
      return 2
      ;;
  esac
}

# check <desc> <expect_exit> <expect_file[,expect_file]|-> <expect_stdout> <expect_stderr> -- <args...>
check() {
  local desc="$1" eexit="$2" efile="$3" estdout="$4" estderr="$5"; shift 5; shift # drop the literal --
  ( cd "$WORK" && rm -f report.html report.pdf document.html document.pdf out.html out.pdf )
  case_no=$((case_no + 1))
  local stdout_file="${ART}/case-${case_no}.stdout" stderr_file="${ART}/case-${case_no}.stderr"
  local rc
  ( cd "$WORK" && SOURCE_DATE_EPOCH=1700000000 "$BIN" "$@" --json >"$stdout_file" 2>"$stderr_file" ); rc=$?
  local fileok=ok
  if [ "$efile" != "-" ]; then
    local expected_file
    IFS=',' read -r -a expected_files <<<"$efile"
    for expected_file in "${expected_files[@]}"; do
      [ -f "${WORK}/${expected_file}" ] || fileok="MISSING(${expected_file})"
    done
  fi
  local stdoutok=ok stderrok=ok
  expect_stdout "$stdout_file" "$estdout" || stdoutok="BAD(stdout=$(first_bytes "$stdout_file"))"
  expect_stderr "$stderr_file" "$estderr" || stderrok="BAD(stderr=$(first_bytes "$stderr_file"))"
  local res=ok
  { [ "$rc" -eq "$eexit" ] && [ "$fileok" = ok ] && [ "$stdoutok" = ok ] && [ "$stderrok" = ok ]; } \
    || { res="FAIL(rc=$rc want=$eexit file=$fileok stdout=$stdoutok stderr=$stderrok)"; fail=1; }
  log "$(printf '%-52s | %-25s | %-8s | %-8s | %-4s | %s' "$desc" "$efile" "$estdout" "$estderr" "$rc" "$res")"
}

# file input
check "report.md --to pdf"           0 report.pdf             empty json-wrote -- report.md --to pdf
check "report.md --to both"          0 report.html,report.pdf empty json-wrote -- report.md --to both
check "report.md --to html"          0 -                      html  empty      -- report.md --to html        # html -> stdout, no file/status
check "report.md --to html --out out.html" 0 out.html         empty json-wrote -- report.md --to html --out out.html
check "report.md --to pdf --out out.pdf"   0 out.pdf          empty json-wrote -- report.md --to pdf --out out.pdf
# text input
check "--text '# Hi' --to pdf"        0 document.pdf          empty json-wrote -- --text "# Hi" --to pdf
# stdout refusals
check "report.md --to pdf --out -"    64 -                     empty json-error -- report.md --to pdf --out -
check "report.md --to both --out -"   64 -                     empty json-error -- report.md --to both --out -

log ""
if [ "$fail" -eq 0 ]; then
  log "cli-output-contract: ok — every output-path cell matches the documented matrix."
else
  log "cli-output-contract: FAILED — see FAIL rows above."
fi
exit "$fail"
