#!/usr/bin/env bash
# Chunked vs monolithic PDF byte parity, heap ceiling, and 10k-page RSS soak
# for bead u9jt.2.
#
# Usage: scripts/pdf-chunk-parity.sh [run-id]
# Options via env:
#   PAGES_1K   requested sections for the 1k overlap fixture (default 1000)
#   PAGES_SOAK requested sections for the RSS soak (default 10000)
#   LINES      filler lines per section (default 48)
#   WIDTH      filler line width (default 72)
#   MAX_HEAP_MB ceiling used for the named-error check (default 1)
# Exit:  0 pass · 64 usage · 66 build/env · 70 assertion failed
set -uo pipefail
cd "$(dirname "$0")/.." || exit
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

RUN_ID="${1:-local}"
fmd_validate_run_id "pdf-chunk-parity" "$RUN_ID"

PAGES_1K="${PAGES_1K:-1000}"
PAGES_SOAK="${PAGES_SOAK:-10000}"
LINES="${LINES:-48}"
WIDTH="${WIDTH:-72}"
MAX_HEAP_MB="${MAX_HEAP_MB:-1}"

ART="$PWD/tests/artifacts/pdf-chunk/${RUN_ID}"
rm -rf -- "$ART"
mkdir -p -- "$ART"

LEDGER="$ART/run.log"
JSONL="$ART/checks.jsonl"
: >"$LEDGER"
: >"$JSONL"

log() { printf '%s\n' "$*" | tee -a "$LEDGER"; }
check() {
  local id="$1" subject="$2" ok="$3"
  local outcome
  if [ "$ok" = "1" ]; then
    outcome=PASS
  else
    outcome=FAIL
  fi
  log "check id=$id subject=$subject outcome=$outcome"
  printf '{"id":"%s","subject":"%s","outcome":"%s"}\n' "$id" "$subject" "$outcome" >>"$JSONL"
  if [ "$outcome" != "PASS" ]; then
    return 1
  fi
}

phase() {
  log "phase=$1 $2"
  printf '{"event":"phase","phase":"%s","detail":"%s"}\n' "$1" "$2" >>"$JSONL"
}

fail=0

phase "build" "example=fmd_pdf_chunk"
if ! cargo build --quiet --example fmd_pdf_chunk; then
  log "pdf-chunk-parity: FAILED to build fmd_pdf_chunk"
  exit 66
fi
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
BIN="$TARGET_DIR/debug/examples/fmd_pdf_chunk"
if [ ! -x "$BIN" ]; then
  BIN="$PWD/target/debug/examples/fmd_pdf_chunk"
fi
if [ ! -x "$BIN" ]; then
  log "pdf-chunk-parity: harness missing"
  exit 66
fi
check "u9jt.2.e2e.build" "fmd_pdf_chunk built" "1" || fail=1

phase "parity-1k" "pages=$PAGES_1K lines=$LINES width=$WIDTH"
CHUNKED_PDF="$ART/chunked-1k.pdf"
MONO_PDF="$ART/monolithic-1k.pdf"
if ! "$BIN" --pages "$PAGES_1K" --lines "$LINES" --width "$WIDTH" \
  --emission chunked --out "$CHUNKED_PDF" --verbose \
  >"$ART/chunked-1k.json" 2>"$ART/chunked-1k.err"; then
  log "pdf-chunk-parity: chunked 1k render failed"
  cat "$ART/chunked-1k.err" >>"$LEDGER"
  fail=1
fi
if ! "$BIN" --pages "$PAGES_1K" --lines "$LINES" --width "$WIDTH" \
  --emission monolithic --out "$MONO_PDF" \
  >"$ART/monolithic-1k.json" 2>"$ART/monolithic-1k.err"; then
  log "pdf-chunk-parity: monolithic 1k render failed"
  cat "$ART/monolithic-1k.err" >>"$LEDGER"
  fail=1
fi
if [ -s "$CHUNKED_PDF" ] && [ -s "$MONO_PDF" ] && cmp -s -- "$CHUNKED_PDF" "$MONO_PDF"; then
  check "u9jt.2.e2e.1k.parity" "1k-page chunked bytes == monolithic bytes" "1" || fail=1
else
  if [ -s "$CHUNKED_PDF" ] && [ -s "$MONO_PDF" ]; then
    python3 - "$CHUNKED_PDF" "$MONO_PDF" >>"$LEDGER" <<'PY'
import sys
a=open(sys.argv[1],"rb").read()
b=open(sys.argv[2],"rb").read()
n=min(len(a),len(b))
at=next((i for i in range(n) if a[i]!=b[i]), n)
print(f"byte mismatch at {at} chunked_len={len(a)} monolithic_len={len(b)}")
PY
  fi
  check "u9jt.2.e2e.1k.parity" "1k-page chunked bytes == monolithic bytes" "0" || fail=1
fi
if grep -q "phase=pagination" "$ART/chunked-1k.err" && grep -q "phase=pages_ready" "$ART/chunked-1k.err"; then
  check "u9jt.2.e2e.verbose" "verbose stderr logs pagination and pages_ready" "1" || fail=1
else
  check "u9jt.2.e2e.verbose" "verbose stderr logs pagination and pages_ready" "0" || fail=1
fi

phase "ceiling" "max-heap-mb=$MAX_HEAP_MB"
set +e
"$BIN" --pages 8 --lines 12 --width 40 --max-heap-mb "$MAX_HEAP_MB" \
  >"$ART/ceiling.json" 2>"$ART/ceiling.err"
ceiling_rc=$?
set -e
if [ "$ceiling_rc" -ne 0 ] && grep -q "pdf_heap_ceiling:" "$ART/ceiling.err"; then
  check "u9jt.2.e2e.ceiling" "--max-heap-mb rejects with pdf_heap_ceiling:" "1" || fail=1
else
  log "ceiling rc=$ceiling_rc stderr=$(tr '\n' ' ' < "$ART/ceiling.err")"
  check "u9jt.2.e2e.ceiling" "--max-heap-mb rejects with pdf_heap_ceiling:" "0" || fail=1
fi

phase "soak-10k" "pages=$PAGES_SOAK"
TIME_MODE=""
if /usr/bin/time -l true >/dev/null 2>&1; then
  TIME_MODE="bsd"
elif /usr/bin/time -v true >/dev/null 2>&1; then
  TIME_MODE="gnu"
fi
SOAK_PDF="$ART/chunked-soak.pdf"
SOAK_TIME="$ART/soak-time.txt"
if [ -n "$TIME_MODE" ]; then
  if [ "$TIME_MODE" = "bsd" ]; then
    /usr/bin/time -l "$BIN" --pages "$PAGES_SOAK" --lines "$LINES" --width "$WIDTH" \
      --emission chunked --out "$SOAK_PDF" \
      >"$ART/soak.json" 2>"$SOAK_TIME"
  else
    /usr/bin/time -v "$BIN" --pages "$PAGES_SOAK" --lines "$LINES" --width "$WIDTH" \
      --emission chunked --out "$SOAK_PDF" \
      >"$ART/soak.json" 2>"$SOAK_TIME"
  fi
  soak_rc=$?
else
  "$BIN" --pages "$PAGES_SOAK" --lines "$LINES" --width "$WIDTH" \
    --emission chunked --out "$SOAK_PDF" \
    >"$ART/soak.json" 2>"$SOAK_TIME"
  soak_rc=$?
fi
if [ "$soak_rc" -eq 0 ] && [ -s "$SOAK_PDF" ]; then
  check "u9jt.2.e2e.soak" "10k-page chunked soak produced a PDF" "1" || fail=1
else
  check "u9jt.2.e2e.soak" "10k-page chunked soak produced a PDF" "0" || fail=1
fi
if [ -s "$SOAK_TIME" ]; then
  log "rss-log:"
  if [ "$TIME_MODE" = "bsd" ]; then
    grep -E "maximum resident set size|real " "$SOAK_TIME" | tee -a "$LEDGER" || true
  elif [ "$TIME_MODE" = "gnu" ]; then
    grep -E "Maximum resident set size|Elapsed" "$SOAK_TIME" | tee -a "$LEDGER" || true
  else
    log "time(1) unavailable; soak stderr retained at $SOAK_TIME"
  fi
  check "u9jt.2.e2e.rss" "RSS soak log captured" "1" || fail=1
else
  check "u9jt.2.e2e.rss" "RSS soak log captured" "0" || fail=1
fi

if [ "$fail" -ne 0 ]; then
  log "pdf-chunk-parity: FAILED"
  exit 70
fi
log "pdf-chunk-parity: PASS"
exit 0
