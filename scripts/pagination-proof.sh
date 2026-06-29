#!/usr/bin/env bash
# pagination-proof.sh — e2e proof of PDF keep-with-next pagination (bead mwm.7).
#
# Renders a multi-page document, extracts the per-page structural-tag map from the
# PDF (decompressing FlateDecode content streams), logs it, and asserts the
# keep-with-next invariant that a heading is never the last block on a page while
# content follows on the next page. Also proves byte-determinism.
#
# Usage: scripts/pagination-proof.sh [run-id]
set -euo pipefail
cd "$(dirname "$0")/.."
RUN_ID="${1:-local}"
ART="tests/artifacts/pagination/${RUN_ID}"
mkdir -p "$ART"
MAP="${ART}/page-map.txt"
log() { printf '%s\n' "$*" | tee -a "$MAP"; }
: >"$MAP"

cargo build --release --quiet
BIN="${CARGO_TARGET_DIR:-target}/release/fmd"
[ -x "$BIN" ] || BIN="target/release/fmd"

# A multi-page document: many heading+prose+table sections, sized so headings and
# captioned tables land at varied page positions (some near page boundaries).
DOC="${ART}/doc.md"
{
  for s in $(seq 1 12); do
    echo "## Section ${s}"
    echo
    for p in 1 2 3; do echo "Paragraph ${p} of section ${s} with enough words to wrap across a couple of lines on a Letter page and exercise the vertical page builder."; echo; done
    echo "Caption for table ${s}"
    echo
    echo "| Key | Value |"
    echo "|---|---:|"
    echo "| a | ${s} |"
    echo "| b | ${s}${s} |"
    echo
  done
} >"$DOC"

PDF="${ART}/doc.pdf"
SOURCE_DATE_EPOCH=1700000000 "$BIN" "$DOC" --to pdf --out "$PDF" >/dev/null 2>&1
log "=== pagination-proof run=${RUN_ID} pdf=$(wc -c <"$PDF") bytes ==="

# Extract + assert the per-page tag map with an embedded parser.
python3 - "$PDF" "$MAP" <<'PY'
import re, sys, zlib
pdf = open(sys.argv[1], "rb").read()
mapfile = open(sys.argv[2], "a")
def emit(s): print(s); mapfile.write(s + "\n")

# Collect page content streams in document order. fmd compresses page streams
# (>=4096 bytes) with FlateDecode; smaller ones are raw.
pages = []
# Match an OPENING `stream` only; the negative lookbehind avoids matching the
# `stream` inside `endstream`.
for m in re.finditer(rb"(?<!end)stream\r?\n", pdf):
    start = m.end()
    end = pdf.find(b"endstream", start)
    if end == -1:
        continue
    body = pdf[start:end]
    try:
        body = zlib.decompress(body)
    except Exception:
        pass
    if b"BT /F" in body:  # a page content stream
        tags = re.findall(rb"/(\w+) <</MCID", body)
        pages.append([t.decode() for t in tags])

emit(f"pages: {len(pages)}")
headings = {"H", "H1", "H2", "H3", "H4", "H5", "H6"}
stranded = []
for i, tags in enumerate(pages):
    emit(f"  page {i}: {tags}")
    # Heading keep-with-next: a heading must never be the LAST block on a page
    # when another page follows (it would be stranded from its content).
    if i < len(pages) - 1 and tags and tags[-1] in headings:
        stranded.append((i, tags[-1]))

if stranded:
    emit(f"FAIL: stranded heading(s) at page bottom: {stranded}")
    sys.exit(1)
emit("ok: no heading stranded at a page bottom (keep-with-next holds).")
PY
rc=$?

# Determinism.
PDF2="${ART}/doc.2.pdf"
SOURCE_DATE_EPOCH=1700000000 "$BIN" "$DOC" --to pdf --out "$PDF2" >/dev/null 2>&1
if cmp -s "$PDF" "$PDF2"; then
  log "determinism: PDF byte-identical across runs (sha256 $(sha256sum "$PDF" | cut -c1-16))"
else
  log "determinism: FAILED — PDF differs across runs"; rc=1
fi

log ""
if [ "$rc" -eq 0 ]; then
  log "pagination-proof: ok"
else
  log "pagination-proof: FAILED"
fi
exit "$rc"
