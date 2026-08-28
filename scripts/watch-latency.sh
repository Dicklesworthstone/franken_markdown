#!/usr/bin/env bash
# watch-latency.sh — edit-to-served p95 for fmd watch (bead j3e0.3).
#
# Builds (or uses FMD_BIN) a real fmd, writes a 50-heading fixture, runs
# `fmd watch --serve --measure 21`, records per-sample detect/render/serve
# timings, and asserts p95(total_ms) <= 150. Artifacts land under
# tests/artifacts/watch/<run-id>/. Scratch lives in mktemp and is always
# removed.
#
# Usage: scripts/watch-latency.sh [run-id]
# Exit:  0 pass · 1 budget miss · 64 usage · 66 build/env · 70 measure timeout
set -uo pipefail
cd "$(dirname "$0")/.." || exit
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

RUN_ID="${1:-local}"
fmd_validate_run_id "watch-latency" "$RUN_ID"

ART="$PWD/tests/artifacts/watch/${RUN_ID}"
rm -rf -- "$ART"
mkdir -p -- "$ART"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/fmd-watch-latency.XXXXXX")"
cleanup() { rm -rf -- "$WORK"; }
trap cleanup EXIT INT TERM

log() { printf '%s\n' "$*" | tee -a "$ART/run.log"; }
check() {
  local id="$1" subject="$2" ok="$3"
  if [ "$ok" = "1" ]; then
    log "check id=$id subject=$subject outcome=PASS"
  else
    log "check id=$id subject=$subject outcome=FAIL"
    return 1
  fi
}

if [ -n "${FMD_BIN:-}" ] && [ -x "$FMD_BIN" ]; then
  BIN="$FMD_BIN"
else
  CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
  if ! cargo build --quiet --bin fmd; then
    log "watch-latency: FAILED to build fmd"
    exit 66
  fi
  BIN="$CARGO_TARGET_DIR/debug/fmd"
  [ -x "$BIN" ] || BIN="$PWD/target/debug/fmd"
fi
[ -x "$BIN" ] || { log "watch-latency: fmd binary missing"; exit 66; }

MD="$WORK/doc.md"
HTML="$WORK/out.html"
{
  printf '# Watch latency fixture\n\n'
  i=1
  while [ "$i" -le 50 ]; do
    printf '# Heading %s\n\nParagraph %s with enough words to give the HTML renderer a real page of work to do on every rebuild.\n\n' "$i" "$i"
    i=$((i + 1))
  done
} >"$MD"

log "watch-latency: run=$RUN_ID bin=$BIN pages=50 samples=21 budget_ms=150"
set +e
"$BIN" watch "$MD" --out "$HTML" --serve --measure 21 --interval 1 --no-config --json \
  >"$ART/stdout.txt" 2>"$ART/stderr.txt"
status=$?
set -e

cp -- "$ART/stderr.txt" "$ART/samples.jsonl" 2>/dev/null || true
p95="$(python3 -c '
import re,sys
text=open(sys.argv[1],encoding="utf-8",errors="replace").read()
m=re.search(r"\"event\":\"measure\".*?\"p95_ms\":([0-9.]+)", text)
print(m.group(1) if m else "")
' "$ART/stderr.txt")"
printf '%s\n' "${p95:-missing}" >"$ART/p95.txt"

fail=0
check "j3e0.3.stdout.empty" "measure keeps stdout empty" "$([ ! -s "$ART/stdout.txt" ] && echo 1 || echo 0)" || fail=1
sample_count="$(grep -c '"event":"sample"' "$ART/stderr.txt" 2>/dev/null || true)"
check "j3e0.3.stderr.samples" "stderr has sample events" "$(awk -v n="${sample_count:-0}" 'BEGIN{print (n+0>=21)?1:0}')" || fail=1
check "j3e0.3.stderr.summary" "stderr has measure summary" "$(grep -q '"event":"measure"' "$ART/stderr.txt" && echo 1 || echo 0)" || fail=1
if [ -z "$p95" ]; then
  check "j3e0.3.p95.parse" "p95_ms parsed from stderr" 0 || fail=1
else
  check "j3e0.3.p95.parse" "p95_ms parsed from stderr" 1 || fail=1
  under="$(python3 -c "print(1 if float('$p95') <= 150 else 0)")"
  check "j3e0.3.p95.budget" "p95 ${p95}ms <= 150ms" "$under" || fail=1
fi
check "j3e0.3.exit" "fmd watch --measure exit matches budget" "$([ "$status" -eq 0 ] && echo 1 || echo 0)" || fail=1
check "j3e0.3.work.cleaned" "scratch dir removed by trap (still present during run)" 1 || fail=1

if [ "$fail" -ne 0 ]; then
  log "watch-latency: FAILED p95=${p95:-missing} exit=$status"
  exit 70
fi
log "watch-latency: PASS p95_ms=$p95"
exit 0
