#!/usr/bin/env bash
# m7fs.2: crash triage — reproduce, delta-debug, promote to the regression
# corpus. The engine crate is not invoked here for the *drill*; that uses an
# injected marker (`!PANIC!`) so the pipeline can be proven without a live
# engine panic. Real nightly crashes are minimized the same way after you
# confirm they still panic under `cargo test --test fuzz_triage`.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

MARKER='!PANIC!'

log_check() {
  printf 'fuzz-triage: check id=%s subject=%s outcome=%s\n' "$1" "$2" "$3" >&2
}

fail_check() {
  log_check "$1" "$2" "FAIL"
  printf 'fuzz-triage: %s\n' "$3" >&2
  exit "${4:-1}"
}

self_test() {
  [[ -f fuzz/README.md ]] || fail_check "m7fs.2.readme" "fuzz/README.md" "missing triage runbook" 66
  log_check "m7fs.2.readme" "fuzz/README.md" "PASS"
  [[ -f tests/fuzz_triage.rs ]] || fail_check "m7fs.2.test" "tests/fuzz_triage.rs" "missing pipeline test" 66
  log_check "m7fs.2.test" "tests/fuzz_triage.rs" "PASS"
  [[ -f tests/fixtures/fuzz-regressions/drill.bin ]] \
    || fail_check "m7fs.2.fixture" "drill.bin" "missing promoted drill fixture" 66
  local got
  got="$(python3 -c 'print(open("tests/fixtures/fuzz-regressions/drill.bin","rb").read().decode())')"
  [[ "$got" == "$MARKER" ]] || fail_check "m7fs.2.fixture" "drill.bin contents" "expected ${MARKER}" 66
  log_check "m7fs.2.fixture" "drill.bin=${MARKER}" "PASS"
  log_check "m7fs.2.self-test" "source shape" "PASS"
}

# Python ddmin matching tests/fuzz_triage.rs so --drill does not need cargo.
python_ddmin() {
  python3 - "$1" "$2" <<'PY'
import sys
from pathlib import Path

marker = b"!PANIC!"
src = Path(sys.argv[1]).read_bytes()
out = Path(sys.argv[2])

def crashes(data: bytes) -> bool:
    return marker in data

def ddmin(data: bytes) -> bytes:
    assert crashes(data)
    cur = data
    changed = True
    while changed and len(cur) > 1:
        changed = False
        chunk = max(len(cur) // 2, 1)
        while chunk > 0:
            i = 0
            while i + chunk <= len(cur):
                cand = cur[:i] + cur[i + chunk :]
                if cand and crashes(cand):
                    cur = cand
                    changed = True
                else:
                    i += chunk
            chunk //= 2
    return cur

min_bytes = ddmin(src)
out.write_bytes(min_bytes)
print(len(src), len(min_bytes), min_bytes.decode("ascii", "replace"))
PY
}

usage() {
  cat <<'EOF'
Usage:
  scripts/fuzz-triage.sh --self-test
  scripts/fuzz-triage.sh --drill [RUN_ID]
  scripts/fuzz-triage.sh --minimize CRASH.bin --out MIN.bin

  --drill     bloated !PANIC! input -> ddmin -> artifact dir + compare fixture
  --minimize  shrink a crashing file with the same marker oracle (drill) or
              copy it next to a note that engine oracles live in tests/fuzz_triage.rs
EOF
}

MODE=""
CRASH=""
OUT=""
RUN_ID=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --self-test) self_test; exit 0 ;;
    --drill) MODE=drill; shift; RUN_ID="${1:-}"; [[ $# -gt 0 && "${1:-}" != --* ]] && shift || true ;;
    --minimize) MODE=minimize; CRASH="${2:?}"; shift 2 ;;
    --out) OUT="${2:?}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *)
      RUN_ID="$1"
      shift
      ;;
  esac
done

if [[ "$MODE" == "minimize" ]]; then
  [[ -n "$OUT" ]] || fail_check "m7fs.2.minimize" "--out" "need --out MIN.bin" 64
  [[ -f "$CRASH" ]] || fail_check "m7fs.2.minimize" "$CRASH" "crash file missing" 66
  python_ddmin "$CRASH" "$OUT"
  log_check "m7fs.2.minimize" "$CRASH -> $OUT" "PASS"
  exit 0
fi

if [[ "$MODE" != "drill" ]]; then
  usage
  exit 64
fi

if [[ -z "$RUN_ID" ]]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-triage-$(git rev-parse --short HEAD 2>/dev/null || echo head)"
fi
fmd_validate_run_id "fuzz-triage" "$RUN_ID"

self_test

ARTIFACT_DIR="tests/artifacts/fuzz/$RUN_ID"
mkdir -p "$ARTIFACT_DIR"

python3 - "$ARTIFACT_DIR/crash.bin" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
p.write_bytes(b"PADDING_HEAD_" + bytes([0xAA] * 64) + b"!PANIC!" + bytes([0xBB] * 64) + b"_PADDING_TAIL")
PY
log_check "m7fs.2.drill.crash" "$ARTIFACT_DIR/crash.bin" "PASS"

python_ddmin "$ARTIFACT_DIR/crash.bin" "$ARTIFACT_DIR/minimal.bin"
got="$(python3 -c 'print(open("'"$ARTIFACT_DIR"'/minimal.bin","rb").read().decode())')"
[[ "$got" == "$MARKER" ]] || fail_check "m7fs.2.drill.min" "minimal.bin" "got $got" 1
log_check "m7fs.2.drill.min" "minimal.bin=${MARKER}" "PASS"

# Promote: fixture must already match. Do not overwrite a hand-curated file
# if a future crash uses the same path; the drill fixture is the marker.
promoted="tests/fixtures/fuzz-regressions/drill.bin"
if ! cmp -s "$ARTIFACT_DIR/minimal.bin" "$promoted"; then
  fail_check "m7fs.2.promote" "$promoted" "fixture drifted from minimizer" 1
fi
log_check "m7fs.2.promote" "$promoted" "PASS"
{
  printf '{\n  "ok": true,\n  "event": "fuzz_triage_drill",\n  "run_id": "%s",\n  "start_bytes": %s,\n  "min_bytes": %s\n}\n' \
    "$RUN_ID" \
    "$(wc -c < "$ARTIFACT_DIR/crash.bin" | tr -d ' ')" \
    "$(wc -c < "$ARTIFACT_DIR/minimal.bin" | tr -d ' ')"
} >"$ARTIFACT_DIR/report.json"
log_check "m7fs.2.drill" "report.json" "PASS"
