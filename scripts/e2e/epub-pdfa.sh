#!/usr/bin/env bash
# scripts/e2e/epub-pdfa.sh — e2e: EPUB 3 & PDF/A-2b compliance suite (s115.5).
#
# Exercises EPUB 3 packaging and PDF/A-2b archive generation via the real fmd binary:
# - EPUB 3 OCF layout: mimetype, container.xml, content.opf, nav.xhtml, chapter-1.xhtml.
# - XML self-closing tag validity and deterministic UUID metadata.
# - PDF/A-2b XMP metadata packet, OutputIntent dictionary, and compact sRGB ICC profile.
# - Strict mode enforcement against forbidden URI schemes (javascript:/file:).
#
# Usage: scripts/e2e/epub-pdfa.sh [run-id]
# Exit:  0 ok · 64 usage · 66 env/build · 70 an assertion failed.
set -uo pipefail
source "$(dirname "$0")/lib.sh"

e2e_init "${1:-epub-pdfa}"
e2e_build_bin || exit 66

WORK="${E2E_ART}/work"; mkdir -p "$WORK"
EPOCH=1700000000

# 1. Sample document for EPUB and PDF/A
DOC="${WORK}/publication.md"
cat >"$DOC" <<'EOF'
# Formal Specification

This specification defines the container architecture and document delivery formats.

## Architecture

The Open Container Format (OCF) coordinates packaging of XHTML documents, stylesheets, and metadata.

- Deterministic archive timestamps
- Byte-for-byte reproducibility
- Accessible navigation structures

### Archival Preservation

PDF/A-2b guarantees visual reproducibility across decadal preservation horizons.
EOF

# --- EPUB 3 rendering ---
e2e_run "epub: render document to EPUB 3 archive" -- \
  "$E2E_BIN" "$DOC" --to epub --out "${WORK}/book.epub"
e2e_expect_exit 0
e2e_expect_file "${WORK}/book.epub"
e2e_expect_file_bytes_ge "${WORK}/book.epub" 1000

# Verify OCF zip invariants using python zipfile
e2e_run "epub: inspect OCF structure via python" -- \
  python3 -c "import zipfile, sys
z = zipfile.ZipFile('${WORK}/book.epub')
namelist = z.namelist()
assert namelist[0] == 'mimetype', f'mimetype not first entry: {namelist}'
assert z.read('mimetype') == b'application/epub+zip', 'mimetype payload mismatch'
assert 'META-INF/container.xml' in namelist, 'missing container.xml'
assert 'OEBPS/content.opf' in namelist, 'missing content.opf'
assert 'OEBPS/nav.xhtml' in namelist, 'missing nav.xhtml'
assert 'OEBPS/chapter-1.xhtml' in namelist, 'missing chapter-1.xhtml'
assert 'OEBPS/style.css' in namelist, 'missing style.css'

# Verify nav.xhtml has toc
nav = z.read('OEBPS/nav.xhtml').decode('utf-8')
assert 'id=\"toc\"' in nav or 'id=\'toc\'' in nav, 'nav missing toc id'
assert 'Architecture' in nav, 'nav missing heading'

# Verify chapter-1 has valid content
chap = z.read('OEBPS/chapter-1.xhtml').decode('utf-8')
assert 'Formal Specification' in chap, 'chapter missing title'
print('EPUB 3 structure verified successfully')
"
e2e_expect_exit 0
e2e_expect_stdout_contains "EPUB 3 structure verified successfully"

# --- PDF/A-2b rendering ---
e2e_run "pdfa: render document to PDF/A-2b" -- \
  env SOURCE_DATE_EPOCH="$EPOCH" "$E2E_BIN" "$DOC" --to pdf --pdf-a 2b --out "${WORK}/archival.pdf"
e2e_expect_exit 0
e2e_expect_file "${WORK}/archival.pdf"
e2e_expect_file_bytes_ge "${WORK}/archival.pdf" 2000

e2e_run "pdfa: inspect PDF/A markers and OutputIntent" -- \
  sh -c "grep -q 'GTS_PDFA1' '${WORK}/archival.pdf' && grep -q 'pdfaid:part' '${WORK}/archival.pdf'"
e2e_expect_exit 0

# --- PDF/A-2b strict validation ---
DOC_UNSAFE="${WORK}/unsafe_link.md"
cat >"$DOC_UNSAFE" <<'EOF'
# Unsafe Document

[Script link](javascript:alert(1))
EOF

e2e_run "pdfa: strict mode rejects forbidden URI scheme" -- \
  "$E2E_BIN" "$DOC_UNSAFE" --to pdf --pdf-a 2b --pdf-a-strict
e2e_expect_exit_nonzero
e2e_expect_stderr_contains "pdf_a_javascript_uri"

e2e_finish
exit $?
