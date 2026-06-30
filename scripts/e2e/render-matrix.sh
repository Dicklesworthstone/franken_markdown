#!/usr/bin/env bash
# scripts/e2e/render-matrix.sh — e2e: full render matrix (bead grn.4.3).
#
# Exercises {file | stdin | --text} x {html | pdf | both} x {sans | serif | --css}
# x {--out path | --out - | omitted} against the real fmd binary, asserting REAL
# output: HTML has the expected structure, PDF starts with %PDF- / ends with %%EOF
# and is non-trivial, byte sizes are sane, themes actually change the bytes, a
# custom stylesheet replaces the default, and repeated renders are byte-identical.
# Absorbs scripts/cli-output-contract.sh's output-path matrix.
#
# Usage: scripts/e2e/render-matrix.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-render-matrix}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
DOC="${WORK}/doc.md"
printf '# Heading\n\nProse with **bold**, *italic*, and `code`.\n\n| A | B |\n|---|--:|\n| 1 | 2 |\n\n> quote\n\n```rust\nfn main() {}\n```\n' >"$DOC"
CSS="${WORK}/custom.css"
printf '/* CUSTOM-MARKER-7F3 */\nbody { color: #123456; }\n' >"$CSS"

# Pin PDF metadata dates so re-renders are byte-identical.
EPOCH=1700000000

# --- input dimension x html -------------------------------------------------
e2e_run "file x html x sans x --out" -- "$E2E_BIN" "$DOC" --to html --out "${WORK}/f.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/f.html"
e2e_expect_file_contains "${WORK}/f.html" "<main"
e2e_expect_file_contains "${WORK}/f.html" "Heading"
e2e_expect_file_bytes_ge "${WORK}/f.html" 2000

E2E_STDIN="$DOC"
e2e_run "stdin x html x sans x omitted (stdout)" -- "$E2E_BIN" - --to html
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"
e2e_expect_stdout_contains "Heading"

e2e_run "--text x html x sans x stdout" -- "$E2E_BIN" --text '# T' --to html --out -
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"

# --- theme dimension: sans vs serif vs custom css ---------------------------
e2e_run "html x sans (baseline)" -- "$E2E_BIN" "$DOC" --to html --out "${WORK}/sans.html"
e2e_expect_exit 0
e2e_run "html x serif" -- "$E2E_BIN" "$DOC" --font serif --to html --out "${WORK}/serif.html"
e2e_expect_exit 0
e2e_run "theme actually changes bytes (sans != serif)" -- \
  sh -c "! cmp -s '${WORK}/sans.html' '${WORK}/serif.html'"
e2e_expect_exit 0

e2e_run "html x --css replaces the stylesheet" -- "$E2E_BIN" "$DOC" --css "$CSS" --to html --out "${WORK}/css.html"
e2e_expect_exit 0
e2e_expect_file_contains "${WORK}/css.html" "CUSTOM-MARKER-7F3"
# A full stylesheet replacement drops the default embedded @font-face block.
e2e_run "custom css dropped the default font-face" -- \
  sh -c "! grep -q '@font-face' '${WORK}/css.html'"
e2e_expect_exit 0

# --- format dimension: pdf + both -------------------------------------------
e2e_run "file x pdf x sans x --out" -- env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC" --to pdf --out "${WORK}/f.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/f.pdf"
e2e_expect_file_bytes_ge "${WORK}/f.pdf" 1000
e2e_run "pdf magic + trailer" -- sh -c "head -c5 '${WORK}/f.pdf' | grep -q '%PDF-' && tail -c 8 '${WORK}/f.pdf' | grep -q '%%EOF'"
e2e_expect_exit 0

E2E_STDIN="$DOC"
e2e_run "stdin x pdf x sans x --out" -- env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" - --to pdf --out "${WORK}/stdin.pdf"
e2e_expect_exit 0
e2e_run "stdin pdf magic" -- sh -c "head -c5 '${WORK}/stdin.pdf' | grep -q '%PDF-'"
e2e_expect_exit 0

e2e_run "--text x both x sans x --out" -- "$E2E_BIN" --text '# B' --to both --out "${WORK}/both.html"
e2e_expect_exit 0
e2e_expect_file "${WORK}/both.html"
e2e_expect_file "${WORK}/both.pdf"

# --- output-path rules (absorbs cli-output-contract.sh) ---------------------
e2e_run "html --out - streams to stdout" -- "$E2E_BIN" "$DOC" --to html --out -
e2e_expect_exit 0
e2e_expect_stdout_contains "<main"

e2e_run "pdf --out - is refused" -- "$E2E_BIN" "$DOC" --to pdf --out -
e2e_expect_exit 64
e2e_expect_no_file "${WORK}/-"

e2e_run "both --out - is refused" -- "$E2E_BIN" "$DOC" --to both --out -
e2e_expect_exit 64

# Omitted --out for pdf derives the path from the input stem (doc.md -> doc.pdf).
E2E_RUN_CWD="$WORK"
e2e_run "pdf omitted --out derives doc.pdf" -- env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" doc.md --to pdf
e2e_expect_exit 0
e2e_expect_file "${WORK}/doc.pdf"
E2E_RUN_CWD="$E2E_REPO_ROOT"

# --- determinism: identical inputs yield byte-identical outputs -------------
e2e_run "html determinism render A" -- "$E2E_BIN" "$DOC" --to html --out "${WORK}/det-a.html"
e2e_expect_exit 0
e2e_run "html determinism render B" -- "$E2E_BIN" "$DOC" --to html --out "${WORK}/det-b.html"
e2e_expect_exit 0
e2e_run "html is byte-identical across runs" -- cmp -s "${WORK}/det-a.html" "${WORK}/det-b.html"
e2e_expect_exit 0

e2e_run "pdf determinism render A" -- env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC" --to pdf --out "${WORK}/det-a.pdf"
e2e_expect_exit 0
e2e_run "pdf determinism render B" -- env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC" --to pdf --out "${WORK}/det-b.pdf"
e2e_expect_exit 0
e2e_run "pdf is byte-identical with pinned SOURCE_DATE_EPOCH" -- cmp -s "${WORK}/det-a.pdf" "${WORK}/det-b.pdf"
e2e_expect_exit 0

e2e_finish
exit $?
