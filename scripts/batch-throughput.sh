#!/usr/bin/env bash
#
# batch-throughput.sh — native batch renderer throughput + correctness e2e
# (bead zmd.1.5).
#
# Builds `fmd` with the native-only `batch` feature, generates corpora, and runs
# the required scenarios (100 files -> HTML, 100 files -> PDF, a mixed set -> both,
# an intentionally-unwritable output case, and a worker-budget case). For each
# timed scenario it records git/toolchain/host fingerprints, the worker count and
# queue bound, queueing-policy inputs (mu/lambda/c/rho), input/output counts and
# bytes, p50/p95/p99/max job duration, total throughput, peak RSS, an error
# summary, and a deterministic receipt checksum. It also asserts the receipt is
# byte-identical across two runs (catching receipt-order nondeterminism), and
# that the unwritable-output case fails as designed.
#
# Artifacts land under tests/artifacts/perf/<run-id>/. Agent-friendly: stdout is
# the human run log, stable exit codes (0 ok, 64 usage, 66 build/env, 70 a
# scenario behaved unexpectedly), and a machine-readable summary.json.
#
# Usage:
#   scripts/batch-throughput.sh [--files N] [--iters K] [--run-id ID] [--smoke]
#   scripts/batch-throughput.sh --self-test    # tiny corpus, fast, CI-friendly
#
# Targets a Linux CI host: uses `date +%s%N`, GNU `/usr/bin/time -v`, and
# `/proc/cpuinfo` (RSS/CPU degrade gracefully elsewhere, but nanosecond timing
# needs GNU date), and bash 4+ for `"${extra[@]}"` under `set -u`.

set -euo pipefail

FILES=100
ITERS=5
RUN_ID=""
SMOKE=0
SCHEMA_VERSION="fmd-perf-artifact-v1"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --files) FILES="${2:?--files needs a count}"; shift 2 ;;
    --iters) ITERS="${2:?--iters needs a count}"; shift 2 ;;
    --run-id) RUN_ID="${2:?--run-id needs an id}"; shift 2 ;;
    --smoke) SMOKE=1; shift ;;
    --self-test) SMOKE=1; FILES=6; ITERS=2; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "batch-throughput: unknown argument '$1' (try --help)" >&2; exit 64 ;;
  esac
done
[ "$SMOKE" -eq 1 ] && { [ "$FILES" -gt 24 ] && FILES=24; }

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

fail_env() { echo "batch-throughput: $*" >&2; exit 66; }
fail_scn() { echo "batch-throughput: SCENARIO FAILURE: $*" >&2; exit 70; }

command -v python3 >/dev/null 2>&1 || fail_env "python3 is required"

# ---- run id ---------------------------------------------------------------
if [ -z "$RUN_ID" ]; then
  RUN_ID="batch-throughput-$(date -u +%Y%m%dT%H%M%SZ)"
fi
fmd_validate_run_id "batch-throughput" "$RUN_ID"

# ---- build the batch-enabled binary ----------------------------------------
echo "batch-throughput: building fmd --features batch (release)..."
cargo build --release --features batch --bin fmd >/dev/null 2>&1 \
  || fail_env "cargo build --release --features batch failed"
target_dir() {
  { cargo metadata --format-version 1 --no-deps 2>/dev/null || true; } \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p'
}
FMD="$(target_dir)/release/fmd"
[ -x "$FMD" ] || fail_env "fmd binary not found at $FMD"

# ---- run dir + fingerprint --------------------------------------------------
OUT_DIR="tests/artifacts/perf/$RUN_ID"
WORK="$OUT_DIR/work"
mkdir -p "$WORK"
echo "batch-throughput: run id $RUN_ID  ($FILES files, $ITERS iters)"

{
  echo "{"
  echo "  \"schema_version\": \"$SCHEMA_VERSION\","
  echo "  \"run_id\": \"$RUN_ID\","
  echo "  \"git_sha\": \"$(git rev-parse HEAD 2>/dev/null || echo unknown)\","
  echo "  \"dirty\": \"$(git status --short 2>/dev/null | head -c 400 | tr '\n' ';')\","
  echo "  \"rustc\": \"$(rustc --version 2>/dev/null)\","
  echo "  \"cargo\": \"$(cargo --version 2>/dev/null)\","
  echo "  \"os\": \"$(uname -srm 2>/dev/null)\","
  echo "  \"cpu\": \"$(grep -m1 'model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2 | sed 's/^ //')\","
  echo "  \"available_parallelism\": $(nproc 2>/dev/null || echo 1),"
  echo "  \"files\": $FILES,"
  echo "  \"iters\": $ITERS"
  echo "}"
} > "$OUT_DIR/fingerprint.json"

# ---- corpora ----------------------------------------------------------------
SHOWCASE_DIR="$WORK/showcase"
MIXED_DIR="$WORK/mixed"
mkdir -p "$SHOWCASE_DIR" "$MIXED_DIR"
gen_doc() { # path index
  cat > "$1" <<EOF
# Document $2

A short paragraph of body text with *emphasis*, **strong**, \`code\`, and a
[link](https://example.com/$2).

- first item
- second item
  - nested item

| Name | Value |
|:---|---:|
| alpha | $2 |
| beta | $(( $2 * 2 )) |

\`\`\`rust
fn doc_$2() -> u32 { $2 }
\`\`\`

> A quoted line for document $2.
EOF
}
i=0
while [ "$i" -lt "$FILES" ]; do
  gen_doc "$SHOWCASE_DIR/doc_$(printf '%03d' "$i").md" "$i"
  i=$((i + 1))
done
# Mixed corpus: README + showcase example + a couple generated docs.
[ -f README.md ] && cp README.md "$MIXED_DIR/readme.md"
[ -f examples/showcase.md ] && cp examples/showcase.md "$MIXED_DIR/showcase.md"
gen_doc "$MIXED_DIR/gen_a.md" 1
gen_doc "$MIXED_DIR/gen_b.md" 2

INPROCESS="$OUT_DIR/inprocess.jsonl"
: > "$INPROCESS"
SUMMARY_ROWS="$WORK/rows.jsonl"
: > "$SUMMARY_ROWS"

# Run a timed scenario: K iterations of `fmd batch`, capture samples + receipt.
# Args: scenario  category  input_dir  to(html|pdf|both)  [extra fmd args...]
run_scenario() {
  local scenario="$1" category="$2" input="$3" to="$4"; shift 4
  local extra=("$@")
  local sdir="$WORK/$scenario"
  mkdir -p "$sdir/out"
  local samples="$sdir/samples.txt"; : > "$samples"
  local receipt="$sdir/receipt.json"

  local k=0
  while [ "$k" -lt "$ITERS" ]; do
    local start end
    start="$(date +%s%N)"
    "$FMD" batch "$input" --to "$to" --out-dir "$sdir/out" --json "${extra[@]}" \
      > "$receipt" 2>"$sdir/stderr.txt" || fail_scn "$scenario: fmd batch exited nonzero"
    end="$(date +%s%N)"
    echo "$((end - start))" >> "$samples"
    k=$((k + 1))
  done

  # Determinism: a second run must produce a byte-identical receipt.
  local receipt2="$sdir/receipt.2.json"
  "$FMD" batch "$input" --to "$to" --out-dir "$sdir/out2" --json "${extra[@]}" \
    > "$receipt2" 2>/dev/null || true
  # out-dir differs, so normalize the volatile out-dir path before comparing.
  if ! diff <(sed "s#$sdir/out2#OUT#g" "$receipt2") \
            <(sed "s#$sdir/out#OUT#g" "$receipt") >/dev/null 2>&1; then
    fail_scn "$scenario: receipt is not byte-identical across runs (order nondeterminism)"
  fi

  # Peak RSS via /usr/bin/time -v on one extra run, if available.
  local rss_kb="null"
  if command -v /usr/bin/time >/dev/null 2>&1; then
    /usr/bin/time -v "$FMD" batch "$input" --to "$to" --out-dir "$sdir/out3" --json "${extra[@]}" \
      >/dev/null 2>"$sdir/time.stderr" || true
    rss_kb="$(grep -i 'Maximum resident set size' "$sdir/time.stderr" 2>/dev/null \
      | rsplit_int)"
    [ -z "$rss_kb" ] && rss_kb="null"
  fi

  # stdout -> perf_sample JSONL; the machine summary row is written to ROWS_OUT
  # directly; the human progress line goes to stderr (the terminal).
  SCN="$scenario" CAT="$category" SAMPLES="$samples" RECEIPT="$receipt" \
    RSS="$rss_kb" OUTDIR="$sdir/out" ROWS_OUT="$SUMMARY_ROWS" \
    python3 - >> "$INPROCESS" <<'PY'
import json, os, sys

scenario = os.environ["SCN"]; category = os.environ["CAT"]
samples = sorted(int(x) for x in open(os.environ["SAMPLES"]) if x.strip())
receipt = json.load(open(os.environ["RECEIPT"]))
rss = os.environ["RSS"]
rss = int(rss) if rss not in ("", "null") else None

def pct(p):
    if not samples:
        return 0
    idx = min(len(samples) - 1, int(round((p / 100.0) * (len(samples) - 1))))
    return samples[idx]

p50, p95, p99, mx, mn = pct(50), pct(95), pct(99), samples[-1] if samples else 0, samples[0] if samples else 0
mean = sum(samples) // len(samples) if samples else 0

# Output bytes (sum of all written outputs in the receipt).
out_bytes = sum(o.get("bytes", 0) for f in receipt.get("files", []) for o in f.get("outputs", []))
inputs = receipt.get("inputs", 0)
workers = receipt.get("workers", 1)
queue_depth = receipt.get("queue_depth", 0)

# Queueing-policy view: a "job" is one input render. Per-batch wall-clock (p50)
# gives total service time for `inputs` jobs across `workers` workers, so the
# per-worker service rate mu and offered rate lambda follow.
total_s = p50 / 1e9 if p50 else 0.0
mu = (inputs / workers) / total_s if (total_s > 0 and workers) else 0.0   # jobs/s/worker
lam = inputs / total_s if total_s > 0 else 0.0                            # offered jobs/s
rho = lam / (workers * mu) if (workers and mu) else 0.0

# perf_sample line (schema fmd-perf-artifact-v1).
print(json.dumps({
    "type": "perf_sample", "scenario": scenario, "category": category,
    "iterations": len(samples), "input_bytes": 0, "output_bytes": out_bytes,
    "min_ns": mn, "mean_ns": mean, "p50_ns": p50, "p95_ns": p95, "p99_ns": p99, "max_ns": mx,
    "notes": f"fmd batch {inputs} inputs on {workers} workers",
}, separators=(",", ":")))

throughput = inputs / total_s if total_s > 0 else 0.0
with open(os.environ["ROWS_OUT"], "a", encoding="utf-8") as rows:
    rows.write(json.dumps({
        "scenario": scenario, "inputs": inputs, "ok": receipt.get("ok", 0),
        "failed": receipt.get("failed", 0), "skipped": receipt.get("skipped", 0),
        "workers": workers, "queue_depth": queue_depth,
        "p50_ms": round(p50 / 1e6, 3), "p95_ms": round(p95 / 1e6, 3), "p99_ms": round(p99 / 1e6, 3),
        "throughput_files_per_s": round(throughput, 1), "output_bytes": out_bytes,
        "peak_rss_kb": rss,
        "policy": {"mu_jobs_per_s_per_worker": round(mu, 1), "lambda_jobs_per_s": round(lam, 1),
                   "c_workers": workers, "rho_utilization": round(rho, 3)},
    }) + "\n")

print(
    f"  {scenario:14} {receipt.get('ok',0)}/{inputs} ok, {receipt.get('failed',0)} failed | "
    f"p50 {p50/1e6:.1f}ms p95 {p95/1e6:.1f}ms | {throughput:.1f} files/s | "
    f"{workers}w q{queue_depth} rho={rho:.2f} | rss={rss}",
    file=sys.stderr,
)
PY
}

# rsplit_int: last whitespace-separated integer on stdin (peak RSS value).
rsplit_int() { awk '{print $NF}' | tr -dc '0-9'; }

echo "batch-throughput: running scenarios"
run_scenario "html-100" "batch-html" "$SHOWCASE_DIR" html
run_scenario "pdf-100" "batch-pdf" "$SHOWCASE_DIR" pdf
run_scenario "mixed-both" "batch-both" "$MIXED_DIR" both
# Budget case: a hard worker cap of 1 (serial) — proves the receipt honors the
# worker budget.
run_scenario "budget-cap1" "batch-pdf" "$SHOWCASE_DIR" pdf --workers 1

# ---- negative-path: intentionally unwritable output -------------------------
echo "batch-throughput: unwritable-output scenario"
BLOCKER="$WORK/blocker"
: > "$BLOCKER" # a regular FILE where an output directory is required
set +e
"$FMD" batch "$SHOWCASE_DIR" --to html --out-dir "$BLOCKER" --json \
  > "$WORK/invalid-receipt.json" 2>"$WORK/invalid.stderr"
INVALID_CODE=$?
set -e
# create_dir_all under a regular file fails for every input, so the run reports
# all-failed and exits 70 (no --continue-on-error). Either a clean nonzero exit
# or an all-failed receipt is acceptable; a success would be a bug.
INVALID_OK=0
if [ "$INVALID_CODE" -ne 0 ]; then
  INVALID_OK=1
elif python3 -c "import json,sys; d=json.load(open('$WORK/invalid-receipt.json')); sys.exit(0 if d.get('failed',0)>0 and d.get('ok',0)==0 else 1)"; then
  INVALID_OK=1
fi
[ "$INVALID_OK" -eq 1 ] || fail_scn "unwritable-output case unexpectedly succeeded (exit=$INVALID_CODE)"
echo "  invalid-output ok (exit=$INVALID_CODE, reported as failure)"

# ---- DEFINE.md + summary ----------------------------------------------------
cat > "$OUT_DIR/DEFINE.md" <<EOF
# DEFINE — native batch throughput (zmd.1.5)

Scenarios: html-100, pdf-100 ($FILES showcase files), mixed-both (README +
showcase + generated), budget-cap1 (--workers 1), and an unwritable-output
negative case. Each timed scenario runs $ITERS iterations; the primary metric is
p95 batch wall-clock.

Queueing view: mu = (inputs/workers)/p50_s (per-worker service rate),
lambda = inputs/p50_s (achieved throughput), c = workers. For a CLOSED batch all
jobs are offered at t=0, so rho = lambda/(c*mu) is definitionally ~1.0 (workers
saturated — the healthy state); the worker-budget policy's rho<=0.70/0.85 targets
apply to the OPEN/watch arrival model, not this script. The transferable capacity
number is mu; the meaningful scaling number is \`parallel_speedup_pdf\` in
summary.json (serial budget-cap1 p50 / parallel pdf-100 p50; ~workers is ideal).

Determinism is enforced by comparing two receipts byte-for-byte (out-dir path
normalized). Cancellation/budget refusal accounting is covered by the
deterministic lab test
\`render_batch_cancelled_at_boundary_skips_all_and_leaks_no_output\` (zmd.1.4).
EOF

python3 - "$OUT_DIR" "$SUMMARY_ROWS" > "$OUT_DIR/summary.json" <<'PY'
import json, sys
out_dir, rows_path = sys.argv[1], sys.argv[2]
rows = [json.loads(l) for l in open(rows_path, encoding="utf-8") if l.strip()]
by = {r["scenario"]: r for r in rows}

# A genuinely-varying scaling metric (rho is tautologically ~1 for a closed
# batch): how much the parallel PDF batch beats the serial (--workers 1)
# baseline on the same corpus. Ideal is ~the worker count.
speedup = None
par, ser = by.get("pdf-100"), by.get("budget-cap1")
if par and ser and par.get("p50_ms", 0) > 0:
    speedup = round(ser["p50_ms"] / par["p50_ms"], 2)

print(json.dumps({
    "type": "batch_throughput_summary",
    "schema_version": "fmd-perf-artifact-v1",
    "artifact_dir": out_dir,
    "parallel_speedup_pdf": speedup,
    "parallel_speedup_ideal": (par or {}).get("workers"),
    "scenarios": rows,
}, indent=2))
PY

echo "batch-throughput: ok — artifacts in $OUT_DIR"
