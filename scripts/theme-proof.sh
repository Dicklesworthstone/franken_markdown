#!/usr/bin/env bash
# theme-proof.sh — e2e proof that PDF and HTML colors derive from the SAME shared
# theme tokens (bead mwm.6, "one theme model" doctrine).
#
# For each theme it renders a small probe document (kept under the PDF page-stream
# compression threshold so colors stay inspectable) to HTML and PDF, then logs a
# per-token, per-surface RGB table and asserts cross-surface agreement. It also
# proves byte-determinism across repeat renders. Artifacts + the ledger land under
# tests/artifacts/theme/<run-id>/.
#
# Usage: scripts/theme-proof.sh [run-id]
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/validate-run-id.sh
source scripts/validate-run-id.sh
RUN_ID="${1:-local}"
fmd_validate_run_id "theme-proof" "$RUN_ID"
ART="tests/artifacts/theme/${RUN_ID}"
mkdir -p "$ART"
LEDGER="${ART}/ledger.txt"
: >"$LEDGER"

log() { printf '%s\n' "$*" | tee -a "$LEDGER"; }

log "=== theme-proof run=${RUN_ID} $(uname -s) ==="

# Build the release binary once.
cargo build --release --quiet
BIN="${CARGO_TARGET_DIR:-target}/release/fmd"
[ -x "$BIN" ] || BIN="target/release/fmd"
log "binary: $BIN"

# Small probe doc: link + blockquote + inline/fenced code + table + thematic break.
PROBE="${ART}/probe.md"
# shellcheck disable=SC2016 # The inline-code backticks are intentional literal fixture text.
printf '# Heading One\n\n> quoted text\n>\n> more quote\n\nBody with a [link](https://example.com) and `inline code`.\n\n| A | B |\n|---|--:|\n| 1 | 2 |\n| 3 | 4 |\n\n---\n' >"$PROBE"

# token hex -> "r.rrr g.ggg b.bbb" exactly as the PDF writer formats it.
pdf_rgb() {
  python3 - "$1" <<'PY'
import sys
h = sys.argv[1].lstrip('#')
r, g, b = (int(h[i:i+2], 16) / 255.0 for i in (0, 2, 4))
print(f"{r:.3f} {g:.3f} {b:.3f}")
PY
}

# Theme-token coverage ledger: (label, hex, pdf-needle, html-needle).
# Default light palette (matches src/theme.rs ThemeColors::light()).
declare -A TOKENS=(
  [link_accent]=0969da
  [body_fg]=1f2328
  [code_quote_bg_subtle]=f6f8fa
  [blockquote_bar]=d1d9e0
  [table_stripe]=f6f8fa
  [heading_table_rule_border_muted]=e6e8eb
)

fail=0
for theme in sans serif; do
  HTML="${ART}/probe-${theme}.html"
  PDF="${ART}/probe-${theme}.pdf"
  SOURCE_DATE_EPOCH=1700000000 "$BIN" "$PROBE" --font "$theme" --out "$HTML" >/dev/null 2>&1
  SOURCE_DATE_EPOCH=1700000000 "$BIN" "$PROBE" --font "$theme" --to pdf --out "$PDF" >/dev/null 2>&1
  log ""
  log "--- theme=${theme} ---"
  log "$(printf '%-34s | %-8s | %-18s | %-7s | %-7s | %s' token hex pdf_rgb in_pdf in_html match)"
  for label in "${!TOKENS[@]}"; do
    hex="#${TOKENS[$label]}"
    rgb="$(pdf_rgb "$hex")"
    if grep -qF "$rgb" "$PDF"; then in_pdf=yes; else in_pdf=NO; fi
    if grep -qiF "$hex" "$HTML"; then in_html=yes; else in_html=NO; fi
    if [ "$in_pdf" = yes ] && [ "$in_html" = yes ]; then match=ok; else match=MISMATCH; fail=1; fi
    log "$(printf '%-34s | %-8s | %-18s | %-7s | %-7s | %s' "$label" "$hex" "$rgb" "$in_pdf" "$in_html" "$match")"
  done

  # Determinism: re-render and compare bytes.
  PDF2="${ART}/probe-${theme}.2.pdf"
  SOURCE_DATE_EPOCH=1700000000 "$BIN" "$PROBE" --font "$theme" --to pdf --out "$PDF2" >/dev/null 2>&1
  if cmp -s "$PDF" "$PDF2"; then
    log "determinism: PDF byte-identical across runs (sha256 $(sha256sum "$PDF" | cut -c1-16))"
  else
    log "determinism: FAILED — PDF differs across runs"
    fail=1
  fi
done

log ""
if [ "$fail" -eq 0 ]; then
  log "theme-proof: ok — every theme token is consumed by BOTH HTML and PDF, deterministically."
else
  log "theme-proof: FAILED — a token diverged across surfaces (see MISMATCH rows above)."
fi
exit "$fail"
