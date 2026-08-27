#!/usr/bin/env bash
# Scanner-share gate for bead p61q.1.
# Validates the run id before any artifact path is constructed or written.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/scanner-share-proof.sh [--iters N] [--scanner-iters N] [--outer N] [--run-id ID]

Measures find_html_text_escape / find_html_escape / find_any_special_byte as a
share of parse+HTML p95. Writes tests/artifacts/perf/<run-id>/ following
docs/PERFORMANCE_ARTIFACT_SCHEMA.md (fmd-perf-artifact-v1).

Decision rule (also in the JSONL): proceed to SIMD only if a named scanner's
median p95 is >= 2% of parse+HTML p95 on at least one document.

Options:
  --iters N           parse+HTML inner iterations (default: 8)
  --scanner-iters N   isolated scanner inner iterations (default: 400)
  --outer N           independent outer runs; medians are taken (default: 3)
  --run-id ID         artifact directory name (validated before use)
  -h, --help          print this help
USAGE
}

ITERS=8
SCANNER_ITERS=400
OUTER=3
RUN_ID=""
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"
BEAD_ID="br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-p61q.1"
ORIGINAL_ARGS="$*"

fail() {
  printf 'scanner-share-proof: %s\n' "$*" >&2
  exit 1
}

log_check() {
  printf 'scanner-share-proof: check id=%s subject=%s outcome=%s\n' "$1" "$2" "$3" >&2
}

log_phase() {
  printf 'scanner-share-proof: phase=%s %s\n' "$1" "$2" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --iters)
      ITERS="${2:?--iters requires a value}"
      shift 2
      ;;
    --scanner-iters)
      SCANNER_ITERS="${2:?--scanner-iters requires a value}"
      shift 2
      ;;
    --outer)
      OUTER="${2:?--outer requires a value}"
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
      echo "scanner-share-proof: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

case "$ITERS" in
  ''|*[!0-9]*) fail "--iters must be a positive integer" ;;
esac
case "$SCANNER_ITERS" in
  ''|*[!0-9]*) fail "--scanner-iters must be a positive integer" ;;
esac
case "$OUTER" in
  ''|*[!0-9]*) fail "--outer must be a positive integer" ;;
esac
if [ "$ITERS" -eq 0 ] || [ "$SCANNER_ITERS" -eq 0 ] || [ "$OUTER" -eq 0 ]; then
  fail "counts must be greater than zero"
fi

if ! command -v python3 >/dev/null 2>&1; then
  fail "python3 is required for artifact summarization"
fi

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-scanner-share-$(git rev-parse --short HEAD 2>/dev/null || echo "head")"
fi
fmd_validate_run_id "scanner-share-proof" "$RUN_ID"
log_check "run-id" "$RUN_ID" "PASS"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"

if [ -e "$ARTIFACT_DIR/inprocess.jsonl" ]; then
  echo "scanner-share-proof: refusing to append to existing run: $ARTIFACT_DIR" >&2
  echo "scanner-share-proof: pass a fresh --run-id or inspect the existing artifact directory" >&2
  exit 73
fi

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR"
log_phase "mkdir" "dir=$ARTIFACT_DIR"

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This scanner-share run follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`
EOF

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - scanner share of render time (p61q.1)

## Scope
Isolated timings for \`find_html_text_escape\`, \`find_html_escape\`, and
\`find_any_special_byte\` versus parse+HTML end-to-end p95. \`scan_markdown_line\`
is recorded as parser-path context (production parse does not call
\`find_any_special_byte\`).

## Metric
Share in basis points: \`scanner_p95_ns * 10000 / e2e_p95_ns\`. Primary e2e is
parse+HTML. Each cell is the median of ${OUTER} outer runs.

## Corpus
- \`examples/showcase.md\` (golden)
- \`README.md\` (golden)
- \`tests/fixtures/perf/scanner-share/prose-heavy.md\` (committed seed)
- \`tests/fixtures/perf/scanner-share/escape-heavy.md\` (committed seed)
- \`generated-large\`: ${OUTER} is irrelevant to generation; target 1048576 bytes
  from repeating prose-heavy.md (dumped into this artifact, not git)

## Gate
Proceed to SIMD only if any *named* scanner is >= 200 bp (2%) of parse+HTML p95
on at least one document. Otherwise close the SIMD island as measured-no-op.
EOF

cat > "$ARTIFACT_DIR/schema_manifest.json" <<EOF
{
  "schema_version": "$SCHEMA_VERSION",
  "schema_doc": "$SCHEMA_DOC",
  "run_id": "$RUN_ID",
  "artifact_dir": "$ARTIFACT_DIR",
  "bead_id": "$BEAD_ID",
  "primary_jsonl": "inprocess.jsonl"
}
EOF

log_phase "build" "profile=release-perf example=fmd_scanner_share"
RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes" cargo build --profile release-perf --example fmd_scanner_share
log_check "build" "fmd_scanner_share" "PASS"

TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
HARNESS="$TARGET_DIR/release-perf/examples/fmd_scanner_share"
if [ ! -x "$HARNESS" ]; then
  fail "expected harness not found: $HARNESS"
fi

log_phase "measure" "iters=$ITERS scanner_iters=$SCANNER_ITERS outer=$OUTER"
set +e
"$HARNESS" --iters "$ITERS" --scanner-iters "$SCANNER_ITERS" --outer "$OUTER" --out-dir "$GOLDEN_DIR" \
  > "$ARTIFACT_DIR/inprocess.jsonl" 2> "$ARTIFACT_DIR/harness.stderr"
HARNESS_STATUS=$?
set -e
if [ "$HARNESS_STATUS" -ne 0 ]; then
  log_check "harness" "exit=$HARNESS_STATUS" "FAIL"
  fail "harness exited $HARNESS_STATUS; see $ARTIFACT_DIR/harness.stderr"
fi
log_check "harness" "jsonl=$ARTIFACT_DIR/inprocess.jsonl" "PASS"

python3 - "$ARTIFACT_DIR" "$RUN_ID" "$SCHEMA_VERSION" "$BEAD_ID" "$ORIGINAL_ARGS" <<'PY'
import hashlib, json, pathlib, platform, socket, subprocess, sys

artifact_dir = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]
schema_version = sys.argv[3]
bead_id = sys.argv[4]
command_tail = sys.argv[5] if len(sys.argv) > 5 else ""

def cmd(args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)
    except Exception as exc:
        return f"unavailable: {exc}"

def load_jsonl(path):
    rows = []
    for line in path.read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        rows.append(json.loads(line))
    return rows

rows = load_jsonl(artifact_dir / "inprocess.jsonl")
decision = next((r for r in rows if r.get("type") == "gate_decision"), {})
shares = [r for r in rows if r.get("type") == "share_summary"]
named = [r for r in shares if r.get("named_gate_scanner")]
samples = [r for r in rows if r.get("type") == "perf_sample"]
fingerprint_row = next((r for r in rows if r.get("type") == "host_fingerprint"), {})

fingerprint = {
    "schema_version": schema_version,
    "run_id": run_id,
    "bead_id": bead_id,
    "captured_at_utc": cmd(["date", "-u", "+%Y-%m-%dT%H:%M:%SZ"]).strip(),
    "git_sha": cmd(["git", "rev-parse", "HEAD"]).strip(),
    "git_status_short": cmd(["git", "status", "--short", "--branch"]),
    "host": {
        "hostname": socket.gethostname(),
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "uname": cmd(["uname", "-a"]).strip(),
        "harness_cpu_features": fingerprint_row.get("cpu_features"),
        "harness_arch": fingerprint_row.get("arch"),
    },
    "toolchain": {
        "rustc": cmd(["rustc", "-vV"]),
        "cargo": cmd(["cargo", "--version"]).strip(),
    },
    "build_profile": {
        "name": "release-perf",
        "rustflags": "-C force-frame-pointers=yes",
    },
    "command": f"scripts/scanner-share-proof.sh {command_tail}".strip(),
    "artifact_dir": str(artifact_dir),
}
(artifact_dir / "fingerprint.json").write_text(json.dumps(fingerprint, indent=2, sort_keys=True) + "\n")

checksum_lines = []
golden = artifact_dir / "golden"
if golden.is_dir():
    for path in sorted(golden.rglob("*")):
        if path.is_file():
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            rel = path.relative_to(artifact_dir).as_posix()
            checksum_lines.append(f"{digest}  {rel}")
(artifact_dir / "golden_checksums.txt").write_text("\n".join(checksum_lines) + ("\n" if checksum_lines else ""))

baseline = ["# BASELINE", "", "| document | category | p50_ns | p95_ns | input_bytes |", "|---|---|---:|---:|---:|"]
for row in samples:
    baseline.append(
        f"| {row.get('scenario','')} | {row.get('category','')} | {row.get('p50_ns','')} | {row.get('p95_ns','')} | {row.get('input_bytes','')} |"
    )
(artifact_dir / "BASELINE.md").write_text("\n".join(baseline) + "\n")

hot = ["# Hotspot table", "", "| document | scanner | named | scanner_p95_ns | e2e_p95_ns | share_bp | over_2pct |", "|---|---|---|---:|---:|---:|---|"]
ranked = sorted(shares, key=lambda r: int(r.get("share_bp") or 0), reverse=True)
for row in ranked:
    hot.append(
        f"| {row.get('scenario')} | {row.get('scanner')} | {row.get('named_gate_scanner')} | {row.get('scanner_p95_ns')} | {row.get('e2e_p95_ns')} | {row.get('share_bp')} | {row.get('over_threshold')} |"
    )
(artifact_dir / "hotspot_table.md").write_text("\n".join(hot) + "\n")

verdict = decision.get("verdict", "unknown")
max_bp = decision.get("max_share_bp", 0)
go = verdict == "go"
(artifact_dir / "hypothesis.md").write_text(
    "\n".join(
        [
            "# Hypotheses",
            "",
            f"- H1 named scanners are >= 2% of parse+HTML p95: {'ACCEPTED' if go else 'REJECTED'} (max_share_bp={max_bp}).",
            "- H2 production parse is dominated by find_any_special_byte: see scan_markdown_line context rows; find_any_special_byte is not on the parse hot path.",
            f"- Gate verdict: {verdict}. SIMD island may start only on go.",
            "",
        ]
    )
)

(artifact_dir / "DECISION.md").write_text(
    "\n".join(
        [
            "# Gate decision",
            "",
            f"- Bead: `{bead_id}`",
            f"- Verdict: **{verdict}**",
            f"- Max named-scanner share: {max_bp} bp (threshold 200 bp = 2%)",
            f"- Hottest document: {decision.get('hottest_document', '')}",
            f"- Hottest scanner: {decision.get('hottest_scanner', '')}",
            "",
            decision.get("rule", ""),
            "",
        ]
    )
)

readme = [
    "# Scanner-share proof",
    "",
    f"Run `{run_id}` for `{bead_id}`.",
    "",
    f"Verdict: **{verdict}** (max named share {max_bp} bp vs 200 bp threshold).",
    "",
    "See `DECISION.md`, `hotspot_table.md`, `BASELINE.md`, `fingerprint.json`, and `inprocess.jsonl`.",
    "",
]
(artifact_dir / "README.md").write_text("\n".join(readme))

print(f"scanner-share-proof: check id=decision subject=verdict={verdict} max_share_bp={max_bp} outcome=PASS", file=sys.stderr)
if not named:
    sys.exit("scanner-share-proof: no named-scanner share_summary rows")
PY

log_check "summarize" "$ARTIFACT_DIR" "PASS"
log_phase "complete" "artifact=$ARTIFACT_DIR"
