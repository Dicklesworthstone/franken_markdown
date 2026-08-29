#!/usr/bin/env bash
# scripts/e2e/book.sh — e2e: multi-chapter book compilation suite (epic 7tus / bead j0o4).
#
# Exercises `fmd book` through the real fmd binary:
# - Multi-chapter HTML site generation with navigation sidebar and link rewriting.
# - index.html redirect to first chapter.
# - Merged PDF book generation with outline bookmarks and chapter page breaks.
# - Manifest ordering via book.toml.
# - JSON receipt generation on stdout with --json.
#
# Usage: scripts/e2e/book.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-book}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"

# 1. Prepare sample multi-chapter book directory
BOOK_DIR="${WORK}/sample_book"
mkdir -p "$BOOK_DIR"

cat >"${BOOK_DIR}/01_intro.md" <<'EOF'
---
title=Introduction
author=Alice
---
# Welcome

Welcome to the book. See [Chapter 2](02_core.md#architecture) for details.
EOF

cat >"${BOOK_DIR}/02_core.md" <<'EOF'
# Core Concepts

## Architecture

This chapter explains the system. Check out [Conclusion](03_summary.md).
EOF

cat >"${BOOK_DIR}/03_summary.md" <<'EOF'
# Conclusion

Final thoughts. Back to [Start](01_intro.md).
EOF

# --- fmd book default (both HTML site + PDF) with --json ---
OUT_DIR="${WORK}/dist"
e2e_run "fmd book: HTML site and PDF generation with JSON receipt" -- \
  "$E2E_BIN" book "$BOOK_DIR" --out-dir "$OUT_DIR" --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"ok":true'
e2e_expect_stdout_contains '"command":"book"'
e2e_expect_stdout_contains '"chapters":3'
e2e_expect_stdout_contains '"unresolved_links":0'

# Verify generated files
e2e_assert "01_intro.html generated" -- test -f "${OUT_DIR}/01_intro.html"
e2e_assert "02_core.html generated" -- test -f "${OUT_DIR}/02_core.html"
e2e_assert "03_summary.html generated" -- test -f "${OUT_DIR}/03_summary.html"
e2e_assert "index.html redirect generated" -- test -f "${OUT_DIR}/index.html"
e2e_assert "sample_book.pdf generated" -- test -f "${OUT_DIR}/sample_book.pdf"

# Verify link rewriting in HTML
e2e_assert "link rewritten in 01_intro.html" -- \
  grep -q 'href="02_core.html#architecture"' "${OUT_DIR}/01_intro.html"

# Verify navigation sidebar injection
e2e_assert "navigation sidebar in 01_intro.html" -- \
  grep -q '<nav class="fmd-book-nav"' "${OUT_DIR}/01_intro.html"

# Verify index.html redirect
e2e_assert "index.html redirects to 01_intro.html" -- \
  grep -q 'url=01_intro.html' "${OUT_DIR}/index.html"

# --- fmd book with manifest ordering ---
MANIFEST_BOOK_DIR="${WORK}/manifest_book"
MANIFEST_OUT_DIR="${WORK}/manifest_site"
mkdir -p "$MANIFEST_BOOK_DIR"

cat >"${MANIFEST_BOOK_DIR}/z_last.md" <<'EOF'
# Last Chapter
Content.
EOF

cat >"${MANIFEST_BOOK_DIR}/a_first.md" <<'EOF'
# First Chapter
Content.
EOF

cat >"${MANIFEST_BOOK_DIR}/book.toml" <<'EOF'
title = "Custom Manifest Book"
order = ["z_last.md", "a_first.md"]
EOF

# --- fmd book with transclusion ---
TRANSCLUDE_BOOK_DIR="${WORK}/transclude_book"
TRANSCLUDE_OUT_DIR="${WORK}/transclude_site"
mkdir -p "$TRANSCLUDE_BOOK_DIR"

cat >"${TRANSCLUDE_BOOK_DIR}/shared_snippet.md" <<'EOF'
This is an included shared fragment.
EOF

cat >"${TRANSCLUDE_BOOK_DIR}/01_chapter.md" <<'EOF'
# Chapter with Include

{{#include shared_snippet.md}}

Post-include text.
EOF

e2e_run "fmd book: include transclusion in book chapter" -- \
  "$E2E_BIN" book "$TRANSCLUDE_BOOK_DIR" --out-dir "$TRANSCLUDE_OUT_DIR" --to html --json
e2e_expect_exit 0
e2e_expect_stdout_contains '"ok":true'
e2e_assert "transcluded content present in rendered chapter" -- \
  grep -q 'This is an included shared fragment.' "${TRANSCLUDE_OUT_DIR}/01_chapter.html"

e2e_finish
exit $?
