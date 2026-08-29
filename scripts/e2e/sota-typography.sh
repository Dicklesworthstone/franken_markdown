#!/usr/bin/env bash
# scripts/e2e/sota-typography.sh — e2e: SOTA typography rendering & pagination (s115.2).
#
# Exercises the real fmd binary on documents utilizing advanced typography features:
# Knuth-Plass line breaking, ragged silhouette penalties, river penalties,
# optical kerning, convex table balancing, baseline grid alignment, and 2D optimal
# pagination. Verifies that real PDF artifacts are generated without margin overflow,
# and verifies the text layer via `fmd verify`.
#
# Usage: scripts/e2e/sota-typography.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-sota-typography}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
EPOCH=1700000000

# 1. Complex typography document: multi-level headings, dense paragraphs, tables, blockquotes
DOC_COMPLEX="${WORK}/complex_typography.md"
cat >"$DOC_COMPLEX" <<'EOF'
# Advanced Typographic Layout

High quality digital typography requires Knuth-Plass paragraph breaking,
continuous optical kerning, and microtypographic expansion.

## The Harmony of Negative Space

Microtypography achieves optical margin alignment through dynamic adjustment
of character shapes and inter-word springs.

> Good typography is invisible. When the baseline grid aligns,
> reading becomes effortless.

| Parameter | Default | Min Bound | Max Bound | Target Metric |
|:----------|:-------:|:---------:|:---------:|--------------:|
| Grid Pitch | 12.0 pt | 8.0 pt | 24.0 pt | Vertical Rhythm |
| Spring Tension | 1.00 | 0.50 | 2.50 | Elasticity |
| River Penalty | 500 | 100 | 2000 | White Space |
| Optical Kerning | Active | N/A | N/A | Quadrature |

### Mathematical Elegance

Euler's identity represents structural elegance:
$$e^{i\pi} + 1 = 0$$

The Knuth-Plass objective function minimizes total demerits:
$$D = \sum_{k=1}^m (d_k + \gamma_k)^2$$
EOF

# --- Render complex typography to PDF ---
e2e_run "typography: render complex document to PDF" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC_COMPLEX" --to pdf --out "${WORK}/complex.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/complex.pdf"
e2e_expect_file_bytes_ge "${WORK}/complex.pdf" 2000
e2e_run "typography: PDF magic and trailer check" -- \
  sh -c "head -c5 '${WORK}/complex.pdf' | grep -q '%PDF-' && tail -c 8 '${WORK}/complex.pdf' | grep -q '%%EOF'"
e2e_expect_exit 0

# --- Verify PDF via fmd verify (text layer & overflow audit) ---
e2e_run "typography: fmd verify clean report" -- \
  "$E2E_BIN" verify "$DOC_COMPLEX"
e2e_expect_exit 0
e2e_expect_stdout_contains "clean"

e2e_run "typography: fmd verify --json schema audit" -- \
  "$E2E_BIN" verify "$DOC_COMPLEX" --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"verdict":"clean"'
e2e_expect_stdout_contains '"anchors"'
e2e_expect_stdout_contains '"pages"'

# 2. Large balanced table document
DOC_TABLE="${WORK}/balanced_table.md"
cat >"$DOC_TABLE" <<'EOF'
# Constrained Convex Table Balancing

| Short Code | Medium Column Heading | Very Long Explanatory Column Text With Significant Word Count | Numeric Value |
|:-----------|:----------------------|:--------------------------------------------------------------|--------------:|
| A-01 | Standard Component | This component handles baseline alignment and elastic spring distributions. | 94.25 |
| B-02 | Secondary Processor | Processes optical kerning pairs using bounding box area quadrature integrals. | 182.50 |
| C-03 | Tertiary Allocator | Allocates table column widths by minimizing non-linear text wrap penalties. | 310.00 |
EOF

e2e_run "typography: balanced table render to PDF" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC_TABLE" --to pdf --out "${WORK}/table.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/table.pdf"

e2e_run "typography: verify table document has no overflow" -- \
  "$E2E_BIN" verify "$DOC_TABLE"
e2e_expect_exit 0
e2e_expect_stdout_contains "clean"

# 3. Drop cap and heading silhouette document
DOC_DROPCAP="${WORK}/dropcap_flow.md"
cat >"$DOC_DROPCAP" <<'EOF'
# The Art of Typesetting

Typography is the art and technique of arranging type to make written language legible, readable, and appealing when displayed. The arrangement of type involves selecting typefaces, point sizes, line lengths, line-spacing, and letter-spacing, and adjusting the space between pairs of letters.
EOF

e2e_run "typography: drop cap document to PDF" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC_DROPCAP" --to pdf --out "${WORK}/dropcap.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/dropcap.pdf"

e2e_finish
exit $?
