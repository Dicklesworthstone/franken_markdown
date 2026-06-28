#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/pdf-perf-proof.sh [--iters N] [--run-id ID] [--self-test-failure]

Creates an agent-friendly end-to-end PDF performance proof bundle under:
  tests/artifacts/perf/<run-id>/

The run:
  - builds the fmd CLI with the release-perf profile,
  - invokes the determinism gate and focused PDF structural tests,
  - renders required PDF scenarios through the real CLI,
  - records command line, git SHA, host/toolchain fingerprint, build profile,
    scenario p50/p95/p99/max, peak RSS where available, object/font/compression
    metrics, checksums, and explicit pass/fail proof obligations,
  - optionally proves the harness catches a deliberately corrupted checksum.

Required scenarios:
  showcase, readme, pdf-large, ligature-heavy, table-list-code-heavy,
  custom-options.

Options:
  --iters N             render iterations per scenario (default: 5)
  --run-id ID          deterministic artifact directory name
  --self-test-failure  corrupt a copy of one golden PDF and verify the checksum
                       invariant catches it; the script passes only if the
                       controlled failure is detected
  -h, --help           print this help
USAGE
}

ITERS=5
RUN_ID=""
SELF_TEST_FAILURE=0
SCHEMA_VERSION="fmd-perf-artifact-v1"
SCHEMA_DOC="docs/PERFORMANCE_ARTIFACT_SCHEMA.md"
BEAD_ID="br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.6.7"
ORIGINAL_ARGS="$*"

fail() {
  printf 'pdf-perf-proof: %s\n' "$*" >&2
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
    --self-test-failure)
      SELF_TEST_FAILURE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "pdf-perf-proof: unknown argument: $1" >&2
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

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

if [ -z "$RUN_ID" ]; then
  RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-pdf-proof-$(git rev-parse --short HEAD)"
fi
validate_run_id "$RUN_ID"

ARTIFACT_DIR="tests/artifacts/perf/$RUN_ID"
GOLDEN_DIR="$ARTIFACT_DIR/golden"
INPUT_DIR="$ARTIFACT_DIR/inputs"
RUN_DIR="$ARTIFACT_DIR/runs"

if [ -e "$ARTIFACT_DIR/inprocess.jsonl" ]; then
  fail "refusing to append to existing run: $ARTIFACT_DIR; pass a fresh --run-id"
fi

mkdir -p "$ARTIFACT_DIR" "$GOLDEN_DIR" "$INPUT_DIR" "$RUN_DIR"
: > "$ARTIFACT_DIR/inprocess.jsonl"

append_jsonl() {
  printf '%s\n' "$1" >> "$ARTIFACT_DIR/inprocess.jsonl"
}

run_cmd_to_files() {
  local label="$1"
  shift
  local stdout_path="$ARTIFACT_DIR/$label.stdout"
  local stderr_path="$ARTIFACT_DIR/$label.stderr"
  set +e
  "$@" > "$stdout_path" 2> "$stderr_path"
  local status=$?
  set -e
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"$(json_escape "$label")\",\"status\":\"$([ "$status" -eq 0 ] && printf pass || printf fail)\",\"evidence_path\":\"$label.stdout $label.stderr\",\"notes\":\"exit_code=$status\"}"
  if [ "$status" -ne 0 ]; then
    fail "$label failed with exit code $status; see $stdout_path and $stderr_path"
  fi
}

cat > "$ARTIFACT_DIR/README.md" <<EOF
# PDF Performance Proof

Run id: \`$RUN_ID\`

This artifact bundle was produced by:

\`\`\`bash
scripts/pdf-perf-proof.sh $ORIGINAL_ARGS
\`\`\`

Primary data:

- \`inprocess.jsonl\`: scenario metrics and proof obligations.
- \`fingerprint.json\`: git, host, toolchain, and build profile.
- \`golden_checksums.txt\`: behavior-preservation checksum ledger.
- \`BASELINE.md\`: scenario timing table.
- \`hotspot_table.md\`: p95-ranked scenario summary.
EOF

cat > "$ARTIFACT_DIR/SCHEMA.md" <<EOF
# Schema

This PDF performance proof follows \`$SCHEMA_VERSION\`.

Canonical schema documentation:

\`\`\`text
$SCHEMA_DOC
\`\`\`
EOF

cat > "$ARTIFACT_DIR/DEFINE.md" <<EOF
# DEFINE - PDF performance proof

## Scope
This run proves end-to-end PDF correctness and performance for the real \`fmd\`
CLI after PDF optimization work.

## Metric
Primary metric is per-scenario p95 wall-clock latency in nanoseconds. Secondary
metrics are p50, p99, max, input bytes, output bytes, peak RSS, PDF object count,
embedded font subset bytes, FlateDecode count, output/input ratio, and SHA-256.

## Scenarios
- \`showcase\`: \`examples/showcase.md\`.
- \`readme\`: repository \`README.md\`.
- \`pdf-large\`: generated large mixed Markdown document.
- \`ligature-heavy\`: repeated ligature-heavy prose.
- \`table-list-code-heavy\`: generated tables, nested lists, and code fences.
- \`custom-options\`: custom title/author/metadata/line-number options.

## Pass criteria
All renders must produce structurally valid PDFs, deterministic repeated bytes,
stable checksum records, successful determinism and structural test gates, and a
pass/fail summary suitable for agents.
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
    "proof_obligation",
    "perf_sample",
    "golden_checksum",
    "hypothesis_evaluated",
    "run_complete"
  ],
  "current_pdf_proof_mapping": {
    "fingerprint.json": ["run_start", "host_fingerprint", "build_profile"],
    "inprocess.jsonl": ["proof_obligation", "perf_sample", "golden_checksum", "hypothesis_evaluated", "run_complete"],
    "golden_checksums.txt": ["golden_checksum"],
    "BASELINE.md": ["run_complete"],
    "hotspot_table.md": ["hotspot interpretation"]
  }
}
EOF

GIT_SHA="$(git rev-parse HEAD)"
CREATED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
COMMAND="scripts/pdf-perf-proof.sh $ORIGINAL_ARGS"
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
    "debug": "line-tables-only",
    "frame_pointers": true
  },
  "artifact_dir": "$(json_escape "$ARTIFACT_DIR")"
}
EOF

append_jsonl "{\"type\":\"run_start\",\"schema_version\":\"$SCHEMA_VERSION\",\"run_id\":\"$(json_escape "$RUN_ID")\",\"created_at_utc\":\"$(json_escape "$CREATED_AT")\",\"git_sha\":\"$(json_escape "$GIT_SHA")\",\"dirty_status\":\"$(json_escape "$GIT_STATUS")\",\"command\":\"$(json_escape "$COMMAND")\",\"artifact_dir\":\"$(json_escape "$ARTIFACT_DIR")\"}"
append_jsonl "{\"type\":\"host_fingerprint\",\"target_triple\":\"$(json_escape "$TARGET_TRIPLE")\",\"rustc\":\"$(json_escape "$RUSTC_VERSION")\",\"cargo\":\"$(json_escape "$CARGO_VERSION")\",\"os\":\"$(json_escape "$UNAME")\",\"cpu\":\"lscpu.txt\",\"feature_flags\":[\"cli\"],\"build_profile\":\"release-perf\"}"
append_jsonl "{\"type\":\"build_profile\",\"profile\":\"release-perf\",\"rustflags\":\"-C force-frame-pointers=yes\",\"debug\":\"line-tables-only\",\"strip\":false,\"frame_pointers\":true}"

echo "pdf-perf-proof: building fmd release-perf binary"
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile release-perf --bin fmd
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p' | head -1)"
FMD_BIN="$TARGET_DIR/release-perf/fmd"
if [ ! -x "$FMD_BIN" ]; then
  fail "expected fmd binary not found: $FMD_BIN"
fi

echo "pdf-perf-proof: running correctness gates"
run_cmd_to_files "determinism_gate" scripts/check-determinism.sh
run_cmd_to_files "pdf_structural_tests" cargo test --test pdf_test pdf_has_valid_header_xref_and_eof_marker -- --nocapture

generate_inputs() {
  cp examples/showcase.md "$INPUT_DIR/showcase.md"
  cp README.md "$INPUT_DIR/readme.md"

  : > "$INPUT_DIR/pdf-large.md"
  for i in $(seq 1 180); do
    cat >> "$INPUT_DIR/pdf-large.md" <<EOF

## Generated PDF Section $i

This section exercises paragraph layout with **bold**, *italic*, \`inline code\`,
links to [the project](https://example.test/franken_markdown), and enough words
to force high-quality wrapping across many lines in the PDF output.

| Metric | Value | Notes |
|---:|:---:|---|
| $i | $((i * 17)) | generated table row for measured layout |
| $((i + 1)) | $((i * 19)) | another row with more text |

\`\`\`rust
fn generated_section_$i(value: usize) -> usize {
    value.wrapping_mul($i).wrapping_add(42)
}
\`\`\`
EOF
  done

  : > "$INPUT_DIR/ligature-heavy.md"
  for i in $(seq 1 260); do
    printf 'Office affinity efficiently refines fluent official fixtures; affiliation, flicker, and final filenames flow. ' >> "$INPUT_DIR/ligature-heavy.md"
    if [ $((i % 5)) -eq 0 ]; then
      printf '\n\n' >> "$INPUT_DIR/ligature-heavy.md"
    fi
  done

  : > "$INPUT_DIR/table-list-code-heavy.md"
  for i in $(seq 1 120); do
    cat >> "$INPUT_DIR/table-list-code-heavy.md" <<EOF

### Workstream $i

- [x] Parse Markdown.
  - Measure tables.
  - Keep code readable.
- [ ] Render PDF proof artifacts.

| Item | Owner | Status | Detail |
|---|---|:---:|---|
| parser | fmd | green | nested list and table stress $i |
| pdf | fmd | green | syntax highlighting and wrapping $i |

\`\`\`text
alpha beta gamma delta epsilon $i
EOF marker stays literal in code
\`\`\`
EOF
  done

  cat > "$INPUT_DIR/custom-options.md" <<'EOF'
# Custom Options

This document is rendered with explicit title, author, metadata epoch, serif
font, and PDF line numbers.

```rust
fn main() {
    println!("custom options");
}
```
EOF
}

percentile_from_sorted() {
  local pct="$1"
  shift
  local values=("$@")
  local n="${#values[@]}"
  if [ "$n" -eq 0 ]; then
    printf '0'
    return
  fi
  local idx=$(( ((n - 1) * pct + 99) / 100 ))
  printf '%s' "${values[$idx]}"
}

sum_values() {
  local total=0
  local value
  for value in "$@"; do
    total=$((total + value))
  done
  printf '%s' "$total"
}

pdf_object_count() {
  LC_ALL=C grep -aE '^[0-9]+ 0 obj$' "$1" | wc -l | tr -d ' '
}

pdf_font_subset_bytes() {
  LC_ALL=C grep -ao '/Length1 [0-9][0-9]*' "$1" | awk '{s += $2} END {print s + 0}'
}

pdf_flate_count() {
  LC_ALL=C grep -ao '/Filter /FlateDecode' "$1" | wc -l | tr -d ' '
}

file_size_bytes() {
  wc -c < "$1" | tr -d ' '
}

verify_pdf_structure() {
  local pdf="$1"
  local scenario="$2"
  local object_count
  if ! head -c 5 "$pdf" | grep -aq '%PDF-'; then
    fail "$scenario produced a file without a %PDF- header: $pdf"
  fi
  if ! LC_ALL=C grep -aq 'xref' "$pdf"; then
    fail "$scenario PDF missing xref table: $pdf"
  fi
  if ! LC_ALL=C grep -aq '%%EOF' "$pdf"; then
    fail "$scenario PDF missing %%EOF marker: $pdf"
  fi
  if ! LC_ALL=C grep -aq '/FontFile2' "$pdf"; then
    fail "$scenario PDF missing embedded FontFile2 subset: $pdf"
  fi
  object_count="$(pdf_object_count "$pdf")"
  if [ "$object_count" -lt 4 ]; then
    fail "$scenario PDF object count too low ($object_count): $pdf"
  fi
}

check_sha256_matches() {
  local expected="$1"
  local path="$2"
  local actual
  actual="$(sha256sum "$path" | awk '{print $1}')"
  [ "$actual" = "$expected" ]
}

run_scenario() {
  local scenario="$1"
  local input="$2"
  local title="$3"
  shift 3
  local extra_args=("$@")
  local input_bytes
  input_bytes="$(file_size_bytes "$input")"
  local durations=()
  local peak_rss_kb=0
  local golden="$GOLDEN_DIR/$scenario.pdf"
  local first_sha=""

  append_jsonl "{\"type\":\"scenario_start\",\"scenario\":\"$(json_escape "$scenario")\",\"category\":\"render-pdf\",\"input_bytes\":$input_bytes,\"iterations\":$ITERS,\"notes\":\"end-to-end fmd CLI PDF render\"}"

  for iter in $(seq 1 "$ITERS"); do
    local out="$RUN_DIR/$scenario-$iter.pdf"
    local stderr="$RUN_DIR/$scenario-$iter.stderr"
    local time_file="$RUN_DIR/$scenario-$iter.time"
    local start_ns end_ns duration_ns render_status
    start_ns="$(date +%s%N)"
    if [ "$iter" -eq 1 ] && command -v /usr/bin/time >/dev/null 2>&1; then
      set +e
      SOURCE_DATE_EPOCH=1700000000 /usr/bin/time -v -o "$time_file" "$FMD_BIN" "$input" --to pdf --out "$out" --title "$title" "${extra_args[@]}" > /dev/null 2> "$stderr"
      render_status=$?
      set -e
      if [ -s "$time_file" ]; then
        peak_rss_kb="$(sed -n 's/.*Maximum resident set size (kbytes): //p' "$time_file" | head -1)"
        case "$peak_rss_kb" in
          ''|*[!0-9]*) peak_rss_kb=0 ;;
        esac
      else
        peak_rss_kb=0
      fi
    else
      set +e
      SOURCE_DATE_EPOCH=1700000000 "$FMD_BIN" "$input" --to pdf --out "$out" --title "$title" "${extra_args[@]}" > /dev/null 2> "$stderr"
      render_status=$?
      set -e
    fi
    end_ns="$(date +%s%N)"
    duration_ns=$((end_ns - start_ns))
    if [ "$render_status" -ne 0 ]; then
      append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"scenario_render_$(json_escape "$scenario")\",\"status\":\"fail\",\"evidence_path\":\"runs/$scenario-$iter.stderr\",\"notes\":\"iteration=$iter exit_code=$render_status\"}"
      fail "$scenario iteration $iter render command failed with exit code $render_status; see $stderr"
    fi
    durations+=("$duration_ns")
    verify_pdf_structure "$out" "$scenario"
    if [ "$iter" -eq 1 ]; then
      cp "$out" "$golden"
      first_sha="$(sha256sum "$golden" | awk '{print $1}')"
    elif ! check_sha256_matches "$first_sha" "$out"; then
      fail "$scenario iteration $iter checksum differs from iteration 1"
    fi
  done

  local sorted=()
  while IFS= read -r value; do
    sorted+=("$value")
  done < <(printf '%s\n' "${durations[@]}" | sort -n)

  local min max p50 p95 p99 total mean output_bytes object_count font_subset_bytes flate_count sha ratio
  min="${sorted[0]}"
  max="${sorted[$((${#sorted[@]} - 1))]}"
  p50="$(percentile_from_sorted 50 "${sorted[@]}")"
  p95="$(percentile_from_sorted 95 "${sorted[@]}")"
  p99="$(percentile_from_sorted 99 "${sorted[@]}")"
  total="$(sum_values "${durations[@]}")"
  mean=$((total / ${#durations[@]}))
  output_bytes="$(file_size_bytes "$golden")"
  object_count="$(pdf_object_count "$golden")"
  font_subset_bytes="$(pdf_font_subset_bytes "$golden")"
  flate_count="$(pdf_flate_count "$golden")"
  sha="$(sha256sum "$golden" | awk '{print $1}')"
  ratio="$(awk -v out="$output_bytes" -v inp="$input_bytes" 'BEGIN { if (inp > 0) printf "%.6f", out / inp; else printf "0.000000" }')"

  append_jsonl "{\"type\":\"perf_sample\",\"scenario\":\"$(json_escape "$scenario")\",\"category\":\"render-pdf\",\"iterations\":$ITERS,\"input_bytes\":$input_bytes,\"output_bytes\":$output_bytes,\"min_ns\":$min,\"mean_ns\":$mean,\"p50_ns\":$p50,\"p95_ns\":$p95,\"p99_ns\":$p99,\"max_ns\":$max,\"peak_rss_kb\":$peak_rss_kb,\"pdf_object_count\":$object_count,\"font_subset_bytes\":$font_subset_bytes,\"flate_stream_count\":$flate_count,\"compression_ratio_output_to_input\":$ratio,\"sha256\":\"$sha\",\"notes\":\"real fmd CLI PDF render\"}"
  append_jsonl "{\"type\":\"golden_checksum\",\"path\":\"$scenario.pdf\",\"sha256\":\"$sha\",\"bytes\":$output_bytes}"
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"scenario_render_$(json_escape "$scenario")\",\"status\":\"pass\",\"evidence_path\":\"golden/$scenario.pdf\",\"notes\":\"$ITERS deterministic render iterations completed\"}"
}

generate_inputs

run_scenario "showcase" "$INPUT_DIR/showcase.md" "Showcase"
run_scenario "readme" "$INPUT_DIR/readme.md" "README"
run_scenario "pdf-large" "$INPUT_DIR/pdf-large.md" "Generated Large PDF"
run_scenario "ligature-heavy" "$INPUT_DIR/ligature-heavy.md" "Ligature Heavy"
run_scenario "table-list-code-heavy" "$INPUT_DIR/table-list-code-heavy.md" "Table List Code Heavy"
run_scenario "custom-options" "$INPUT_DIR/custom-options.md" "Custom Options" --author "fmd perf proof" --font serif --pdf-line-numbers

sha256sum "$GOLDEN_DIR"/*.pdf > "$ARTIFACT_DIR/golden_checksums.txt"

if [ "$SELF_TEST_FAILURE" -eq 1 ]; then
  corrupt="$RUN_DIR/self-test-corrupt.pdf"
  cp "$GOLDEN_DIR/showcase.pdf" "$corrupt"
  printf 'corruption for checksum self-test\n' >> "$corrupt"
  expected="$(sha256sum "$GOLDEN_DIR/showcase.pdf" | awk '{print $1}')"
  if check_sha256_matches "$expected" "$corrupt"; then
    append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"controlled_checksum_failure_detected\",\"status\":\"fail\",\"evidence_path\":\"runs/self-test-corrupt.pdf\",\"notes\":\"corrupted file unexpectedly matched checksum\"}"
    fail "controlled checksum corruption was not detected"
  fi
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"controlled_checksum_failure_detected\",\"status\":\"pass\",\"evidence_path\":\"runs/self-test-corrupt.pdf\",\"notes\":\"deliberately corrupted PDF failed checksum comparison as expected\"}"
else
  append_jsonl "{\"type\":\"proof_obligation\",\"bead_id\":\"$BEAD_ID\",\"obligation\":\"controlled_checksum_failure_detected\",\"status\":\"not_applicable\",\"evidence_path\":\"--self-test-failure\",\"notes\":\"rerun with --self-test-failure before closing fep.6.7\"}"
fi

append_jsonl "{\"type\":\"hypothesis_evaluated\",\"hypothesis\":\"pdf_e2e_proof_covers_required_scenarios\",\"result\":\"supports\",\"evidence_path\":\"inprocess.jsonl\",\"notes\":\"all required PDF scenarios emitted perf_sample records and golden checksums\"}"

{
  echo "| Scenario | p50 | p95 | p99 | max | output bytes | objects | font subset bytes | peak RSS KB |"
  echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|"
  LC_ALL=C grep '"type":"perf_sample"' "$ARTIFACT_DIR/inprocess.jsonl" |
    sed -E 's/.*"scenario":"([^"]+)".*"output_bytes":([0-9]+).*"p50_ns":([0-9]+).*"p95_ns":([0-9]+).*"p99_ns":([0-9]+).*"max_ns":([0-9]+).*"peak_rss_kb":([0-9]+).*"pdf_object_count":([0-9]+).*"font_subset_bytes":([0-9]+).*/| `\1` | \3 ns | \4 ns | \5 ns | \6 ns | \2 | \8 | \9 | \7 |/'
} > "$ARTIFACT_DIR/BASELINE.md"

{
  echo "| Rank | Scenario | p95 ns | Output bytes | Objects | Font subset bytes |"
  echo "|---:|---|---:|---:|---:|---:|"
  LC_ALL=C grep '"type":"perf_sample"' "$ARTIFACT_DIR/inprocess.jsonl" |
    sed -E 's/.*"scenario":"([^"]+)".*"output_bytes":([0-9]+).*"p95_ns":([0-9]+).*"pdf_object_count":([0-9]+).*"font_subset_bytes":([0-9]+).*/\3\t\1\t\2\t\4\t\5/' |
    sort -nr |
    awk -F '\t' '{printf "| %d | `%s` | %s | %s | %s | %s |\n", NR, $2, $1, $3, $4, $5}'
} > "$ARTIFACT_DIR/hotspot_table.md"

cat > "$ARTIFACT_DIR/hypothesis.md" <<EOF
# Hypothesis

- pdf_e2e_proof_covers_required_scenarios: supports - every required scenario
  produced a structurally checked deterministic PDF, a perf_sample JSONL record,
  and a checksum ledger entry.
- controlled_checksum_failure_detected: $([ "$SELF_TEST_FAILURE" -eq 1 ] && printf supports || printf not_run) - use
  \`--self-test-failure\` to prove the harness catches a deliberately altered
  checksum.
EOF

append_jsonl "{\"type\":\"run_complete\",\"run_id\":\"$(json_escape "$RUN_ID")\",\"artifact_dir\":\"$(json_escape "$ARTIFACT_DIR")\",\"status\":\"pass\",\"summary_path\":\"BASELINE.md\",\"top_hotspot\":\"hotspot_table.md\",\"notes\":\"PDF performance proof completed\"}"

echo "pdf-perf-proof: ok ($ARTIFACT_DIR)"
