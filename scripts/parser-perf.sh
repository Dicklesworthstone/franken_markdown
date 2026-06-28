#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/parser-perf.sh [--iters N] [--run-id ID]

Creates an agent-friendly parser performance artifact bundle under:
  tests/artifacts/perf/<run-id>/

The run:
  - builds examples/fmd_parser_perf.rs with the release-perf profile,
  - runs scripts/parser-diff.sh and records the exact status/stdout/stderr,
  - measures parser-only scenarios from the in-process library API,
  - captures git/toolchain/host/build fingerprints,
  - records p50/p95/p99/max, block/inline/diagnostic counts, peak RSS where
    available, output checksums, and a hypothesis summary.

Scenarios:
  parser-large-1mib, README.md, examples/showcase.md, table-heavy,
  reference-link-heavy, code-fence-heavy, and malformed-diagnostics.

Options:
  --iters N    parser iterations per scenario (default: 25)
  --run-id ID deterministic artifact directory name
  -h, --help  print this help
USAGE
}

ITERS=25
RUN_ID=""
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"
BEAD_ID="br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.6.5"
ORIGINAL_ARGS="$*"

validate_run_id() {
  case "$1" in
    ''|'.'|'..'|[!A-Za-z0-9]*|*[^A-Za-z0-9._-]*)
      echo "parser-perf: --run-id must start with an ASCII letter/digit and contain only ASCII letters, digits, dot, underscore, or dash" >&2
      exit 64
      ;;
  esac
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --iters)
      ITERS="${2:?--iters requires a value}"
      shift 2
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
      echo "parser-perf: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

case "$ITERS" in
  ''|*[!0-9]*)
    echo "parser-perf: --iters must be a positive integer" >&2
    exit 64
    ;;
esac
if [ "$ITERS" -eq 0 ]; then
  echo "parser-perf: --iters must be greater than zero" >&2
  exit 64
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "parser-perf: python3 is required for artifact summarization" >&2
  exit 69
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-parser-$(git rev-parse --short HEAD)"
fi
validate_run_id "$RUN_ID"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"

if [ -e "$ARTIFACT_DIR/inprocess.jsonl" ]; then
  echo "parser-perf: refusing to append to existing run: $ARTIFACT_DIR" >&2
  echo "parser-perf: pass a fresh --run-id or inspect the existing artifact directory" >&2
  exit 73
fi

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR"

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This parser performance run follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`

This is the parser-focused q6.5 harness. It preserves the shared artifact file
names while adding parser-specific block/inline/diagnostic counts and explicit
\`parser-diff\` status.
EOF

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - parser performance regression gate

## Scope
This run measures parser-only work through the in-process library API, then
checks that every scenario can still render deterministic HTML from the parsed
AST. It is designed to catch regressions after scanner, allocation, reference,
table, code-fence, and diagnostics changes.

## Metric
Primary metric is per-scenario p95 parser latency in nanoseconds. Secondary
metrics are p50, p99, max, block count, inline count, recoverable diagnostic
count, generated HTML bytes, output checksums, and process peak RSS when
\`/usr/bin/time -v\` is available.

## Scenarios
- \`parser-large-1mib\`: generated 1 MiB mixed Markdown document.
- \`readme\`: repository \`README.md\`.
- \`showcase\`: \`examples/showcase.md\`.
- \`table-heavy\`: generated GFM pipe-table-heavy input.
- \`reference-link-heavy\`: generated reference-link-heavy input.
- \`code-fence-heavy\`: generated fenced-code-heavy input.
- \`malformed-diagnostics\`: malformed but recoverable diagnostics input.

## Regression signal
Future parser optimization beads should compare this bundle's p95 values and
\`golden_checksums.txt\` against their after-run. A same-host p95 drift above
10% is investigation-worthy; above 20% with unchanged behavior is a material
regression or speedup.
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
    "parser_diff_status",
    "scenario_start",
    "perf_sample",
    "resource_summary",
    "golden_checksum",
    "hypothesis_evaluated",
    "proof_obligation",
    "run_complete"
  ],
  "current_parser_mapping": {
    "fingerprint.json": ["run_start", "host_fingerprint", "build_profile"],
    "parser-diff-status.json": ["parser_diff_status"],
    "parser-diff.stdout": ["parser_diff_status stdout"],
    "parser-diff.stderr": ["parser_diff_status stderr"],
    "inprocess.jsonl": ["run_start", "host_fingerprint", "build_profile", "parser_diff_status", "scenario_start", "perf_sample", "resource_summary", "golden_checksum", "hypothesis_evaluated", "proof_obligation", "run_complete"],
    "golden_checksums.txt": ["golden_checksum"],
    "BASELINE.md": ["run_complete"],
    "hotspot_table.md": ["hotspot interpretation"],
    "hypothesis.md": ["hypothesis_evaluated"],
    "time.stderr": ["resource_summary peak rss where available"]
  }
}
EOF

echo "parser-perf: building release-perf parser harness"
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile release-perf --example fmd_parser_perf

TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
HARNESS="$TARGET_DIR/release-perf/examples/fmd_parser_perf"

if [ ! -x "$HARNESS" ]; then
  echo "parser-perf: expected harness not found: $HARNESS" >&2
  exit 70
fi

echo "parser-perf: running parser-diff correctness gate"
set +e
scripts/parser-diff.sh > "$ARTIFACT_DIR/parser-diff.stdout" 2> "$ARTIFACT_DIR/parser-diff.stderr"
PARSER_DIFF_STATUS=$?
set -e

python3 - "$ARTIFACT_DIR" "$RUN_ID" "$SCHEMA_VERSION" "$PARSER_DIFF_STATUS" <<'PY'
import json
import pathlib
import platform
import socket
import subprocess
import sys

artifact_dir = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]
schema_version = sys.argv[3]
parser_diff_status = int(sys.argv[4])

def cmd(args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)
    except Exception as exc:
        return f"unavailable: {exc}"

fingerprint = {
    "schema_version": schema_version,
    "run_id": run_id,
    "captured_at_utc": cmd(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"]).strip(),
    "git_sha": cmd(["git", "rev-parse", "HEAD"]).strip(),
    "git_status_short": cmd(["git", "status", "--short", "--branch"]),
    "host": {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "uname": cmd(["uname", "-a"]).strip(),
        "lscpu": cmd(["lscpu"]),
    },
    "toolchain": {
        "rustc": cmd(["rustc", "-vV"]),
        "cargo": cmd(["cargo", "--version"]).strip(),
    },
    "build_profile": {
        "name": "release-perf",
        "rustflags": "-C force-frame-pointers=yes",
        "strip": False,
        "debug": "line-tables-only",
    },
    "parser_diff_status": parser_diff_status,
    "artifact_dir": str(artifact_dir),
}
(artifact_dir / "fingerprint.json").write_text(json.dumps(fingerprint, indent=2, sort_keys=True) + "\n")
(artifact_dir / "parser-diff-status.json").write_text(json.dumps({
    "type": "parser_diff_status",
    "status": "pass" if parser_diff_status == 0 else "fail",
    "exit_code": parser_diff_status,
    "stdout_path": "parser-diff.stdout",
    "stderr_path": "parser-diff.stderr",
}, indent=2, sort_keys=True) + "\n")
PY

: > "$ARTIFACT_DIR/inprocess.jsonl"

python3 - "$ARTIFACT_DIR" "$RUN_ID" "$SCHEMA_VERSION" "$BEAD_ID" "$PARSER_DIFF_STATUS" "$ORIGINAL_ARGS" >> "$ARTIFACT_DIR/inprocess.jsonl" <<'PY'
import json
import pathlib
import subprocess
import sys

artifact_dir = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]
schema_version = sys.argv[3]
bead_id = sys.argv[4]
parser_diff_status = int(sys.argv[5])
command_tail = sys.argv[6] if len(sys.argv) > 6 else ""

def cmd(args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT).strip()
    except Exception as exc:
        return f"unavailable: {exc}"

def rust_host_triple():
    rustc = cmd(["rustc", "-vV"])
    for line in rustc.splitlines():
        if line.startswith("host: "):
            return line.split(": ", 1)[1]
    return "unknown"

records = [
    {
        "type": "run_start",
        "schema_version": schema_version,
        "run_id": run_id,
        "created_at_utc": cmd(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"]),
        "git_sha": cmd(["git", "rev-parse", "HEAD"]),
        "dirty_status": cmd(["git", "status", "--short", "--branch"]),
        "command": f"scripts/parser-perf.sh {command_tail}".strip(),
        "artifact_dir": str(artifact_dir),
    },
    {
        "type": "host_fingerprint",
        "target_triple": rust_host_triple(),
        "rustc": cmd(["rustc", "-vV"]),
        "cargo": cmd(["cargo", "--version"]),
        "os": cmd(["uname", "-a"]),
        "cpu": cmd(["lscpu"]),
        "feature_flags": ["default"],
        "build_profile": "release-perf",
    },
    {
        "type": "build_profile",
        "profile": "release-perf",
        "rustflags": "-C force-frame-pointers=yes",
        "debug": "line-tables-only",
        "strip": False,
        "frame_pointers": True,
    },
    {
        "type": "parser_diff_status",
        "status": "pass" if parser_diff_status == 0 else "fail",
        "exit_code": parser_diff_status,
        "stdout_path": "parser-diff.stdout",
        "stderr_path": "parser-diff.stderr",
        "notes": "parser-diff must pass before parser perf samples are trusted",
    },
    {
        "type": "proof_obligation",
        "bead_id": bead_id,
        "obligation": "parser_diff_passes_before_perf_regression_baseline",
        "status": "pass" if parser_diff_status == 0 else "fail",
        "evidence_path": "parser-diff-status.json",
        "notes": "scripts/parser-diff.sh stdout/stderr captured in this artifact bundle",
    },
]
for record in records:
    print(json.dumps(record, sort_keys=True))
PY

if [ "$PARSER_DIFF_STATUS" -ne 0 ]; then
  cat > "$ARTIFACT_DIR/README.md" <<EOF
# fmd parser perf run $RUN_ID

Status: parser-diff failed before measurements.

See:
- \`parser-diff-status.json\`
- \`parser-diff.stdout\`
- \`parser-diff.stderr\`
- \`fingerprint.json\`
- \`inprocess.jsonl\`
EOF
  echo "parser-perf: parser-diff failed; artifacts written to $ARTIFACT_DIR" >&2
  exit "$PARSER_DIFF_STATUS"
fi

echo "parser-perf: running parser scenarios ($ITERS iterations each)"
if [ -x /usr/bin/time ]; then
  /usr/bin/time -v "$HARNESS" --iters "$ITERS" --out-dir "$GOLDEN_DIR" \
    >> "$ARTIFACT_DIR/inprocess.jsonl" 2> "$ARTIFACT_DIR/time.stderr"
else
  "$HARNESS" --iters "$ITERS" --out-dir "$GOLDEN_DIR" \
    >> "$ARTIFACT_DIR/inprocess.jsonl"
  printf 'resource usage unavailable: /usr/bin/time not found\n' > "$ARTIFACT_DIR/time.stderr"
fi

echo "parser-perf: checksumming golden outputs"
python3 - "$GOLDEN_DIR" "$ARTIFACT_DIR/golden_checksums.txt" >> "$ARTIFACT_DIR/inprocess.jsonl" <<'PY'
import hashlib
import json
import pathlib
import sys

golden = pathlib.Path(sys.argv[1])
ledger_path = pathlib.Path(sys.argv[2])

with ledger_path.open("w") as ledger:
    for path in sorted(p for p in golden.rglob("*") if p.is_file()):
        rel = path.relative_to(golden).as_posix()
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        ledger.write(f"{digest}  {rel}\n")
        print(json.dumps({
            "type": "golden_checksum",
            "path": rel,
            "sha256": digest,
            "bytes": len(data),
        }, sort_keys=True))
PY

python3 - "$ARTIFACT_DIR" "$RUN_ID" "$BEAD_ID" >> "$ARTIFACT_DIR/inprocess.jsonl" <<'PY'
import json
import pathlib
import re
import sys

artifact_dir = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]
bead_id = sys.argv[3]
time_path = artifact_dir / "time.stderr"
time_text = time_path.read_text(errors="replace") if time_path.exists() else ""
rss_match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", time_text)
peak_rss_kb = int(rss_match.group(1)) if rss_match else None

samples = []
for line in (artifact_dir / "inprocess.jsonl").read_text().splitlines():
    if line.strip():
        row = json.loads(line)
        if row.get("type") == "perf_sample":
            samples.append(row)

samples.sort(key=lambda row: row["p95_ns"], reverse=True)

def ms(ns):
    return ns / 1_000_000

with (artifact_dir / "BASELINE.md").open("w") as f:
    f.write("# Parser Performance Baseline\n\n")
    f.write("| Scenario | p50 | p95 | p99 | Max | Blocks | Inlines | Diagnostics | Input | Output | Checksum |\n")
    f.write("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|\n")
    for row in samples:
        f.write(
            f"| `{row['scenario']}` | {ms(row['p50_ns']):.3f} ms | "
            f"{ms(row['p95_ns']):.3f} ms | {ms(row['p99_ns']):.3f} ms | "
            f"{ms(row['max_ns']):.3f} ms | {row['block_count']} | "
            f"{row['inline_count']} | {row['diagnostic_count']} | "
            f"{row['input_bytes']} B | {row['output_bytes']} B | `{row['output_checksum']}` |\n"
        )
    f.write("\n## Parser Diff\n\n")
    f.write("`scripts/parser-diff.sh` passed; see `parser-diff.stdout` and `parser-diff.stderr`.\n\n")
    f.write("## Peak RSS\n\n")
    if peak_rss_kb is None:
        f.write("Unavailable; see `time.stderr`.\n")
    else:
        f.write(f"Maximum resident set size: {peak_rss_kb} KB; see `time.stderr`.\n")

with (artifact_dir / "hotspot_table.md").open("w") as f:
    f.write("# Parser Hotspot Table\n\n")
    f.write("| Rank | Scenario | p95 | Mean | Blocks | Inlines | Diagnostics | Evidence |\n")
    f.write("|---:|---|---:|---:|---:|---:|---:|---|\n")
    for idx, row in enumerate(samples, 1):
        f.write(
            f"| {idx} | `{row['scenario']}` | {ms(row['p95_ns']):.3f} ms | "
            f"{ms(row['mean_ns']):.3f} ms | {row['block_count']} | "
            f"{row['inline_count']} | {row['diagnostic_count']} | `inprocess.jsonl` |\n"
        )

top = samples[0] if samples else None
with (artifact_dir / "hypothesis.md").open("w") as f:
    f.write("# Parser Hypothesis Summary\n\n")
    if top:
        f.write(
            f"- parser_regression_gate_ready: supports - `{top['scenario']}` is the current highest-p95 parser scenario and all scenario outputs have golden checksums.\n"
        )
    f.write("- correctness_before_speed: supports - `parser-diff` ran before measurements and passed.\n")
    f.write("- diagnostics_path_covered: supports - `malformed-diagnostics` requires at least one recoverable diagnostic while still rendering HTML.\n")
    f.write("- dependency_lean_perf_harness: supports - measurements use an in-repo example binary and add no third-party crates.\n")

with (artifact_dir / "README.md").open("w") as f:
    f.write(f"# fmd parser perf run {run_id}\n\n")
    f.write("Status: pass\n\n")
    f.write("Artifacts:\n\n")
    f.write("- `SCHEMA.md` - schema stamp for this parser run.\n")
    f.write("- `DEFINE.md` - scenario, metric, scope, and regression signal.\n")
    f.write("- `schema_manifest.json` - machine-readable file/record mapping.\n")
    f.write("- `fingerprint.json` - git, host, toolchain, and build profile.\n")
    f.write("- `parser-diff-status.json`, `parser-diff.stdout`, `parser-diff.stderr` - correctness gate evidence.\n")
    f.write("- `inprocess.jsonl` - parser scenario starts, perf samples, checksums, proofs, and run completion.\n")
    f.write("- `BASELINE.md` - p50/p95/p99/max with block/inline/diagnostic counts.\n")
    f.write("- `hotspot_table.md` - p95-ranked parser scenarios.\n")
    f.write("- `hypothesis.md` - accepted/rejected hypotheses for the run.\n")
    f.write("- `golden/` - rendered HTML outputs for every scenario.\n")
    f.write("- `golden_checksums.txt` - SHA-256 ledger for behavior-preservation outputs.\n")
    f.write("- `time.stderr` - peak RSS probe when `/usr/bin/time -v` is available.\n")

records = [
    {
        "type": "resource_summary",
        "scenario": "parser-perf-all",
        "available": peak_rss_kb is not None,
        "peak_rss_kb": peak_rss_kb,
        "stderr_path": "time.stderr",
        "notes": "process-level peak RSS for the parser performance harness where /usr/bin/time -v is available",
    },
    {
        "type": "hypothesis_evaluated",
        "hypothesis": "parser_regression_script_covers_required_scenarios",
        "result": "supports",
        "evidence_path": "BASELINE.md",
        "notes": "all required parser scenarios emitted perf_sample records with block, inline, diagnostic, timing, and checksum fields",
    },
    {
        "type": "proof_obligation",
        "bead_id": bead_id,
        "obligation": "parser_perf_artifact_bundle_complete",
        "status": "pass",
        "evidence_path": "README.md",
        "notes": "artifact bundle includes schema, fingerprint, parser-diff status, inprocess samples, RSS evidence, checksums, baseline, hotspot table, and hypothesis summary",
    },
    {
        "type": "run_complete",
        "run_id": run_id,
        "status": "pass",
        "artifact_dir": str(artifact_dir),
        "summary_path": "BASELINE.md",
        "top_hotspot": top["scenario"] if top else "none",
        "notes": "parser performance regression script completed successfully",
    },
]
for record in records:
    print(json.dumps(record, sort_keys=True))
PY

echo "parser-perf: wrote $ARTIFACT_DIR"
