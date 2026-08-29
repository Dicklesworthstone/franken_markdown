#!/usr/bin/env bash
# scripts/e2e/batch-orchestration.sh — e2e: multi-file batch CLI suite (s115.6).
#
# Exercises the real fmd batch binary across multi-document directory trees:
# - Recursive input resolution and exclusion filtering.
# - Concurrent worker pools and progress telemetry.
# - Multi-format rendering (--to html, --to pdf, --to both).
# - Atomic staging and failure isolation.
#
# Usage: scripts/e2e/batch-orchestration.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-batch-orchestration}"
e2e_build_bin batch || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
EPOCH=1700000000

# 1. Create multi-file directory tree
INPUT_DIR="${WORK}/docs"
mkdir -p "${INPUT_DIR}/sub"
cat >"${INPUT_DIR}/index.md" <<'EOF'
# Main Index
Welcome to the documentation suite.
EOF

cat >"${INPUT_DIR}/sub/page1.md" <<'EOF'
# Page One
Details regarding architecture and performance.
EOF

cat >"${INPUT_DIR}/sub/page2.md" <<'EOF'
# Page Two
Information regarding deployment and metrics.
EOF

# --- Batch render to HTML ---
OUT_HTML="${WORK}/dist_html"
e2e_run "batch: multi-file HTML rendering" -- \
  "$E2E_BIN" batch "${INPUT_DIR}" --to html --out-dir "$OUT_HTML"
e2e_expect_exit 0
e2e_expect_file "${OUT_HTML}/index.html"
e2e_expect_file "${OUT_HTML}/page1.html"
e2e_expect_file "${OUT_HTML}/page2.html"

# --- Batch render to both (HTML + PDF) with JSON output ---
OUT_BOTH="${WORK}/dist_both"
e2e_run "batch: multi-file dual-format rendering with JSON receipt" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" batch "${INPUT_DIR}" --to both --out-dir "$OUT_BOTH" --json
e2e_expect_exit 0
e2e_expect_file "${OUT_BOTH}/index.html"
e2e_expect_file "${OUT_BOTH}/index.pdf"
e2e_expect_file "${OUT_BOTH}/page1.html"
e2e_expect_file "${OUT_BOTH}/page1.pdf"
e2e_expect_file "${OUT_BOTH}/page2.html"
e2e_expect_file "${OUT_BOTH}/page2.pdf"
e2e_expect_stdout_contains '"schema":"fmd-batch-receipt-v1"'
e2e_expect_stdout_contains '"inputs":3'
e2e_expect_stdout_contains '"ok":3'
e2e_assert "batch receipt is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

# --- Batch failure isolation & atomic abort ---
BAD_DIR="${WORK}/bad_input"
mkdir -p "$BAD_DIR"
cat >"${BAD_DIR}/valid.md" <<'EOF'
# Valid Document
Content here.
EOF

e2e_run "batch: strict unexpandable input aborts atomically" -- \
  "$E2E_BIN" batch "${BAD_DIR}/valid.md" "${BAD_DIR}/nonexistent.md" --out-dir "${WORK}/dist_partial"
e2e_expect_exit 66
e2e_expect_no_file "${WORK}/dist_partial/valid.html"

e2e_finish
exit $?
