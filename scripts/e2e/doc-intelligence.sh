#!/usr/bin/env bash
# scripts/e2e/doc-intelligence.sh — e2e: document intelligence, diff & search index suite (s115.4).
#
# Exercises document intelligence features through the real fmd binary:
# - `fmd stats` and `fmd stats --json` (readability scores, telemetry, outline).
# - `fmd diff` (semantic AST visual diffing to HTML and JSON).
# - `fmd search-index --json` (document-order deep linkable search index).
#
# Usage: scripts/e2e/doc-intelligence.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-doc-intelligence}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# 1. Prepare sample document for stats and search index
DOC_SAMPLE="${WORK}/sample.md"
cat >"$DOC_SAMPLE" <<'EOF'
# Document Intelligence Guide

The intelligence subsystem calculates reading velocity, sentence difficulty, and structural health.

## Readability Metrics

Flesch reading ease provides an estimate of text accessibility for various reading grade levels.

- Fast evaluation
- Automated telemetry
- Zero third-party dependencies

### Technical Findings

All internal anchors must correspond to declared heading slugs.
EOF

# --- fmd stats ---
e2e_run "doc-stats: human text telemetry output" -- \
  "$E2E_BIN" stats "$DOC_SAMPLE"
e2e_expect_exit 0
e2e_expect_stdout_contains "Words:"
e2e_expect_stdout_contains "Reading time:"
e2e_expect_stdout_contains "Flesch Reading Ease:"
e2e_expect_stdout_contains "Outline:"

e2e_run "doc-stats: JSON telemetry output" -- \
  "$E2E_BIN" stats "$DOC_SAMPLE" --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"schema":"fmd-document-stats-v1"'
e2e_expect_stdout_contains '"flesch_reading_ease"'
e2e_expect_stdout_contains '"outline"'
e2e_assert "stats --json is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

# --- fmd search-index ---
e2e_run "search-index: JSON output schema and entries" -- \
  "$E2E_BIN" search-index "$DOC_SAMPLE" --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"schema":"fmd-search-index-v1"'
e2e_expect_stdout_contains '"kind":"heading"'
e2e_expect_stdout_contains '"kind":"paragraph"'
e2e_expect_stdout_contains '"anchor":"readability-metrics"'
e2e_assert "search-index --json is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

# 2. Prepare documents for semantic diffing
DOC_V1="${WORK}/v1.md"
cat >"$DOC_V1" <<'EOF'
# Release Notes

Welcome to version 1.0 of the application.

- Initial parser implementation
- Basic HTML export
- Experimental PDF layout
EOF

DOC_V2="${WORK}/v2.md"
cat >"$DOC_V2" <<'EOF'
# Release Notes (v2.0)

Welcome to version 2.0 of the high-performance renderer.

- Production-grade parser implementation
- Polished HTML and EPUB export
- Next-level SOTA PDF layout with Knuth-Plass breaking
- Built-in visual diff engine
EOF

# --- fmd diff to HTML ---
e2e_run "diff: generate visual HTML diff report" -- \
  "$E2E_BIN" diff "$DOC_V1" "$DOC_V2" --html --out "${WORK}/diff.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/diff.html"
e2e_expect_file_contains "${WORK}/diff.html" "Comparing"
e2e_expect_file_contains "${WORK}/diff.html" "diff-badge-ins"
e2e_expect_file_contains "${WORK}/diff.html" "diff-badge-del"
e2e_expect_file_contains "${WORK}/diff.html" "Similarity:"

# --- fmd diff to JSON ---
e2e_run "diff: generate machine-readable JSON diff report" -- \
  "$E2E_BIN" diff "$DOC_V1" "$DOC_V2" --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"schema":"fmd-diff-v1"'
e2e_expect_stdout_contains '"similarity_ratio"'
e2e_expect_stdout_contains '"words_inserted"'
e2e_expect_stdout_contains '"words_deleted"'
e2e_assert "diff --json is valid JSON" -- \
  sh -c "python3 -c 'import json,sys; json.load(open(sys.argv[1]))' '$E2E_LAST_STDOUT'"

e2e_finish
exit $?
