#!/usr/bin/env bash
# coverage.sh — line + region + BRANCH coverage for franken_markdown (grn.1.1).
#
# Wraps cargo-llvm-cov to produce the one authoritative coverage measurement that
# every phase-B gap-fill task targets. Unlike a plain `cargo llvm-cov`, this:
#
#   * measures BRANCH coverage too (cargo-llvm-cov --branch; branch was 0/unmeasured
#     before this script existed), not just line+region+function;
#   * merges THREE feature configurations so feature-gated code is not invisible:
#       1. default (cli)      — the whole product surface + every integration test
#       2. --features batch   — batch.rs (Asupersync orchestration), absent from the
#                               default run, so it stops reading as 0% / unmeasured
#       3. --features wasm-bindgen — wasm_abi.rs (the wasm-bindgen ABI adapter), so it
#                               appears in the report instead of being dropped
#   * emits a per-module summary (summary.md + machine-readable summary.json), an
#     lcov file, and a browsable HTML report under tests/artifacts/coverage/<run-id>/,
#     deterministically (no wall-clock in the committed summary payload);
#   * excludes the integration-test sources and the thin bin shims so the per-module
#     numbers reflect production code only (font/hyphenation data are include_bytes!/
#     include_str! binary blobs, never instrumented, so they need no exclusion).
#
# Branch coverage is "unstable" in cargo-llvm-cov and requires a nightly toolchain;
# this repo already builds on nightly, and CI installs it. If --branch is rejected
# the script falls back to line/region/function and records branch as unmeasured so
# the run still produces a usable report instead of failing hard.
#
# Usage:
#   scripts/coverage.sh [run-id]   # full merged run (default+batch+wasm-bindgen)
#   scripts/coverage.sh --quick    # lib-unit-tests only, all reports (fast iteration)
#   scripts/coverage.sh --check [run-id]   # full run + enforce the committed floor (CI)
#   scripts/coverage.sh --update-floor     # full run + ratchet the floor to current
#   scripts/coverage.sh --self-test  # CI: prove the toolchain + machinery works, fast
#   scripts/coverage.sh --help
#
# The ratcheted floor (tests/fixtures/coverage/coverage-floor.txt) mirrors the
# CommonMark conformance floor: --check fails if any of line/region/branch/function
# coverage drops below the committed integer floor, so the number can only go up;
# --update-floor raises the floor to the current measurement and refreshes the
# committed baseline ledger snapshot (tests/artifacts/coverage/baseline.{tsv,md}).
#
# Exit codes (stable, agent-friendly):
#   0  success
#   2  missing prerequisite (cargo-llvm-cov not installed, etc.)
#   3  a coverage pass failed (compile/test error under instrumentation)
#   4  report generation or post-processing failed
#   5  coverage regressed below the committed floor (--check)
set -uo pipefail
cd "$(dirname "$0")/.." || exit
REPO_ROOT="$PWD"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

# ---- argument parsing ------------------------------------------------------
RUN_MODE="full"          # full | quick | self-test | check | update-floor
RUN_ID="local"
FLOOR_FILE="tests/fixtures/coverage/coverage-floor.txt"
case "${1:-}" in
  --help|-h)
    sed -n '2,52p' "$0"
    exit 0
    ;;
  --self-test)    RUN_MODE="self-test" ;;
  --quick)        RUN_MODE="quick"; RUN_ID="${2:-quick}" ;;
  --check)        RUN_MODE="check"; RUN_ID="${2:-check}" ;;
  --update-floor) RUN_MODE="update-floor"; RUN_ID="${2:-update-floor}" ;;
  "")             ;;
  --*)
    echo "coverage: unknown flag '$1' (try --help)" >&2
    exit 2
    ;;
  *) RUN_ID="$1" ;;
esac
fmd_validate_run_id "coverage" "$RUN_ID"

# Instrumented builds must not clobber the normal/check target dir; cargo-llvm-cov
# isolates its own artifacts under target/llvm-cov-target by default, so we leave
# CARGO_TARGET_DIR unset here on purpose.
ART_ROOT="tests/artifacts/coverage"
ART="$ART_ROOT/$RUN_ID"
LOG=""   # set once ART exists

log() { printf '%s\n' "coverage: $*"; [ -n "$LOG" ] && printf '%s\n' "$*" >>"$LOG"; }
die() { printf '%s\n' "coverage: FAILED — $*" >&2; exit "${2:-1}"; }

# ---- prerequisites ---------------------------------------------------------
command -v cargo >/dev/null 2>&1 || die "cargo not found on PATH" 2
if ! cargo llvm-cov --version >/dev/null 2>&1; then
  die "cargo-llvm-cov not installed (cargo install cargo-llvm-cov)" 2
fi

# Probe branch support once: build a one-arg invocation and see if --branch is
# accepted. We test via the help surface to avoid a slow compile just to probe.
BRANCH_FLAG="--branch"
if ! cargo llvm-cov --help 2>&1 | grep -q -- '--branch'; then
  log "WARNING: this cargo-llvm-cov has no --branch; recording branch as unmeasured"
  BRANCH_FLAG=""
fi

# ---- self-test: fast, no full suite ---------------------------------------
if [ "$RUN_MODE" = "self-test" ]; then
  tmp_json="$(mktemp)"
  trap 'rm -f "$tmp_json"' EXIT
  log "self-test: lib-unit branch coverage probe (no full suite)"
  if ! cargo llvm-cov $BRANCH_FLAG --lib --summary-only --json --output-path "$tmp_json" >/dev/null 2>&1; then
    die "self-test: a lib-only instrumented run failed" 3
  fi
  python3 - "$tmp_json" "${BRANCH_FLAG:+1}" <<'PY' || die "self-test: summary JSON malformed or empty" 4
import json, sys
path, want_branch = sys.argv[1], sys.argv[2]
d = json.load(open(path))
data = d.get("data") or []
assert data, "no data section in llvm-cov json"
t = data[0]["totals"]
files = data[0].get("files") or []
assert files, "no per-file entries"
for k in ("lines", "regions", "functions"):
    assert k in t and t[k]["count"] > 0, f"missing/empty {k} totals"
if want_branch:
    assert "branches" in t and t["branches"]["count"] > 0, "branch totals missing/empty (--branch not effective)"
print(f"self-test ok: lines={t['lines']['percent']:.1f}% "
      f"regions={t['regions']['percent']:.1f}% "
      f"branches={t.get('branches', {}).get('percent', float('nan')):.1f}% "
      f"functions={t['functions']['percent']:.1f}% over {len(files)} files")
PY
  log "self-test: PASS"
  exit 0
fi

# ---- full / quick run ------------------------------------------------------
mkdir -p "$ART"
LOG="$ART/run.log"
: >"$LOG"
log "run-id=$RUN_ID mode=$RUN_MODE branch=${BRANCH_FLAG:-off}"

# Production-code-only view: drop integration-test sources and the thin bin shims.
# Leading '/' anchors prevent matching the repo's own '/data/.../' path prefix.
IGNORE='/tests/|/src/bin/|/src/main\.rs$'

run_pass() {
  # run_pass <label> <extra cargo-llvm-cov args...>
  local label="$1"; shift
  log "pass: $label  (cargo llvm-cov --no-report $BRANCH_FLAG $*)"
  if ! cargo llvm-cov --no-report $BRANCH_FLAG "$@" >>"$LOG" 2>&1; then
    die "coverage pass '$label' failed (see $LOG)" 3
  fi
}

log "cleaning prior coverage artifacts"
cargo llvm-cov clean --workspace >>"$LOG" 2>&1 || true

if [ "$RUN_MODE" = "quick" ]; then
  run_pass "default(cli) lib-only" --lib
else
  # Pass 1: default features — unit + every integration test (the bulk of coverage).
  run_pass "default(cli) full"
  # Pass 2: batch — adds batch.rs; scoped to --lib (batch tests are inline lib tests)
  # so we don't re-run the whole integration suite under the heavier Asupersync build.
  run_pass "batch lib" --features batch --lib
  # Pass 3: wasm-bindgen — covers wasm_abi.rs (the bindgen ABI adapter). Its
  # success paths run natively via the wasm_abi_test integration test; the lib
  # unit tests come along too. (Error paths cross the JS boundary and are covered
  # by the real wasm build in scripts/check-wasm-package.sh, not here.)
  run_pass "wasm-bindgen lib+abi" --features wasm-bindgen --lib --test wasm_abi_test
fi

# ---- reports ---------------------------------------------------------------
log "generating lcov"
cargo llvm-cov report --lcov --ignore-filename-regex "$IGNORE" \
  --output-path "$ART/lcov.info" >>"$LOG" 2>&1 || die "lcov report failed" 4

log "generating per-file json summary"
cargo llvm-cov report --json --summary-only --ignore-filename-regex "$IGNORE" \
  --output-path "$ART/coverage-llvm.json" >>"$LOG" 2>&1 || die "json report failed" 4

log "generating html report"
# llvm-cov writes into <output-dir>/html/, so point it at $ART (yields $ART/html/).
cargo llvm-cov report --html --ignore-filename-regex "$IGNORE" \
  --output-dir "$ART" >>"$LOG" 2>&1 || die "html report failed" 4

log "generating text table (per-file)"
cargo llvm-cov report --ignore-filename-regex "$IGNORE" \
  >"$ART/coverage.txt" 2>>"$LOG" || die "text report failed" 4

# ---- per-module summary (machine-readable + human) ------------------------
log "aggregating per-module summary"
python3 - "$ART/coverage-llvm.json" "$ART" "$RUN_ID" "${BRANCH_FLAG:+1}" "$REPO_ROOT" <<'PY' || die "summary aggregation failed" 4
import json, os, sys
src_json, art, run_id, has_branch, repo_root = sys.argv[1:6]
has_branch = bool(has_branch)
d = json.load(open(src_json))
data = d["data"][0]
files = data.get("files") or []
totals = data["totals"]

def relmod(fn):
    # Normalize to a repo-relative src path for stable module keys.
    p = os.path.relpath(fn, repo_root) if os.path.isabs(fn) else fn
    return p

def metrics(summary):
    out = {}
    for k in ("lines", "regions", "functions", "branches"):
        s = summary.get(k)
        if s is None:
            out[k] = None
        else:
            out[k] = {"count": s["count"], "covered": s["covered"],
                      "missed": s["count"] - s["covered"],
                      "percent": round(s["percent"], 2)}
    return out

modules = []
for f in files:
    mod = relmod(f["filename"])
    if not mod.startswith("src/"):
        continue  # defensive: ignore anything outside the crate's own sources
    modules.append({"module": mod, **{"summary": metrics(f["summary"])}})

# Sort worst-first by line coverage (the metric phase-B tasks chase), then by
# missed lines descending so the biggest absolute gaps surface at the top.
def line_pct(m):
    li = m["summary"]["lines"]
    return (li["percent"] if li else 100.0, -(li["missed"] if li else 0))
modules.sort(key=line_pct)

summary = {
    "schema": "fmd-coverage-summary-v1",
    "run_id": run_id,
    "tool": "cargo-llvm-cov",
    "branch_measured": has_branch,
    "feature_passes": ["default(cli)", "batch", "wasm-bindgen"],
    "totals": metrics(totals),
    "modules": modules,
}
with open(f"{art}/summary.json", "w") as fh:
    json.dump(summary, fh, indent=2, sort_keys=True)
    fh.write("\n")

def cell(m):
    return f"{m['percent']:.1f}% ({m['covered']}/{m['count']})" if m else "—"

with open(f"{art}/summary.md", "w") as fh:
    fh.write(f"# Coverage summary — run `{run_id}`\n\n")
    fh.write("Tool: cargo-llvm-cov. Feature passes merged: default(cli), batch, wasm-bindgen. ")
    fh.write(f"Branch coverage measured: {'yes' if has_branch else 'NO (unmeasured)'}\n\n")
    t = summary["totals"]
    fh.write("## Totals\n\n")
    fh.write("| Metric | Coverage |\n|---|---|\n")
    for k in ("lines", "regions", "branches", "functions"):
        fh.write(f"| {k} | {cell(t[k])} |\n")
    fh.write("\n## Per-module (worst line coverage first)\n\n")
    fh.write("| Module | Lines | Regions | Branches | Functions | Missed lines |\n")
    fh.write("|---|---|---|---|---|---:|\n")
    for m in modules:
        s = m["summary"]
        missed = s["lines"]["missed"] if s["lines"] else 0
        fh.write(f"| {m['module']} | {cell(s['lines'])} | {cell(s['regions'])} | "
                 f"{cell(s['branches'])} | {cell(s['functions'])} | {missed} |\n")

t = summary["totals"]
def pct(k):
    return f"{t[k]['percent']:.2f}%" if t[k] else "n/a"
print(f"coverage: TOTALS  lines={pct('lines')}  regions={pct('regions')}  "
      f"branches={pct('branches')}  functions={pct('functions')}")
print(f"coverage: {len(modules)} src modules; worst: " +
      ", ".join(f"{m['module']}={m['summary']['lines']['percent']:.0f}%" for m in modules[:5]))
PY

log "artifacts under $ART/ (summary.json, summary.md, coverage.txt, lcov.info, html/)"

# ---- ratcheted floor: enforce (--check) or ratchet (--update-floor) -------
if [ "$RUN_MODE" = "check" ] || [ "$RUN_MODE" = "update-floor" ]; then
  mkdir -p "$(dirname "$FLOOR_FILE")"
  python3 - "$ART/summary.json" "$FLOOR_FILE" "$RUN_MODE" "$REPO_ROOT" <<'PY'
import json, math, os, sys
summary_path, floor_file, mode, repo_root = sys.argv[1:5]
totals = json.load(open(summary_path))["totals"]
METRICS = ("lines", "regions", "branches", "functions")

def current(metric):
    m = totals.get(metric)
    # A metric with no instrumentable count (e.g. branches in a data-only module
    # at the total level — never happens, but be defensive) is treated as 100%.
    return m["percent"] if m and m["count"] else 100.0

if mode == "update-floor":
    lines = ["# Ratcheted coverage floor (grn.1.3). CI (--check) fails if any metric",
             "# drops below these integers. Raise with: scripts/coverage.sh --update-floor.",
             "# Whole-percent floors absorb cross-platform instrumentation noise while",
             "# still catching real regressions."]
    for k in METRICS:
        lines.append(f"{k}={math.floor(current(k))}")
    open(floor_file, "w").write("\n".join(lines) + "\n")
    print("coverage: floor updated to " +
          ", ".join(f"{k}>={math.floor(current(k))}" for k in METRICS))
    # Refresh the committed baseline ledger snapshot alongside the floor.
    import shutil
    art = os.path.dirname(summary_path)
    shutil.copy(f"{art}/summary.md", "tests/artifacts/coverage/baseline.md") if os.path.exists(f"{art}/summary.md") else None
    sys.exit(0)

# mode == check: read floor, compare, fail on any regression.
floor = {}
if os.path.exists(floor_file):
    for ln in open(floor_file):
        ln = ln.strip()
        if not ln or ln.startswith("#") or "=" not in ln:
            continue
        k, v = ln.split("=", 1)
        try:
            floor[k.strip()] = float(v.strip())
        except ValueError:
            pass
if not floor:
    print(f"coverage: FAILED — no floor found at {floor_file}; seed it with "
          f"scripts/coverage.sh --update-floor", file=sys.stderr)
    sys.exit(5)

regressions = []
for k in METRICS:
    f = floor.get(k)
    if f is None:
        continue
    cur = current(k)
    # Round current down to the same whole-percent granularity as the floor.
    if math.floor(cur + 1e-9) < f:
        regressions.append(f"{k}: {cur:.2f}% < floor {f:.0f}%")

if regressions:
    print("coverage: FAILED — coverage regressed below the committed floor:", file=sys.stderr)
    for r in regressions:
        print(f"  - {r}", file=sys.stderr)
    print("coverage: if this is an intentional drop, justify it and run "
          "scripts/coverage.sh --update-floor.", file=sys.stderr)
    sys.exit(5)

print("coverage: ok — all metrics at/above floor (" +
      ", ".join(f"{k}={current(k):.1f}%>={floor.get(k,0):.0f}%" for k in METRICS if k in floor) + ")")
PY
  rc=$?
  if [ "$rc" != "0" ]; then exit "$rc"; fi
fi

log "ok"
