#!/usr/bin/env bash
# RSS vs page-count gate for bead u9jt.1.
# Validates the run id before any artifact path is constructed or written.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/rss-profile-proof.sh [--pages LIST] [--outer N] [--run-id ID]

Measures peak RSS of parse+PDF on synthetic 1k/5k/10k-page documents.
Writes tests/artifacts/perf/<run-id>/ following fmd-perf-artifact-v1.

Decision: proceed to chunked PDF emission only if 10k-page median RSS is
super-linear vs 1k (ratio > 12.5x for a 10x page increase) OR exceeds
the 1024 MiB ceiling. Otherwise close as measured-fine.

Options:
  --pages LIST   comma-separated requested page counts (default: 1000,5000,10000)
  --outer N      independent runs per size; medians are taken (default: 3)
  --lines N      lines of filler per section (default: 48)
  --width N      filler line width (default: 72)
  --run-id ID    artifact directory name (validated before use)
  -h, --help     print this help
USAGE
}

PAGES_CSV="1000,5000,10000"
OUTER=3
LINES=48
WIDTH=72
RUN_ID=""
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"
BEAD_ID="br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-u9jt.1"
CEILING_BYTES=$((1024 * 1024 * 1024))
SUPERLINEAR_BP=12500 # 12.50x vs 10x pages = 125% of linear
ORIGINAL_ARGS="$*"

fail() {
  printf 'rss-profile-proof: %s\n' "$*" >&2
  exit 1
}

log_check() {
  printf 'rss-profile-proof: check id=%s subject=%s outcome=%s\n' "$1" "$2" "$3" >&2
}

log_phase() {
  printf 'rss-profile-proof: phase=%s %s\n' "$1" "$2" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --pages)
      PAGES_CSV="${2:?--pages requires a value}"
      shift 2
      ;;
    --outer)
      OUTER="${2:?--outer requires a value}"
      shift 2
      ;;
    --lines)
      LINES="${2:?--lines requires a value}"
      shift 2
      ;;
    --width)
      WIDTH="${2:?--width requires a value}"
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
      echo "rss-profile-proof: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

case "$OUTER" in
  ''|*[!0-9]*) fail "--outer must be a positive integer" ;;
esac
if [ "$OUTER" -eq 0 ]; then
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
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-rss-profile-$(git rev-parse --short HEAD 2>/dev/null || echo "head")"
fi
fmd_validate_run_id "rss-profile-proof" "$RUN_ID"
log_check "run-id" "$RUN_ID" "PASS"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"

if [ -e "$ARTIFACT_DIR/inprocess.jsonl" ]; then
  echo "rss-profile-proof: refusing to append to existing run: $ARTIFACT_DIR" >&2
  echo "rss-profile-proof: pass a fresh --run-id or inspect the existing artifact directory" >&2
  exit 73
fi

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR"
log_phase "mkdir" "dir=$ARTIFACT_DIR"

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This RSS-profile run follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`
EOF

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - RSS vs page count (u9jt.1)

## Scope
Peak RSS of a single parse+PDF of synthetic Letter-sized documents at
requested page counts ${PAGES_CSV}. Generator flags: --lines ${LINES} --width ${WIDTH}.

## Metric
Primary: median peak RSS in bytes across ${OUTER} independent process runs.
Secondary: wall_ns, observed_pages, pdf_bytes.

## Ceiling
${CEILING_BYTES} bytes (1024 MiB) at the largest requested size.

## Super-linear rule
If 1k and 10k (or the smallest and largest requested sizes) both exist:
RSS_large / RSS_small > (pages_large / pages_small) * ${SUPERLINEAR_BP}/10000.
Default: 12.50x RSS for a 10x page increase.

## Gate
Proceed to chunked emission only if the ceiling is exceeded OR growth is
super-linear. Otherwise measured-fine (successful no-op).
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

TIME_MODE=""
if /usr/bin/time -l true >/dev/null 2>&1; then
  TIME_MODE="bsd"
elif /usr/bin/time -v true >/dev/null 2>&1; then
  TIME_MODE="gnu"
else
  fail "need /usr/bin/time -l (BSD/macOS) or /usr/bin/time -v (GNU/Linux)"
fi
log_check "time" "mode=$TIME_MODE" "PASS"

log_phase "build" "profile=release-perf example=fmd_rss_profile"
RUSTFLAGS="${RUSTFLAGS:-} -C force-frame-pointers=yes" cargo build --profile release-perf --example fmd_rss_profile
log_check "build" "fmd_rss_profile" "PASS"

TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
HARNESS="$TARGET_DIR/release-perf/examples/fmd_rss_profile"
if [ ! -x "$HARNESS" ]; then
  fail "expected harness not found: $HARNESS"
fi

: > "$ARTIFACT_DIR/inprocess.jsonl"
IFS=',' read -r -a PAGE_LIST <<< "$PAGES_CSV"

for pages in "${PAGE_LIST[@]}"; do
  case "$pages" in
    ''|*[!0-9]*) fail "--pages entries must be positive integers (got '$pages')" ;;
  esac
  if [ "$pages" -eq 0 ]; then
    fail "page counts must be greater than zero"
  fi
  run=1
  while [ "$run" -le "$OUTER" ]; do
    log_phase "measure" "pages=$pages run=$run/$OUTER"
    TIME_ERR="$ARTIFACT_DIR/time-p${pages}-r${run}.stderr"
    DUMP="$GOLDEN_DIR/generated-p${pages}.md"
    DUMP_FLAG=()
    if [ "$run" -eq 1 ]; then
      DUMP_FLAG=(--dump "$DUMP")
    fi
    set +e
    if [ "$TIME_MODE" = "bsd" ]; then
      /usr/bin/time -l "$HARNESS" --pages "$pages" --lines "$LINES" --width "$WIDTH" "${DUMP_FLAG[@]}" \
        >> "$ARTIFACT_DIR/inprocess.jsonl" 2> "$TIME_ERR"
    else
      /usr/bin/time -v "$HARNESS" --pages "$pages" --lines "$LINES" --width "$WIDTH" "${DUMP_FLAG[@]}" \
        >> "$ARTIFACT_DIR/inprocess.jsonl" 2> "$TIME_ERR"
    fi
    STATUS=$?
    set -e
    if [ "$STATUS" -ne 0 ]; then
      log_check "harness" "pages=$pages run=$run exit=$STATUS" "FAIL"
      fail "harness exited $STATUS; see $TIME_ERR"
    fi
    python3 - "$TIME_ERR" "$pages" "$run" "$TIME_MODE" >> "$ARTIFACT_DIR/inprocess.jsonl" <<'PY'
import json, pathlib, re, sys
path = pathlib.Path(sys.argv[1])
pages = int(sys.argv[2])
run = int(sys.argv[3])
mode = sys.argv[4]
text = path.read_text(errors="replace")
rss_bytes = None
if mode == "bsd":
    m = re.search(r"^\s*(\d+)\s+maximum resident set size", text, re.M | re.I)
    if m:
        rss_bytes = int(m.group(1))
else:
    m = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", text, re.I)
    if m:
        rss_bytes = int(m.group(1)) * 1024
print(json.dumps({
    "type": "rss_probe",
    "requested_pages": pages,
    "run": run,
    "time_mode": mode,
    "peak_rss_bytes": rss_bytes,
    "stderr_path": path.name,
}))
if rss_bytes is None:
    sys.exit("rss-profile-proof: could not parse peak RSS from " + str(path))
PY
    log_check "rss" "pages=$pages run=$run" "PASS"
    run=$((run + 1))
  done
done

python3 - "$ARTIFACT_DIR" "$RUN_ID" "$SCHEMA_VERSION" "$BEAD_ID" "$CEILING_BYTES" "$SUPERLINEAR_BP" "$ORIGINAL_ARGS" <<'PY'
import json, pathlib, platform, socket, subprocess, sys, statistics

artifact_dir = pathlib.Path(sys.argv[1])
run_id = sys.argv[2]
schema_version = sys.argv[3]
bead_id = sys.argv[4]
ceiling = int(sys.argv[5])
superlinear_bp = int(sys.argv[6])
command_tail = sys.argv[7] if len(sys.argv) > 7 else ""

def cmd(args):
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.STDOUT)
    except Exception as exc:
        return f"unavailable: {exc}"

rows = []
for line in (artifact_dir / "inprocess.jsonl").read_text().splitlines():
    line = line.strip()
    if line:
        rows.append(json.loads(line))

probes = [r for r in rows if r.get("type") == "rss_probe"]
renders = [r for r in rows if r.get("type") == "rss_render_sample"]

by_pages = {}
for p in probes:
    by_pages.setdefault(int(p["requested_pages"]), []).append(int(p["peak_rss_bytes"]))

summary = []
for pages in sorted(by_pages):
    vals = sorted(by_pages[pages])
    med = vals[len(vals) // 2]
    matching = [r for r in renders if int(r.get("requested_pages") or 0) == pages]
    observed = None
    pdf_bytes = None
    wall = None
    if matching:
        obs = [int(r["observed_pages"]) for r in matching if r.get("observed_pages") is not None]
        pdfs = [int(r["pdf_bytes"]) for r in matching]
        walls = [int(r["wall_ns"]) for r in matching]
        observed = obs[len(obs) // 2] if obs else None
        pdf_bytes = pdfs[len(pdfs) // 2]
        wall = walls[len(walls) // 2]
    summary.append({
        "requested_pages": pages,
        "median_peak_rss_bytes": med,
        "runs": len(vals),
        "observed_pages": observed,
        "median_pdf_bytes": pdf_bytes,
        "median_wall_ns": wall,
    })

smallest = summary[0] if summary else None
largest = summary[-1] if summary else None
ratio_pages = None
ratio_rss = None
superlinear = False
over_ceiling = False
if smallest and largest and smallest["requested_pages"] > 0:
    ratio_pages = largest["requested_pages"] / smallest["requested_pages"]
    if smallest["median_peak_rss_bytes"] > 0:
        ratio_rss = largest["median_peak_rss_bytes"] / smallest["median_peak_rss_bytes"]
        # superlinear if rss ratio exceeds page-ratio * superlinear_bp/10000
        superlinear = ratio_rss > ratio_pages * (superlinear_bp / 10000.0)
    over_ceiling = largest["median_peak_rss_bytes"] > ceiling

proceed = bool(superlinear or over_ceiling)
verdict = "go-chunked" if proceed else "measured-fine"

decision = {
    "type": "gate_decision",
    "bead_id": bead_id,
    "verdict": verdict,
    "proceed_to_chunked": proceed,
    "superlinear": superlinear,
    "over_ceiling": over_ceiling,
    "ceiling_bytes": ceiling,
    "superlinear_bp": superlinear_bp,
    "ratio_pages": ratio_pages,
    "ratio_rss": ratio_rss,
    "sizes": summary,
    "rule": "proceed to chunked emission only if 10k-vs-1k RSS is super-linear or 10k RSS exceeds 1024 MiB",
}
with (artifact_dir / "inprocess.jsonl").open("a") as fh:
    fh.write(json.dumps(decision) + "\n")

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
        "uname": cmd(["uname", "-a"]).strip(),
    },
    "toolchain": {
        "rustc": cmd(["rustc", "-vV"]),
        "cargo": cmd(["cargo", "--version"]).strip(),
    },
    "build_profile": {"name": "release-perf", "rustflags": "-C force-frame-pointers=yes"},
    "command": f"scripts/rss-profile-proof.sh {command_tail}".strip(),
    "artifact_dir": str(artifact_dir),
}
(artifact_dir / "fingerprint.json").write_text(json.dumps(fingerprint, indent=2, sort_keys=True) + "\n")

base = ["# BASELINE", "", "| requested_pages | observed_pages | median_peak_rss_bytes | median_pdf_bytes | median_wall_ns |", "|---:|---:|---:|---:|---:|"]
for row in summary:
    base.append(
        f"| {row['requested_pages']} | {row['observed_pages']} | {row['median_peak_rss_bytes']} | {row['median_pdf_bytes']} | {row['median_wall_ns']} |"
    )
(artifact_dir / "BASELINE.md").write_text("\n".join(base) + "\n")

hot = [
    "# Hotspot table",
    "",
    f"- Verdict: **{verdict}**",
    f"- Super-linear: {superlinear} (rss_ratio={ratio_rss}, page_ratio={ratio_pages}, threshold_bp={superlinear_bp})",
    f"- Over 1024 MiB ceiling: {over_ceiling}",
    "",
]
(artifact_dir / "hotspot_table.md").write_text("\n".join(hot))

(artifact_dir / "hypothesis.md").write_text(
    "\n".join(
        [
            "# Hypotheses",
            "",
            f"- H1 RSS at 10k pages is super-linear vs 1k: {'ACCEPTED' if superlinear else 'REJECTED'}.",
            f"- H2 RSS at largest size exceeds 1024 MiB: {'ACCEPTED' if over_ceiling else 'REJECTED'}.",
            f"- Gate verdict: {verdict}. Chunked emission starts only on go-chunked.",
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
            f"- Super-linear: {superlinear}",
            f"- Over ceiling: {over_ceiling}",
            f"- RSS ratio: {ratio_rss}",
            f"- Page ratio: {ratio_pages}",
            "",
            decision["rule"],
            "",
        ]
    )
)
(artifact_dir / "README.md").write_text(
    f"# RSS profile proof\n\nRun `{run_id}` for `{bead_id}`.\n\nVerdict: **{verdict}**.\n"
)
print(f"rss-profile-proof: check id=decision subject=verdict={verdict} outcome=PASS", file=sys.stderr)
if not summary:
    sys.exit("rss-profile-proof: no rss_probe rows")
PY

log_check "summarize" "$ARTIFACT_DIR" "PASS"
log_phase "complete" "artifact=$ARTIFACT_DIR"
