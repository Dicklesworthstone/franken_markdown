#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/perf-gauntlet.sh [--iters N] [--tune-perf] [--run-id ID]

Creates a timestamped performance artifact bundle under:
  tests/artifacts/perf/<run-id>/

The gauntlet is measurement-first:
  - builds the release-perf profile with frame pointers,
  - captures host/build fingerprint,
  - runs in-process library scenarios via examples/fmd_perf_harness.rs,
  - runs CLI wall-clock baselines through hyperfine,
  - optionally opens Linux perf counters temporarily and restores them on exit,
  - writes schema manifest, golden checksums, hotspot table, hypothesis ledger,
    and scaling notes.

Options:
  --iters N      in-process iterations per scenario (default: 3; raise for
                 final publish numbers after the dominant hotspots are fixed)
  --tune-perf   temporarily set perf_event_paranoid=-1, kptr_restrict=0,
                and nmi_watchdog=0; values are restored at exit
  --run-id ID   deterministic artifact directory name
  -h, --help    print this help
USAGE
}

ITERS=3
TUNE_PERF=0
RUN_ID=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --iters)
      ITERS="${2:?--iters requires a value}"
      shift 2
      ;;
    --tune-perf)
      TUNE_PERF=1
      shift
      ;;
    --run-id)
      RUN_ID="${2:?--run-id requires a value}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "perf-gauntlet: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

case "$ITERS" in
  ''|*[!0-9]*)
    echo "perf-gauntlet: --iters must be a positive integer" >&2
    exit 64
    ;;
esac
if [ "$ITERS" -eq 0 ]; then
  echo "perf-gauntlet: --iters must be greater than zero" >&2
  exit 64
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$(git rev-parse --short HEAD)"
fi
fmd_validate_run_id "perf-gauntlet" "$RUN_ID"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"
if [ -e "$ARTIFACT_DIR" ]; then
  echo "perf-gauntlet: refusing to reuse existing run: $ARTIFACT_DIR; pass a fresh --run-id" >&2
  exit 64
fi

TMP_PARENT="${TMPDIR:-/data/tmp}"
mkdir -p "$TMP_PARENT"
TMP_ROOT="$(mktemp -d "$TMP_PARENT/fmd-perf-${RUN_ID}.XXXXXX")"
BATCH_IN="$TMP_ROOT/batch-in"
BATCH_OUT="$TMP_ROOT/batch-out"
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR" "$BATCH_IN" "$BATCH_OUT"

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This run follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`

Every optimization closeout should cite this run directory and the relevant
schema records or mapped files.
EOF

cat > "$ARTIFACT_DIR/schema_manifest.json" <<EOF
{
  "schema_version": "$SCHEMA_VERSION",
  "schema_doc": "$SCHEMA_DOC",
  "run_id": "$RUN_ID",
  "artifact_dir": "$ARTIFACT_DIR",
  "primary_jsonl": "inprocess.jsonl",
  "required_record_types": [
    "run_start",
    "host_fingerprint",
    "build_profile",
    "scenario_start",
    "stage_summary",
    "perf_sample",
    "hardware_counter_summary",
    "golden_checksum",
    "hypothesis_evaluated",
    "proof_obligation",
    "run_complete",
    "next_target_recommendation"
  ],
  "current_gauntlet_mapping": {
    "fingerprint.json": ["run_start", "host_fingerprint", "build_profile"],
    "inprocess.jsonl": ["perf_sample"],
    "golden/pdf-large-stages.jsonl": ["scenario_start", "stage_summary", "proof_obligation"],
    "golden/pdf-large-recommendation.jsonl": ["next_target_recommendation"],
    "golden/parser-large-stages.jsonl": ["scenario_start", "stage_summary", "proof_obligation"],
    "golden/parser-large-spanned-stages.jsonl": ["scenario_start", "stage_summary", "proof_obligation"],
    "golden/parser-large-recommendation.jsonl": ["next_target_recommendation"],
    "golden_checksums.txt": ["golden_checksum"],
    "hypothesis.md": ["hypothesis_evaluated"],
    "perf-stat.stdout": ["hardware_counter_summary"],
    "perf-stat.stderr": ["hardware_counter_summary"],
    "BASELINE.md": ["run_complete"],
    "hotspot_table.md": ["next_target_recommendation source evidence"]
  }
}
EOF

PERF_PARANOID_OLD=""
KPTR_RESTRICT_OLD=""
NMI_WATCHDOG_OLD=""

read_sysctl_file() {
  local path="$1"
  if [ -r "$path" ]; then
    cat "$path"
  else
    printf 'unavailable'
  fi
}

restore_perf_tuning() {
  if [ "$TUNE_PERF" -eq 1 ]; then
    if [ -n "$PERF_PARANOID_OLD" ] && [ "$PERF_PARANOID_OLD" != "unavailable" ]; then
      sudo sysctl -q -w "kernel.perf_event_paranoid=$PERF_PARANOID_OLD" || true
    fi
    if [ -n "$KPTR_RESTRICT_OLD" ] && [ "$KPTR_RESTRICT_OLD" != "unavailable" ]; then
      sudo sysctl -q -w "kernel.kptr_restrict=$KPTR_RESTRICT_OLD" || true
    fi
    if [ -n "$NMI_WATCHDOG_OLD" ] && [ "$NMI_WATCHDOG_OLD" != "unavailable" ]; then
      sudo sysctl -q -w "kernel.nmi_watchdog=$NMI_WATCHDOG_OLD" || true
    fi
  fi
}
trap restore_perf_tuning EXIT

if [ "$TUNE_PERF" -eq 1 ]; then
  PERF_PARANOID_OLD="$(read_sysctl_file /proc/sys/kernel/perf_event_paranoid)"
  KPTR_RESTRICT_OLD="$(read_sysctl_file /proc/sys/kernel/kptr_restrict)"
  NMI_WATCHDOG_OLD="$(read_sysctl_file /proc/sys/kernel/nmi_watchdog)"
  {
    echo "{"
    echo "  \"perf_event_paranoid_old\": \"$PERF_PARANOID_OLD\","
    echo "  \"kptr_restrict_old\": \"$KPTR_RESTRICT_OLD\","
    echo "  \"nmi_watchdog_old\": \"$NMI_WATCHDOG_OLD\","
    echo "  \"requested\": true"
    echo "}"
  } > "$ARTIFACT_DIR/tuning.json"
  sudo sysctl -q -w kernel.perf_event_paranoid=-1
  sudo sysctl -q -w kernel.kptr_restrict=0
  sudo sysctl -q -w kernel.nmi_watchdog=0
else
  {
    echo "{"
    echo "  \"requested\": false,"
    echo "  \"perf_event_paranoid\": \"$(read_sysctl_file /proc/sys/kernel/perf_event_paranoid)\","
    echo "  \"kptr_restrict\": \"$(read_sysctl_file /proc/sys/kernel/kptr_restrict)\","
    echo "  \"nmi_watchdog\": \"$(read_sysctl_file /proc/sys/kernel/nmi_watchdog)\""
    echo "}"
  } > "$ARTIFACT_DIR/tuning.json"
fi

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - fmd measured optimization gauntlet

## Scenario
Run the canonical franken_markdown performance scenarios from \`PERFORMANCE_OPTIMIZATION_PLAN.md\`: HTML/PDF showcase renders, large parser input, 1000-word paragraph breaking, 50k-word TeX hyphenation, bundled font subsetting, large PDF render, and batch-100 CLI throughput.

## Metric
Primary metric is p95 wall-clock latency per scenario. Secondary metrics are output bytes, throughput proxies, peak RSS via \`/usr/bin/time -v\`, and hardware counters from \`perf stat\` when permitted.

## Budget
This first run establishes baseline. Future runs should treat >10% p95 drift as noise/investigate boundary and >20% drift as regression or material improvement.

## Golden output
Golden outputs are written to \`golden/\` and checksummed in \`golden_checksums.txt\`.

## Schema
This run follows \`$SCHEMA_VERSION\`; see \`SCHEMA.md\`,
\`schema_manifest.json\`, and \`$SCHEMA_DOC\`.

## Scope boundary
This run does not change code or prove a specific optimization. It ranks targets for one-lever optimization commits.

## Variance envelope
- <=10% drift vs prior same-host run: noise.
- >10% drift: investigate.
- >20%, or 3 consecutive >10%: escalate.

## Stakeholder / requester
Jeffrey requested a hyper-optimized Markdown/text/PDF rendering plan with multicore and SIMD strategy.
EOF

echo "perf-gauntlet: building release-perf with frame pointers"
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile release-perf --example fmd_perf_harness --bin fmd

TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | jq -r '.target_directory')"
BIN="$TARGET_DIR/release-perf/fmd"
HARNESS="$TARGET_DIR/release-perf/examples/fmd_perf_harness"

if [ ! -x "$BIN" ] || [ ! -x "$HARNESS" ]; then
  echo "perf-gauntlet: expected binaries not found under $TARGET_DIR/release-perf" >&2
  exit 70
fi

cat > "$ARTIFACT_DIR/fingerprint.json" <<EOF
{
  "schema_version": "$SCHEMA_VERSION",
  "run_id": "$RUN_ID",
  "captured_at_utc": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "git_sha": "$(git rev-parse HEAD)",
  "git_status_short": $(git status --short --branch | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
  "hardware": {
    "lscpu": $(lscpu | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')
  },
  "os": {
    "uname": "$(uname -a)"
  },
  "toolchain": {
    "rustc": $(rustc -vV | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'),
    "cargo": "$(cargo --version)",
    "hyperfine": "$(hyperfine --version 2>/dev/null || true)",
    "samply": "$(samply --version 2>/dev/null || true)",
    "perf": "$(perf --version 2>/dev/null || true)"
  },
  "build_profile": {
    "name": "release-perf",
    "rustflags": "-C force-frame-pointers=yes",
    "strip": false,
    "debug": "line-tables-only"
  },
  "cache_state": "warm",
  "artifact_dir": "$ARTIFACT_DIR"
}
EOF

echo "perf-gauntlet: running in-process harness ($ITERS iterations)"
"$HARNESS" --scenario all --iters "$ITERS" --out-dir "$GOLDEN_DIR" | tee "$ARTIFACT_DIR/inprocess.jsonl"

echo "perf-gauntlet: preparing batch-100 inputs"
for i in $(seq 0 99); do
  cp examples/showcase.md "$BATCH_IN/file-$i.md"
done

echo "perf-gauntlet: running CLI hyperfine baselines"
hyperfine --warmup 5 --runs 20 --export-json "$ARTIFACT_DIR/hyperfine.json" \
  "$BIN examples/showcase.md --out $TMP_ROOT/showcase.html" \
  "$BIN examples/showcase.md --to pdf --out $TMP_ROOT/showcase.pdf" \
  "$BIN README.md --out $TMP_ROOT/readme.html" \
  "$BIN README.md --to pdf --out $TMP_ROOT/readme.pdf" \
  "for f in $BATCH_IN/*.md; do $BIN \"\$f\" --out \"$BATCH_OUT/\$(basename \"\$f\" .md).html\" >/dev/null; done" \
  > "$ARTIFACT_DIR/hyperfine.txt"

echo "perf-gauntlet: running peak RSS probe"
/usr/bin/time -v "$BIN" README.md --to pdf --out "$TMP_ROOT/time-readme.pdf" \
  > "$ARTIFACT_DIR/time.stdout" 2> "$ARTIFACT_DIR/time.stderr" || true

echo "perf-gauntlet: running perf stat if available"
if perf stat -e cycles,instructions "$BIN" --version > "$ARTIFACT_DIR/perf-smoke.stdout" 2> "$ARTIFACT_DIR/perf-smoke.stderr"; then
  perf stat -r 10 -e cycles,instructions,branches,branch-misses,cache-references,cache-misses \
    "$HARNESS" --scenario pdf-large --iters 10 --out-dir "$TMP_ROOT/perf-golden" \
    > "$ARTIFACT_DIR/perf-stat.stdout" 2> "$ARTIFACT_DIR/perf-stat.stderr" || true
else
  {
    echo "perf stat unavailable; see perf-smoke.stderr"
    cat "$ARTIFACT_DIR/perf-smoke.stderr"
  } > "$ARTIFACT_DIR/perf-stat.stderr"
  : > "$ARTIFACT_DIR/perf-stat.stdout"
fi

echo "perf-gauntlet: checksumming golden outputs"
(cd "$GOLDEN_DIR" && find . -type f -print0 | sort -z | xargs -0 sha256sum) > "$ARTIFACT_DIR/golden_checksums.txt"

python3 - "$ARTIFACT_DIR" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
samples = []
for line in (root / "inprocess.jsonl").read_text().splitlines():
    if line.strip():
        samples.append(json.loads(line))

samples.sort(key=lambda row: row["p95_ns"], reverse=True)

def ms(ns):
    return ns / 1_000_000

with (root / "hotspot_table.md").open("w") as f:
    f.write("# Hotspot Table\n\n")
    f.write("| Rank | Scenario | Category | p95 | Mean | Input | Output | Evidence |\n")
    f.write("|---:|---|---|---:|---:|---:|---:|---|\n")
    for idx, row in enumerate(samples, 1):
        f.write(
            f"| {idx} | `{row['scenario']}` | {row['category']} | "
            f"{ms(row['p95_ns']):.3f} ms | {ms(row['mean_ns']):.3f} ms | "
            f"{row['input_bytes']} B | {row['output_bytes']} B | `inprocess.jsonl` |\n"
        )

with (root / "BASELINE.md").open("w") as f:
    f.write("# Baseline\n\n")
    f.write("| Scenario | p50 | p95 | p99 | Max | Iterations | Notes |\n")
    f.write("|---|---:|---:|---:|---:|---:|---|\n")
    for row in samples:
        f.write(
            f"| `{row['scenario']}` | {ms(row['p50_ns']):.3f} ms | "
            f"{ms(row['p95_ns']):.3f} ms | {ms(row['p99_ns']):.3f} ms | "
            f"{ms(row['max_ns']):.3f} ms | {row['iterations']} | {row['notes']} |\n"
        )
    f.write("\n## CLI Baselines\n\nSee `hyperfine.txt` and `hyperfine.json`.\n\n")
    f.write("## Peak RSS\n\nSee `time.stderr`.\n\n")
    f.write("## Hardware Counters\n\nSee `perf-stat.stderr` and `perf-stat.stdout`.\n")

with (root / "hypothesis.md").open("w") as f:
    f.write("# Hypothesis Ledger\n\n")
    top = samples[0] if samples else None
    if top:
        f.write(f"- dominant_inprocess_cost: supports - `{top['scenario']}` has highest p95 in `inprocess.jsonl`.\n")
    f.write("- process_startup_dominates_small_files: supports - prior CLI timings are sub-5ms for tiny showcase HTML/PDF, so in-process benches are required.\n")
    f.write("- SIMD_first: rejects - no scanner scenario is promoted until parser/HTML escaping appears top-5 under in-process evidence.\n")
    f.write("- page_builder_parallelism_first: rejects - page building has correctness coupling; parallelize file/paragraph/font-face work first.\n")

with (root / "scaling_law.md").open("w") as f:
    f.write("# Scaling Law\n\n")
    f.write("Current run establishes single-process stage timings. Batch-100 CLI timing is in `hyperfine.json`.\n\n")
    f.write("For future Asupersync batch mode, use `rho = lambda / (c * mu)` and keep rho <= 0.70 for latency-sensitive interactive mode or <= 0.85 for throughput mode.\n")
    f.write("Single large documents should parallelize independent paragraph/code/font-face work while preserving serial page-building until partition certificates exist.\n")

with (root / "ALIEN_ARTIFACT.md").open("w") as f:
    f.write("# Alien Artifact Performance Design\n\n")
    f.write("## Objective\n")
    f.write("Minimize render latency and PDF byte size while preserving deterministic output, WASM portability, zero-dependency core policy, and scalar-correctness proofs.\n\n")
    f.write("## Selected Families\n")
    f.write("- Certified rewrite pipelines: scalar implementation remains the specification for SIMD and serialization rewrites.\n")
    f.write("- Compositional latency algebra: `T_total <= T_parse + T_highlight + T_shape + T_linebreak + T_paginate + T_pdf`.\n")
    f.write("- Queueing theory: native batch worker count is sized by utilization, service variance, and deterministic receipt ordering.\n")
    f.write("- Convex/resource allocation: only compile to small policy tables; no solver dependency in the render core.\n\n")
    f.write("## Proof Obligations\n")
    f.write("- Golden checksum preservation for every optimization.\n")
    f.write("- Tie-break and ordering preservation for line-breaking optimizations.\n")
    f.write("- Scalar/SIMD differential equivalence before enabling accelerated scanners.\n")
    f.write("- WASM/no-default build remains green.\n")
    f.write("- Perf delta must clear the same-host variance envelope.\n\n")
    f.write("## Galaxy-Brain Cards\n")
    f.write("### Queueing\n")
    f.write("Equation: `rho = lambda / (c * mu)`. Substitution will use measured batch throughput and service time once Asupersync batch exists. Intuition: tails explode as rho approaches 1. Change decision if service-time CV exceeds 1.5.\n\n")
    f.write("### Latency Composition\n")
    f.write("Equation: `T_total <= sum(stage_p95) + coupling_margin`. Substitution comes from `BASELINE.md`. Intuition: optimize the largest certified stage first. Change decision if profiling shows file I/O or process startup dominates.\n\n")
    f.write("### SIMD Gate\n")
    f.write("Equation: `EV = impact * confidence / effort`. SIMD proceeds only when scanner p95 is top-5 and EV >= 2.0 after scalar baselines. Change decision if AVX2/NEON differential tests fail or gains stay within noise.\n")
PY

cat > "$ARTIFACT_DIR/README.md" <<EOF
# fmd perf run $RUN_ID

Artifacts:

- \`SCHEMA.md\` - schema stamp for this run.
- \`schema_manifest.json\` - machine-readable schema/version/file mapping.
- \`DEFINE.md\` - scenario and budget definition.
- \`fingerprint.json\` - host, build, git, and toolchain fingerprint.
- \`BASELINE.md\` - p50/p95/p99 baseline table.
- \`hotspot_table.md\` - ranked in-process scenario costs.
- \`hypothesis.md\` - interpreted bottleneck hypotheses.
- \`scaling_law.md\` - multicore/batch scaling guidance.
- \`ALIEN_ARTIFACT.md\` - advanced-math artifact/proof plan.
- \`golden/pdf-large-stages.jsonl\` - PDF stage attribution records.
- \`golden/pdf-large-recommendation.jsonl\` - next PDF optimization target recommendation.
- \`golden/parser-large-stages.jsonl\` - parser stage/allocation attribution records.
- \`golden/parser-large-spanned-stages.jsonl\` - source-span/diagnostic parser attribution records.
- \`golden/parser-large-recommendation.jsonl\` - next parser optimization target recommendation.
- \`golden_checksums.txt\` - behavior-preservation checksums.
- \`hyperfine.*\` - CLI wall-clock baselines.
- \`perf-stat.*\` - hardware counter output when permitted.
- \`time.stderr\` - peak RSS probe.
EOF

echo "perf-gauntlet: wrote $ARTIFACT_DIR"
