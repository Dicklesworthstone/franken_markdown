#!/usr/bin/env bash
# check-pdf-ua.sh — optional external PDF/UA spot-check via veraPDF (bead grn.5.6).
#
# A DEV-ONLY, optional cross-check: it runs the external veraPDF validator against
# a real fmd-rendered tagged PDF to independently confirm the structure tree is
# well-formed. veraPDF is NEVER a build/runtime/test dependency of the engine (that
# would violate the clean-room policy); this gate SKIPS cleanly when veraPDF is not
# installed, so it can live in CI without becoming a hard dependency.
#
# fmd's tagging is intentionally PARTIAL (H1-H3, lists, tables, blockquotes,
# figures, links — see docs/PDF_ACCESSIBILITY.md; H4-H6 + cell-id linkage are
# roadmap), so full PDF/UA-1 *conformance* is not yet expected. The spot-check
# therefore asserts veraPDF can PARSE the document and recognizes its structure
# tree (no fatal/parse error), and records the full conformance report for review
# rather than hard-failing on the documented, roadmap-tracked rule gaps.
#
# Usage:
#   scripts/check-pdf-ua.sh [run-id]   # render + spot-check (skips if veraPDF absent)
#   scripts/check-pdf-ua.sh --self-test
#
# Exit: 0 ok or skipped · 2 missing prerequisite (cargo) · 70 veraPDF could not
#       parse the PDF / no structure tree (a real regression).
set -uo pipefail
cd "$(dirname "$0")/.." || exit
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$PWD/target/fmd-checks}"
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh

RUN_ID="local"
case "${1:-}" in
  --self-test) RUN_ID="selftest" ;;
  --help|-h)   sed -n '2,28p' "$0"; exit 0 ;;
  "")          ;;
  *)           RUN_ID="$1" ;;
esac
fmd_validate_run_id "check-pdf-ua" "$RUN_ID"

ART="tests/artifacts/pdf-ua/${RUN_ID}"
mkdir -p "$ART"

if ! command -v verapdf >/dev/null 2>&1; then
  echo "check-pdf-ua: veraPDF not installed — SKIPPING the optional external PDF/UA spot-check."
  echo "check-pdf-ua: (this is a dev-only cross-check; fmd's tagged-PDF structure is also"
  echo "             validated internally by tests/pdf_test.rs and tests/parser_metamorphic.rs.)"
  echo "check-pdf-ua: install veraPDF (https://verapdf.org) to enable this gate."
  # In --self-test mode the skip path IS the thing under test on a veraPDF-less host.
  exit 0
fi

command -v cargo >/dev/null 2>&1 || { echo "check-pdf-ua: cargo not found" >&2; exit 2; }

echo "check-pdf-ua: building fmd + rendering a representative tagged PDF"
cargo build --release --quiet --bin fmd || { echo "check-pdf-ua: fmd build failed" >&2; exit 2; }
BIN="${CARGO_TARGET_DIR}/release/fmd"
DOC="$ART/sample.md"
cat >"$DOC" <<'MD'
# Accessible Document

A paragraph with **strong** and *emphasis*.

## Section

- item one
- item two

| Name | Value |
|---|--:|
| alpha | 1 |

> A blockquote.
MD
PDF="$ART/sample.pdf"
SOURCE_DATE_EPOCH=1700000000 "$BIN" "$DOC" --to pdf --out "$PDF" || { echo "check-pdf-ua: render failed" >&2; exit 2; }

echo "check-pdf-ua: running veraPDF (PDF/UA-1 flavour) — report -> $ART/report.txt"
verapdf --flavour ua1 "$PDF" >"$ART/report.txt" 2>&1 || true   # conformance failures are informational

# A real regression is veraPDF failing to PARSE the PDF or finding no structure
# tree; documented rule-level gaps are not hard failures here.
if grep -qiE "could not be parsed|not a valid pdf|exception" "$ART/report.txt"; then
  echo "check-pdf-ua: FAILED — veraPDF could not parse the rendered PDF (see $ART/report.txt)." >&2
  exit 70
fi

# Beyond "can parse", require that veraPDF actually exercised PDF/UA checks and
# that SOME passed. A document whose tagging regressed to nothing (no structure
# tree, zero satisfied rules) must fail here instead of passing silently — that
# is the difference between a real spot-check and a no-op.
PASSED="$(grep -oiE 'passedChecks[^0-9]*[0-9]+' "$ART/report.txt" | grep -oE '[0-9]+' | head -1)"
PASSED="${PASSED:-0}"
if [ "$PASSED" -le 0 ]; then
  # Some veraPDF builds report compliance differently; accept an explicit
  # compliant marker as an alternative positive signal before failing.
  if ! grep -qiE "isCompliant=.true|\"compliant\"[[:space:]]*:[[:space:]]*true" "$ART/report.txt"; then
    echo "check-pdf-ua: FAILED — veraPDF satisfied 0 PDF/UA checks; the structure tree" >&2
    echo "             appears to have regressed (see $ART/report.txt)." >&2
    exit 70
  fi
fi
echo "check-pdf-ua: ok — veraPDF parsed the tagged PDF and ${PASSED} PDF/UA check(s) passed."
echo "check-pdf-ua: (partial PDF/UA conformance is expected; see docs/PDF_ACCESSIBILITY.md for the roadmap.)"
exit 0
