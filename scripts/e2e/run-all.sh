#!/usr/bin/env bash
# scripts/e2e/run-all.sh — run every e2e suite + the batch throughput e2e (grn.4.7).
#
# The single entry point for the comprehensive, structured-logging e2e tier. It
# runs each suite that drives the real fmd binary (built once and shared via
# FMD_BIN so the suites don't each rebuild), folds in the existing batch-throughput
# e2e, verifies the harness + test-double machinery via their self-tests, and
# writes a combined aggregate.json over every suite's per-step results.
#
# Usage: scripts/e2e/run-all.sh [run-id]
# Exit:  0 all suites ok · 66 env/build · 70 one or more suites failed.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit
REPO_ROOT="$PWD"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

RUN_ID="${1:-run-all}"

# Discover suites before build/cleanup so every derived artifact id can be
# validated before any directory is removed or expensive work starts.
SUITES=(cli-surface render-matrix error-paths)
[ -f scripts/e2e/parity.sh ] && [ "${E2E_RUN_PARITY:-0}" = "1" ] && SUITES+=(parity)
[ -f scripts/e2e/installer.sh ] && [ "${E2E_RUN_INSTALLER:-0}" = "1" ] && SUITES+=(installer)

fmd_validate_run_id "e2e run-all" "$RUN_ID" "base run-id"
fmd_validate_run_id "e2e run-all" "${RUN_ID}-all" "aggregate run-id"
for s in "${SUITES[@]}"; do
  fmd_validate_run_id "e2e run-all" "${RUN_ID}-$s" "suite run-id"
done

ART="tests/artifacts/e2e/${RUN_ID}-all"
rm -rf -- "$ART"; mkdir -p "$ART"
LOG="$ART/run.log"; : >"$LOG"
log() { printf '%s\n' "$*" | tee -a "$LOG"; }

# Build the fmd binary once; the suites honor FMD_BIN and skip their own builds.
log "run-all: building fmd (release) once for all suites"
export FMD_TARGET_DIR="${FMD_TARGET_DIR:-$REPO_ROOT/target/fmd-checks}"
if ! ( CARGO_TARGET_DIR="$FMD_TARGET_DIR" cargo build --release --quiet --bin fmd ); then
  log "run-all: FAILED to build fmd"
  exit 66
fi
export FMD_BIN="$FMD_TARGET_DIR/release/fmd"
[ -x "$FMD_BIN" ] || { log "run-all: fmd binary missing at $FMD_BIN"; exit 66; }

fail=0
SUMMARIES=()
for s in "${SUITES[@]}"; do
  log "=== e2e suite: $s ==="
  if scripts/e2e/"$s".sh "${RUN_ID}-$s" >>"$LOG" 2>&1; then
    log "  $s: ok"
  else
    log "  $s: FAILED"
    fail=1
  fi
  summ="tests/artifacts/e2e/${RUN_ID}-$s/summary.json"
  [ -f "$summ" ] && SUMMARIES+=("$s=$summ")
done

# Fold in the existing batch-throughput e2e (its own structured scenario harness).
# Set E2E_SKIP_BATCH=1 to skip it (it compiles Asupersync); CI does this in the
# e2e job because the dedicated `batch` job already runs batch-throughput.
if [ "${E2E_SKIP_BATCH:-0}" = "1" ]; then
  log "=== batch-throughput e2e: SKIPPED (E2E_SKIP_BATCH=1) ==="
  bt=skipped
else
  log "=== batch-throughput e2e (--self-test) ==="
  if scripts/batch-throughput.sh --self-test >>"$LOG" 2>&1; then
    log "  batch-throughput: ok"
    bt=ok
  else
    log "  batch-throughput: FAILED"
    bt=FAILED; fail=1
  fi
fi

# Machinery self-tests (the harness + the test-double gate verify themselves).
for selftest in "scripts/e2e/lib.sh --self-test" "scripts/check-test-doubles.sh --self-test"; do
  log "=== machinery: $selftest ==="
  if $selftest >>"$LOG" 2>&1; then log "  ok"; else log "  FAILED"; fail=1; fi
done

# Aggregate every suite's summary.json into one report.
log "run-all: aggregating per-suite results"
python3 - "$ART/aggregate.json" "$RUN_ID" "$bt" "$REPO_ROOT" "${SUMMARIES[@]}" <<'PY'
import json, sys, os
out, run_id, bt, root = sys.argv[1:5]
pairs = sys.argv[5:]
suites = []
tot_steps = tot_pass = tot_assert = tot_assert_fail = 0
for p in pairs:
    name, path = p.split("=", 1)
    try:
        d = json.load(open(path))
        t = d["totals"]
    except Exception as e:
        suites.append({"suite": name, "error": str(e)})
        continue
    suites.append({"suite": name, "steps": t["steps"], "steps_passed": t["steps_passed"],
                   "steps_failed": t["steps_failed"], "assertions": t["assertions"],
                   "assertions_failed": t["assertions_failed"]})
    tot_steps += t["steps"]; tot_pass += t["steps_passed"]
    tot_assert += t["assertions"]; tot_assert_fail += t["assertions_failed"]
agg = {
    "schema": "fmd-e2e-aggregate-v1",
    "run_id": run_id,
    "batch_throughput": bt,
    "totals": {"suites": len(suites), "steps": tot_steps, "steps_passed": tot_pass,
               "assertions": tot_assert, "assertions_failed": tot_assert_fail},
    "suites": suites,
}
with open(out, "w") as fh:
    json.dump(agg, fh, indent=2, sort_keys=True)
    fh.write("\n")
print(f"run-all: {tot_pass}/{tot_steps} steps passed across {len(suites)} suites; "
      f"{tot_assert - tot_assert_fail}/{tot_assert} assertions ok; batch-throughput={bt}")
PY

log "run-all: aggregate at $ART/aggregate.json"
if [ "$fail" -ne 0 ]; then
  log "run-all: FAILED — at least one e2e suite or machinery check failed (see $LOG)."
  exit 70
fi
log "run-all: ok — every e2e suite + the batch e2e + machinery self-tests passed."
exit 0
