#!/usr/bin/env bash
# variable-font-e2e.sh — gk3v.4 CLI gate: mixed-weight PDF via --pdf-font slot,
# determinism, size report, hostile fvar/gvar bytes that must not abort.
#
# Usage: scripts/variable-font-e2e.sh [run-id]
# Exit:  0 pass · 64 usage · 66 build/env · 70 assertion failed
set -uo pipefail
cd "$(dirname "$0")/.." || exit
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

RUN_ID="${1:-local}"
fmd_validate_run_id "variable-font-e2e" "$RUN_ID"

ART="$PWD/tests/artifacts/variable-font/${RUN_ID}"
rm -rf -- "$ART"
mkdir -p -- "$ART"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/fmd-gk3v4.XXXXXX")"
cleanup() { rm -rf -- "$WORK"; }
trap cleanup EXIT INT TERM

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
  log "phase=$1"
  printf '{"event":"phase","phase":"%s"}\n' "$1" >>"$JSONL"
}

fail=0

if [ -n "${FMD_BIN:-}" ] && [ -x "$FMD_BIN" ]; then
  BIN="$FMD_BIN"
else
  CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
  phase "build"
  if ! cargo build --quiet --bin fmd; then
    log "variable-font-e2e: FAILED to build fmd"
    exit 66
  fi
  BIN="$CARGO_TARGET_DIR/debug/fmd"
  [ -x "$BIN" ] || BIN="$PWD/target/debug/fmd"
fi
[ -x "$BIN" ] || { log "variable-font-e2e: fmd binary missing"; exit 66; }

phase "dump-fixture"
VF="$WORK/FmdTriangleVF.ttf"
if ! FMD_DUMP_TRIANGLE_VF="$VF" cargo test -p fmd-font dump_triangle_vf_when_requested -- --exact --quiet; then
  log "variable-font-e2e: FAILED to dump triangle VF"
  exit 66
fi
check "gk3v.4.e2e.dump" "triangle VF dumped" "$([ -s "$VF" ] && echo 1 || echo 0)" || fail=1
cp -- "$VF" "$ART/FmdTriangleVF.ttf"

MD="$WORK/mixed.md"
printf '%s\n' "# Mixed weight" "" "Regular body." "" "**Bold body at the bold slot.**" >"$MD"

OUT_A="$ART/mixed-a.pdf"
OUT_B="$ART/mixed-b.pdf"
OUT_LIGHT="$ART/regular-only.pdf"

run_pdf() {
  local out="$1"
  shift
  SOURCE_DATE_EPOCH=1700000000 "$BIN" "$MD" --no-config --to pdf --out "$out" \
    --pdf-font "body-regular=$VF" "$@"
}

phase "cli-mixed-a"
set +e
run_pdf "$OUT_A" --pdf-font-weight body-regular=400 --pdf-font-weight body-bold=700 \
  >"$ART/run-a.stdout" 2>"$ART/run-a.stderr"
status_a=$?
set -e
check "gk3v.4.e2e.a.exit" "first mixed-weight CLI render exits 0" "$([ "$status_a" -eq 0 ] && echo 1 || echo 0)" || fail=1
magic="$(head -c 5 "$OUT_A" 2>/dev/null || true)"
check "gk3v.4.e2e.a.pdf" "first output is a PDF" "$([ "$magic" = "%PDF-" ] && echo 1 || echo 0)" || fail=1
check "gk3v.4.e2e.a.stdout" "stdout stays empty with --out" "$([ ! -s "$ART/run-a.stdout" ] && echo 1 || echo 0)" || fail=1
check "gk3v.4.e2e.a.phase" "stderr has font phase logs" "$(grep -q 'font_instance\|font_assets' "$ART/run-a.stderr" && echo 1 || echo 0)" || fail=1

phase "cli-mixed-b"
set +e
run_pdf "$OUT_B" --pdf-font-weight body-regular=400 --pdf-font-weight body-bold=700 \
  >"$ART/run-b.stdout" 2>"$ART/run-b.stderr"
status_b=$?
set -e
check "gk3v.4.e2e.b.exit" "second mixed-weight CLI render exits 0" "$([ "$status_b" -eq 0 ] && echo 1 || echo 0)" || fail=1

phase "determinism"
if [ -f "$OUT_A" ] && [ -f "$OUT_B" ] && cmp -s -- "$OUT_A" "$OUT_B"; then
  check "gk3v.4.e2e.det" "two CLI mixed-weight PDFs are byte-identical" 1 || fail=1
else
  check "gk3v.4.e2e.det" "two CLI mixed-weight PDFs are byte-identical" 0 || fail=1
fi

phase "regular-only"
set +e
run_pdf "$OUT_LIGHT" --pdf-font-weight body-regular=400 \
  >"$ART/run-light.stdout" 2>"$ART/run-light.stderr"
status_light=$?
set -e
check "gk3v.4.e2e.light.exit" "regular-only CLI render exits 0" "$([ "$status_light" -eq 0 ] && echo 1 || echo 0)" || fail=1
if [ -f "$OUT_A" ] && [ -f "$OUT_LIGHT" ] && ! cmp -s -- "$OUT_A" "$OUT_LIGHT"; then
  check "gk3v.4.e2e.mixed.diff" "mixed-weight PDF differs from regular-only" 1 || fail=1
else
  check "gk3v.4.e2e.mixed.diff" "mixed-weight PDF differs from regular-only" 0 || fail=1
fi

phase "size-report"
python3 - "$VF" "$OUT_A" "$PWD/fmd-font/fonts/ibm-plex-sans" "$ART/size_report.json" <<'PY'
import json, os, sys
vf, pdf, plex_dir, out = sys.argv[1:5]
regular = os.path.join(plex_dir, "IBMPlexSans-Regular.ttf")
bold = os.path.join(plex_dir, "IBMPlexSans-Bold.ttf")
report = {
    "schema": "fmd-variable-font-size-v1",
    "variable_triangle_fixture_bytes": os.path.getsize(vf),
    "mixed_weight_pdf_bytes": os.path.getsize(pdf) if os.path.isfile(pdf) else None,
    "ibm_plex_sans_regular_bytes": os.path.getsize(regular),
    "ibm_plex_sans_bold_bytes": os.path.getsize(bold),
    "ibm_plex_sans_regular_plus_bold_bytes": os.path.getsize(regular) + os.path.getsize(bold),
    "note": "Synthetic VF size is not a retail-face claim. smif.2 should budget instanced static subsets, not the host VF.",
}
open(out, "w", encoding="utf-8").write(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, sort_keys=True))
PY
check "gk3v.4.e2e.size.json" "size_report.json written" "$([ -s "$ART/size_report.json" ] && echo 1 || echo 0)" || fail=1

phase "hostile-fvar"
HOSTILE="$WORK/hostile.ttf"
python3 - "$VF" "$HOSTILE" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
data = bytearray(open(src, "rb").read())
n = int.from_bytes(data[4:6], "big")
for i in range(n):
    rec = 12 + i * 16
    if data[rec:rec+4] == b"fvar":
        off = int.from_bytes(data[rec+8:rec+12], "big")
        ln = int.from_bytes(data[rec+12:rec+16], "big")
        for j in range(min(ln, 16)):
            data[off + j] ^= 0x5A
        break
open(dst, "wb").write(data)
PY
set +e
SOURCE_DATE_EPOCH=1700000000 "$BIN" --no-config --text '# hi' --to pdf \
  --out "$WORK/hostile.pdf" --pdf-font "body-regular=$HOSTILE" --pdf-font-weight 650 \
  >"$ART/hostile.stdout" 2>"$ART/hostile.stderr"
hostile_status=$?
set -e
# Must not abort (128+signals). Clean error or a still-valid PDF are both fine.
check "gk3v.4.e2e.hostile.no_abort" "hostile fvar does not abort the CLI" \
  "$(awk -v s="$hostile_status" 'BEGIN{print (s<128)?1:0}')" || fail=1

python3 - "$JSONL" "$ART/summary.json" "$fail" <<'PY'
import json, sys
jsonl, out, fail = sys.argv[1], sys.argv[2], int(sys.argv[3])
checks = []
for line in open(jsonl, encoding="utf-8"):
    line = line.strip()
    if not line:
        continue
    rec = json.loads(line)
    if "id" in rec:
        checks.append(rec)
summary = {
    "schema": "fmd-e2e-v1",
    "suite": "variable-font",
    "bead": "gk3v.4",
    "fail": fail,
    "checks": checks,
}
open(out, "w", encoding="utf-8").write(json.dumps(summary, indent=2) + "\n")
PY

if [ "$fail" -ne 0 ]; then
  log "variable-font-e2e: FAILED run=$RUN_ID"
  exit 70
fi
log "variable-font-e2e: PASS run=$RUN_ID"
exit 0
