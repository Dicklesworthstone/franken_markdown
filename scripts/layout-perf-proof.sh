#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/layout-perf-proof.sh [--iters N] [--run-id ID]

Creates an agent-friendly layout/hyphenation quality and performance proof
bundle under:
  tests/artifacts/perf/<run-id>/

The run:
  - builds examples/fmd_layout_perf.rs with the release-perf profile,
  - measures required text-layout scenarios through the real library layout API,
  - records git/toolchain/host/build fingerprints,
  - records paragraph count, word count, hyphenation count, line count, badness
    totals, cumulative demerits, widow/orphan placeholder count, p50/p95/p99/max,
    checksums, and per-scenario line-break ledgers,
  - validates that every perf_sample has a matching non-empty ledger.

Required scenarios:
  paragraph-1000, unique-long-words, narrow-measure, wide-measure,
  punctuation-heavy, code-table-list-heavy, readme, generated-large.

Options:
  --iters N    layout iterations per scenario (default: 25)
  --run-id ID deterministic artifact directory name
  -h, --help  print this help
USAGE
}

ITERS=25
RUN_ID=""
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"
BEAD_ID="br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.7.6"
ORIGINAL_ARGS="$*"

fail() {
  printf 'layout-perf-proof: %s\n' "$*" >&2
  exit 1
}

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

validate_run_id() {
  case "$1" in
    ''|'.'|'..'|[!A-Za-z0-9]*|*[^A-Za-z0-9._-]*)
      fail "--run-id must start with an ASCII letter/digit and contain only ASCII letters, digits, dot, underscore, or dash"
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
      echo "layout-perf-proof: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

case "$ITERS" in
  ''|*[!0-9]*)
    fail "--iters must be a positive integer"
    ;;
esac
if [ "$ITERS" -eq 0 ]; then
  fail "--iters must be greater than zero"
fi

if ! command -v python3 >/dev/null 2>&1; then
  fail "python3 is required for artifact summarization"
fi

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-layout-$(git rev-parse --short HEAD)"
fi
validate_run_id "$RUN_ID"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"

if [ -e "$ARTIFACT_DIR/inprocess.jsonl" ]; then
  fail "refusing to append to existing run: $ARTIFACT_DIR; pass a fresh --run-id"
fi

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR"
: > "$ARTIFACT_DIR/inprocess.jsonl"

append_jsonl() {
  printf '%s\n' "$1" >> "$ARTIFACT_DIR/inprocess.jsonl"
}

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This layout performance proof follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`

This is the \`qw1.7.6\` layout/hyphenation proof harness. It uses the shared
artifact shape while adding layout-specific quality counters and per-scenario
line-break ledgers.
EOF

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - layout quality/performance proof

## Scope
This run measures text layout and hyphenation quality through the in-process
library API, not through CLI process startup. It focuses on the line-breaker and
hyphenator surfaces needed before layout optimization children can proceed.

## Metric
Primary performance metric is per-scenario p95 line-break latency in
nanoseconds. Quality counters are paragraph count, word count, legal hyphenation
point count, chosen line count, badness total, cumulative demerits, and
widow/orphan count. Widow/orphan count is currently \`0\` with
\`page_builder_not_modelled_yet\` because this harness does not model pagination.

## Scenarios
- \`paragraph-1000\`: 1000-word repeated paragraph.
- \`unique-long-words\`: unique long words stressing hyphenation points.
- \`narrow-measure\`: prose at a narrow measure.
- \`wide-measure\`: the same prose at a wide measure.
- \`punctuation-heavy\`: punctuation and code-like token pressure.
- \`code-table-list-heavy\`: Markdown list/table/code document projected to layout text.
- \`readme\`: repository README projected to layout text.
- \`generated-large\`: generated large mixed Markdown document.

## Pass criteria
All required scenarios must emit a \`perf_sample\`, a non-empty
\`golden/<scenario>.breaks.tsv\` ledger, and a checksum entry. Ledgers must have
exactly the line count reported by the sample and every chosen line must have a
badness in \`0..=10000\`.
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
    "perf_sample",
    "golden_checksum",
    "proof_obligation",
    "hypothesis_evaluated",
    "run_complete"
  ],
  "current_layout_mapping": {
    "fingerprint.json": ["run_start", "host_fingerprint", "build_profile"],
    "inprocess.jsonl": ["run_start", "host_fingerprint", "build_profile", "perf_sample", "golden_checksum", "proof_obligation", "hypothesis_evaluated", "run_complete"],
    "golden/*.breaks.tsv": ["line-break ledger"],
    "golden_checksums.txt": ["golden_checksum"],
    "BASELINE.md": ["run_complete"],
    "hotspot_table.md": ["hotspot interpretation"],
    "hypothesis.md": ["hypothesis_evaluated"]
  }
}
EOF

GIT_SHA="$(git rev-parse HEAD)"
CREATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMAND="scripts/layout-perf-proof.sh $ORIGINAL_ARGS"
GIT_STATUS="$(git status --short --branch)"
UNAME="$(uname -a 2>/dev/null || printf unavailable)"
RUSTC_VERSION="$(rustc -vV 2>/dev/null || printf unavailable)"
CARGO_VERSION="$(cargo --version 2>/dev/null || printf unavailable)"
TARGET_TRIPLE="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' | head -1)"

printf '%s\n' "$GIT_STATUS" > "$ARTIFACT_DIR/git-status.txt"
lscpu > "$ARTIFACT_DIR/lscpu.txt" 2> "$ARTIFACT_DIR/lscpu.stderr" || true

cat > "$ARTIFACT_DIR/fingerprint.json" <<EOF
{
  "schema_version": "$SCHEMA_VERSION",
  "run_id": "$RUN_ID",
  "captured_at_utc": "$(json_escape "$CREATED_AT")",
  "git_sha": "$(json_escape "$GIT_SHA")",
  "git_status_path": "git-status.txt",
  "host": {
    "uname": "$(json_escape "$UNAME")",
    "lscpu_path": "lscpu.txt"
  },
  "toolchain": {
    "rustc": "$(json_escape "$RUSTC_VERSION")",
    "cargo": "$(json_escape "$CARGO_VERSION")",
    "target_triple": "$(json_escape "$TARGET_TRIPLE")"
  },
  "build_profile": {
    "name": "release-perf",
    "rustflags": "-C force-frame-pointers=yes",
    "strip": false,
    "debug": "line-tables-only"
  },
  "artifact_dir": "$(json_escape "$ARTIFACT_DIR")"
}
EOF

append_jsonl "{\"type\":\"run_start\",\"schema_version\":\"$SCHEMA_VERSION\",\"run_id\":\"$(json_escape "$RUN_ID")\",\"created_at_utc\":\"$(json_escape "$CREATED_AT")\",\"git_sha\":\"$(json_escape "$GIT_SHA")\",\"dirty_status\":\"$(json_escape "$GIT_STATUS")\",\"command\":\"$(json_escape "$COMMAND")\",\"artifact_dir\":\"$(json_escape "$ARTIFACT_DIR")\"}"
append_jsonl "{\"type\":\"host_fingerprint\",\"target_triple\":\"$(json_escape "$TARGET_TRIPLE")\",\"rustc\":\"$(json_escape "$RUSTC_VERSION")\",\"cargo\":\"$(json_escape "$CARGO_VERSION")\",\"os\":\"$(json_escape "$UNAME")\",\"cpu\":\"lscpu.txt\",\"feature_flags\":[\"layout-proof\"],\"build_profile\":\"release-perf\"}"
append_jsonl "{\"type\":\"build_profile\",\"profile\":\"release-perf\",\"rustflags\":\"-C force-frame-pointers=yes\",\"debug\":\"line-tables-only\",\"strip\":false,\"frame_pointers\":true}"

echo "layout-perf-proof: building release-perf layout harness"
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile release-perf --example fmd_layout_perf
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
HARNESS="$TARGET_DIR/release-perf/examples/fmd_layout_perf"
if [ ! -x "$HARNESS" ]; then
  fail "expected layout harness not found: $HARNESS"
fi

echo "layout-perf-proof: running layout scenarios ($ITERS iterations)"
set +e
"$HARNESS" --scenario all --iters "$ITERS" --out-dir "$GOLDEN_DIR" >> "$ARTIFACT_DIR/inprocess.jsonl" 2> "$ARTIFACT_DIR/harness.stderr"
HARNESS_STATUS=$?
set -e
if [ "$HARNESS_STATUS" -ne 0 ]; then
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"layout_harness_completed\",\"status\":\"fail\",\"evidence_path\":\"harness.stderr\",\"notes\":\"exit_code=$HARNESS_STATUS\"}"
  fail "layout harness failed with exit code $HARNESS_STATUS; see $ARTIFACT_DIR/harness.stderr"
fi
append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"layout_harness_completed\",\"status\":\"pass\",\"evidence_path\":\"inprocess.jsonl\",\"notes\":\"all scenarios executed\"}"

echo "layout-perf-proof: checksumming line-break ledgers"
LEDGER_LIST_ABS="$ROOT/$ARTIFACT_DIR/golden-ledgers.list0"
(cd "$GOLDEN_DIR" && find . -type f -name '*.breaks.tsv' -print0 | sort -z) > "$LEDGER_LIST_ABS"
if [ ! -s "$LEDGER_LIST_ABS" ]; then
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"layout_ledgers_written\",\"status\":\"fail\",\"evidence_path\":\"golden/\",\"notes\":\"no *.breaks.tsv files were produced\"}"
  fail "no line-break ledgers were written under $GOLDEN_DIR"
fi
(cd "$GOLDEN_DIR" && xargs -0 sha256sum < "$LEDGER_LIST_ABS") > "$ARTIFACT_DIR/golden_checksums.txt"

python3 - "$ARTIFACT_DIR" "$BEAD_ID" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
bead_id = sys.argv[2]
required = [
    "paragraph-1000",
    "unique-long-words",
    "narrow-measure",
    "wide-measure",
    "punctuation-heavy",
    "code-table-list-heavy",
    "readme",
    "generated-large",
]
records = []
jsonl_path = root / "inprocess.jsonl"
for line_no, line in enumerate(jsonl_path.read_text().splitlines(), 1):
    if not line.strip():
        continue
    try:
        records.append(json.loads(line))
    except json.JSONDecodeError as exc:
        raise SystemExit(f"inprocess.jsonl line {line_no} is not valid JSON: {exc}") from exc

samples = [row for row in records if row.get("type") == "perf_sample"]
by_name = {row.get("scenario"): row for row in samples}
missing = [name for name in required if name not in by_name]
if missing:
    raise SystemExit(f"missing required layout perf_sample records: {missing}")

checksum_rows = []
validation_notes = []
for name in required:
    sample = by_name[name]
    ledger = root / "golden" / f"{name}.breaks.tsv"
    if not ledger.is_file() or ledger.stat().st_size == 0:
        raise SystemExit(f"missing or empty line-break ledger: {ledger}")
    lines = ledger.read_text().splitlines()
    if not lines or not lines[0].startswith("paragraph\tline\t"):
        raise SystemExit(f"ledger header is invalid: {ledger}")
    data_rows = lines[1:]
    if len(data_rows) != sample.get("line_count"):
        raise SystemExit(
            f"{name} ledger row count {len(data_rows)} != line_count {sample.get('line_count')}"
        )
    for row_idx, row in enumerate(data_rows, 2):
        parts = row.split("\t")
        if len(parts) != 8:
            raise SystemExit(f"{ledger}:{row_idx} expected 8 tab-separated columns")
        badness = int(parts[6])
        if badness < 0 or badness > 10000:
            raise SystemExit(f"{ledger}:{row_idx} badness out of range: {badness}")
    if sample.get("paragraph_count", 0) <= 0 or sample.get("word_count", 0) <= 0:
        raise SystemExit(f"{name} did not report useful paragraph/word counts")
    if sample.get("line_count", 0) <= 0:
        raise SystemExit(f"{name} did not choose any lines")
    validation_notes.append(f"{name}:{sample['line_count']} lines")

for line in (root / "golden_checksums.txt").read_text().splitlines():
    if not line.strip():
        continue
    sha, path = line.split(maxsplit=1)
    checksum_rows.append({"path": path.removeprefix("./"), "sha256": sha})

with jsonl_path.open("a") as f:
    for row in checksum_rows:
        path = root / "golden" / row["path"]
        f.write(json.dumps({
            "type": "golden_checksum",
            "path": row["path"],
            "sha256": row["sha256"],
            "bytes": path.stat().st_size,
        }, sort_keys=True) + "\n")
    f.write(json.dumps({
        "type": "proof_obligation",
        "bead_id": bead_id,
        "obligation": "layout_ledgers_match_reported_line_counts",
        "status": "pass",
        "evidence_path": "golden/*.breaks.tsv",
        "notes": "; ".join(validation_notes),
    }, sort_keys=True) + "\n")
    f.write(json.dumps({
        "type": "proof_obligation",
        "bead_id": bead_id,
        "obligation": "required_layout_scenarios_present",
        "status": "pass",
        "evidence_path": "inprocess.jsonl",
        "notes": ",".join(required),
    }, sort_keys=True) + "\n")

samples.sort(key=lambda row: row["p95_ns"], reverse=True)

def ms(ns):
    return ns / 1_000_000

with (root / "BASELINE.md").open("w") as f:
    f.write("# Layout Baseline\n\n")
    f.write("| Scenario | p50 | p95 | p99 | Max | Paragraphs | Words | Hyphen points | Lines | Badness total | Demerits | Width |\n")
    f.write("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n")
    for row in samples:
        f.write(
            f"| `{row['scenario']}` | {ms(row['p50_ns']):.3f} ms | "
            f"{ms(row['p95_ns']):.3f} ms | {ms(row['p99_ns']):.3f} ms | "
            f"{ms(row['max_ns']):.3f} ms | {row['paragraph_count']} | "
            f"{row['word_count']} | {row['hyphenation_count']} | {row['line_count']} | "
            f"{row['badness_total']} | {row['demerits']} | {row['line_width_pt']} pt |\n"
        )

with (root / "hotspot_table.md").open("w") as f:
    f.write("# Layout Hotspot Table\n\n")
    f.write("| Rank | Scenario | p95 | Lines | Hyphen points | Badness total | Evidence |\n")
    f.write("|---:|---|---:|---:|---:|---:|---|\n")
    for idx, row in enumerate(samples, 1):
        f.write(
            f"| {idx} | `{row['scenario']}` | {ms(row['p95_ns']):.3f} ms | "
            f"{row['line_count']} | {row['hyphenation_count']} | "
            f"{row['badness_total']} | `golden/{row['scenario']}.breaks.tsv` |\n"
        )

top = samples[0]
with (root / "hypothesis.md").open("w") as f:
    f.write("# Hypothesis Ledger\n\n")
    f.write(f"- dominant_layout_cost: supports - `{top['scenario']}` has highest p95 in `inprocess.jsonl`.\n")
    f.write("- linebreak_ledger_gate: supports - every required scenario has a non-empty ledger whose rows match reported line_count.\n")
    f.write("- widow_orphan_gate: deferred - this harness records `widow_orphan_count=0` until page-builder modeling exists.\n")
    f.write("- optimize_hyphen_cache_next: investigate - compare `hyphenation_count` and p95 before closing hyphen-cache children.\n")

with (root / "README.md").open("w") as f:
    f.write(f"# Layout Performance Proof\n\nRun directory: `{root}`\n\n")
    f.write("Primary files:\n\n")
    f.write("- `inprocess.jsonl`: schema records, layout samples, checksums, and proof obligations.\n")
    f.write("- `BASELINE.md`: scenario timing and quality counter table.\n")
    f.write("- `hotspot_table.md`: p95-ranked scenario summary.\n")
    f.write("- `golden/*.breaks.tsv`: per-scenario line-break ledgers.\n")
    f.write("- `golden_checksums.txt`: ledger checksums.\n")
    f.write("- `fingerprint.json`: git, host, toolchain, and build profile.\n")

with jsonl_path.open("a") as f:
    f.write(json.dumps({
        "type": "hypothesis_evaluated",
        "hypothesis": "layout_e2e_proof_covers_required_scenarios",
        "result": "supports",
        "evidence_path": "BASELINE.md",
        "notes": "required scenarios emitted perf samples, ledgers, quality counters, and checksums",
    }, sort_keys=True) + "\n")
    f.write(json.dumps({
        "type": "run_complete",
        "run_id": root.name,
        "artifact_dir": str(root),
        "status": "pass",
        "summary_path": "BASELINE.md",
        "top_hotspot": "hotspot_table.md",
        "notes": "layout performance proof completed",
    }, sort_keys=True) + "\n")
PY

echo "layout-perf-proof: ok ($ARTIFACT_DIR)"
