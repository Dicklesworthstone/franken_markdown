#!/usr/bin/env bash
# check-wasm-package.sh — the real proof gate for "first-class WASM" (bead 3i5.6).
#
# Builds the release wasm-bindgen artifact, assembles the browser package, loads
# the GENERATED module in headless node, renders HTML+PDF, asserts byte-identical
# native<->WASM parity over a corpus, and enforces a committed .wasm size budget.
# String-matching source tests do NOT satisfy this gate; only a built, loadable,
# rendering module does.
#
# Requires: rustup wasm32 target, wasm-bindgen CLI (== Cargo.toml version), node.
#
# Usage: scripts/check-wasm-package.sh [run-id]
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
export RCH_SHIM_LOCAL_IDE=1
export CARGO_TARGET_DIR="${FMD_TARGET_DIR:-$repo_root/target/fmd-checks}"
mkdir -p "$CARGO_TARGET_DIR"
# shellcheck source=scripts/validate-run-id.sh

source scripts/validate-run-id.sh

RUN_ID="${1:-local}"
fmd_validate_run_id "fmd wasm-package" "$RUN_ID"
ART_BASE="$repo_root/tests/artifacts/wasm"
ART="${ART_BASE}/${RUN_ID}"
WORK="$ART/work"
rm -rf -- "$WORK"; mkdir -p -- "$WORK"
LEDGER="$ART/ledger.txt"
: >"$LEDGER"
log() { printf '%s\n' "$*" | tee -a "$LEDGER"; }
log_check() {
  printf 'wasm-package: check id=%s subject=%s outcome=%s\n' "$1" "$2" "$3" | tee -a "$LEDGER"
}

# Committed size budget for the wasm-bindgen .wasm (raw + gzip). The bundled
# fonts and vector-SVG/PDF drawing code dominate; bump consciously (and note why)
# if a real win/cost lands.
#
# History (measured `franken_markdown_bg.wasm` after wasm-bindgen --target web):
#   0.3.0  budget raw 3,200,000→3,300,000  gzip 1,600,000   vector-SVG/PDF
#   0.3.2  measured raw=3,351,808 gzip=1,510,214  budget 3,400,000 / 1,600,000
#   0.3.4  measured raw=3,447,897 gzip=1,557,945  budget 3,500,000 / 1,600,000
#          (~56 KiB Noto Sans Math fallback + JPEG /DCTDecode)
#   0.3.5  measured raw=4,019,715 gzip=1,798,217 brotli=1,344,427
#          budget 4,200,000 / 1,850,000  (smif.2)  +571,818 raw / +240,272 gzip
#          vs 0.3.4. Shipped contributors (not test-only):
#            fmd-math TeX engine + MathML serializer (dominant compiled code);
#            de/fr/nl/es hyphenation tries; CJK UAX #14; variable-font gvar;
#            typography knobs + page numbers; NotoSansMathSymbols regen
#            (bundled TTF 72,908 B, ~+16 KiB vs 0.3.4's +15.4 KiB face).
#          Not trimmed: build-time face subsetting would edit the reserved
#          fmd-font reader. Cost is documented; the gate re-ratchets.
#   0.3.5 (post-math & definition lists)
#          measured raw=4,150,881 gzip=1,848,152 brotli=1,382,581
#          budget 4,300,000 / 1,900,000 (+131,166 raw / +49,935 gzip)
#          Shipped contributors:
#            GFM-plus definition lists AST/parser/HTML/PDF engine,
#            expanded superscript/phonetic glyph fallbacks,
#            MathML block/inline wiring & caret diagnostics.
#
# PREVIOUS_* is the last ratchet's *measured* size (signed delta in the log).
# Update PREVIOUS_* and BUDGET_* together.
PREVIOUS_RAW=4150881
PREVIOUS_GZIP=1848152
BUDGET_RAW=4300000
BUDGET_GZIP=1900000

target="wasm32-unknown-unknown"
package_dir="$CARGO_TARGET_DIR/wasm-package"
pkg_dir="$package_dir/pkg"

require() {
  command -v "$1" >/dev/null 2>&1 || { printf 'fmd wasm-package: missing required tool: %s\n%s\n' "$1" "$2" >&2; exit 3; }
}

if ! rustup target list --installed | grep -qx "$target"; then
  printf "fmd wasm-package: missing Rust target '%s' (rustup target add %s)\n" "$target" "$target" >&2
  exit 3
fi
require wasm-bindgen "Install: cargo install wasm-bindgen-cli --version 0.2.126 --locked"
require node "Install Node.js (>=18)."

log "=== wasm-package gate run=${RUN_ID} ==="
log "core no-default check"
cargo check --no-default-features --lib

log "build release wasm-bindgen adapter (real shippable artifact)"
cargo build --release --target "$target" --no-default-features --features wasm-bindgen --lib

wasm_in="$CARGO_TARGET_DIR/$target/release/franken_markdown.wasm"
[ -s "$wasm_in" ] || { log "missing wasm artifact: $wasm_in"; exit 1; }

log "wasm-bindgen --target web"
rm -rf "$pkg_dir"; mkdir -p "$pkg_dir"
wasm-bindgen "$wasm_in" --target web --out-dir "$pkg_dir"

# Assemble the package: hand-written wrapper + generated pkg/ + demo.
cp wasm/franken_markdown.js "$package_dir/franken_markdown.js"
cp wasm/franken_markdown.d.ts "$package_dir/franken_markdown.d.ts"
cp wasm/package.json "$package_dir/package.json"
cp wasm/README.md "$package_dir/README.md"
mkdir -p "$package_dir/demo"
cp wasm/demo/index.html "$package_dir/demo/index.html"
cp wasm/demo/demo.js "$package_dir/demo/demo.js"

for artifact in \
  "$package_dir/franken_markdown.js" "$package_dir/franken_markdown.d.ts" \
  "$package_dir/package.json" "$package_dir/demo/index.html" "$package_dir/demo/demo.js" \
  "$pkg_dir/franken_markdown.js" "$pkg_dir/franken_markdown_bg.wasm"; do
  [ -s "$artifact" ] || { log "expected package artifact missing: $artifact"; exit 1; }
done
log "package assembled at $package_dir"

# Manifest completeness (publishability proof, npm-free): every path declared in
# package.json files[] must exist in the assembled package — exactly what
# `npm pack` would include. Plus the README npm auto-ships. The only remaining
# step to publish is a maintainer pushing a tag (see .github/workflows/release-wasm.yml).
log "manifest completeness (publishability):"
manifest_fail=0
# Capture the declared file list first (a command substitution, so `set -e`
# catches a python failure — a process substitution would not, and an empty list
# would vacuously "pass").
declared_files="$(python3 -c "import json; [print(f) for f in json.load(open('wasm/package.json'))['files']]")"
[ -n "$declared_files" ] || { log "manifest FAIL: could not read package.json files[]"; exit 1; }
while IFS= read -r rel; do
  [ -n "$rel" ] || continue
  if [ -s "$package_dir/$rel" ]; then
    log "  files[] ${rel}: present"
  else
    log "  files[] ${rel}: MISSING"; manifest_fail=1
  fi
done <<<"$declared_files"
if [ -s "$package_dir/README.md" ]; then log "  README.md: present"; else log "  README.md: MISSING"; manifest_fail=1; fi
[ "$manifest_fail" -eq 0 ] || { log "manifest FAIL: declared package files missing from assembly"; exit 1; }

# Size budget (raw + gzip; brotli where available), with a ratchet, plus a
# checksum so the artifact identity is recorded in the ledger.
bg="$pkg_dir/franken_markdown_bg.wasm"
raw=$(wc -c <"$bg"); gz=$(gzip -c "$bg" | wc -c)
raw=${raw// /}
gz=${gz// /}
if command -v brotli >/dev/null 2>&1; then br=$(brotli -c "$bg" | wc -c); br=${br// /}; else br="n/a"; fi
fmt_delta() {
  local d="$1"
  if [ "$d" -gt 0 ]; then printf '+%s' "$d"
  elif [ "$d" -eq 0 ]; then printf '0'
  else printf '%s' "$d"
  fi
}
delta_raw=$(fmt_delta $((raw - PREVIOUS_RAW)))
delta_gz=$(fmt_delta $((gz - PREVIOUS_GZIP)))
log "wasm size: raw=${raw} (budget ${BUDGET_RAW}, delta vs last ratchet ${delta_raw}); gzip=${gz} (budget ${BUDGET_GZIP}, delta vs last ratchet ${delta_gz}); brotli=${br}"
log "wasm sha256: $(sha256sum "$bg" | cut -d' ' -f1)"
size_fail=0
if [ "$raw" -le "$BUDGET_RAW" ]; then
  log_check "smif.2.size.raw" "raw=${raw} budget=${BUDGET_RAW} delta=${delta_raw}" PASS
else
  log_check "smif.2.size.raw" "raw=${raw} budget=${BUDGET_RAW} delta=${delta_raw}" FAIL
  log "SIZE FAIL: raw ${raw} > ${BUDGET_RAW}"
  size_fail=1
fi
if [ "$gz" -le "$BUDGET_GZIP" ]; then
  log_check "smif.2.size.gzip" "gzip=${gz} budget=${BUDGET_GZIP} delta=${delta_gz}" PASS
else
  log_check "smif.2.size.gzip" "gzip=${gz} budget=${BUDGET_GZIP} delta=${delta_gz}" FAIL
  log "SIZE FAIL: gzip ${gz} > ${BUDGET_GZIP}"
  size_fail=1
fi

# Native binary for the parity oracle (debug is fine: output is deterministic).
log "build native fmd (parity oracle)"
host_target="$(rustc -vV | sed -n 's|host: ||p')"
# The build must not abort under `set -e` before the host-executable check can
# engage: an rch offload can fail outright (no same-OS worker for a darwin
# target) or copy back a wrong-OS binary. Both paths fall through to the
# local-bypass rebuild below.
if ! cargo build --quiet --bin fmd --target "$host_target"; then
  log "remote-routed fmd build failed; building the parity oracle locally"
  RCH_CARGO_WRAPPER_BYPASS=1 cargo build --quiet --bin fmd --target "$host_target"
fi
fmd="$CARGO_TARGET_DIR/$host_target/debug/fmd"
if [ ! -x "$fmd" ] && [ -x "$CARGO_TARGET_DIR/debug/fmd" ]; then
  fmd="$CARGO_TARGET_DIR/debug/fmd"
fi
if [ ! -x "$fmd" ] || ! "$fmd" --version >/dev/null 2>&1; then
  log "fmd binary not executable on host; rebuilding locally"
  RCH_CARGO_WRAPPER_BYPASS=1 cargo build --quiet --bin fmd --target "$host_target"
  fmd="$CARGO_TARGET_DIR/$host_target/debug/fmd"
  if [ ! -x "$fmd" ] && [ -x "$CARGO_TARGET_DIR/debug/fmd" ]; then
    fmd="$CARGO_TARGET_DIR/debug/fmd"
  fi
fi


# Corpus: the showcase plus a focused probe.
EPOCH=1700000000
corpus=()
cp examples/showcase.md "$WORK/showcase.md"; corpus+=("$WORK/showcase.md")
# shellcheck disable=SC2016 # The Markdown code fence is intentional literal fixture text.
printf '# Probe\n\n> quote\n>\n> more\n\nBody with a [link](https://example.com) and `code`.\n\n| A | B |\n|---|--:|\n| 1 | 2 |\n| 3 | 4 |\n\n```rust\nfn x() {}\n```\n\n---\n' >"$WORK/probe.md"
corpus+=("$WORK/probe.md")

# WASM side: load the generated module and render the corpus.
log "headless node: load generated module + render corpus"
node wasm/smoke.mjs "$package_dir" "$bg" "$WORK" "$EPOCH" "${corpus[@]}" 2>&1 | tee -a "$LEDGER"

# Native side + byte parity.
log "native<->WASM byte parity:"
parity_fail=0
for md in "${corpus[@]}"; do
  stem="$(basename "$md" .md)"
  "$fmd" "$md" --out "$WORK/${stem}.native.html" >/dev/null 2>&1
  SOURCE_DATE_EPOCH="$EPOCH" "$fmd" "$md" --to pdf --out "$WORK/${stem}.native.pdf" >/dev/null 2>&1
  for ext in html pdf; do
    if cmp -s "$WORK/${stem}.wasm.${ext}" "$WORK/${stem}.native.${ext}"; then
      log "  ${stem}.${ext}: IDENTICAL ($(wc -c <"$WORK/${stem}.native.${ext}") bytes, sha256 $(sha256sum "$WORK/${stem}.wasm.${ext}" | cut -c1-16))"
    else
      log "  ${stem}.${ext}: DIFFER — wasm and native render diverged"; parity_fail=1
    fi
  done
done

log ""
if [ "$size_fail" -eq 0 ] && [ "$parity_fail" -eq 0 ]; then
  log "wasm-package gate: ok — generated module loads, renders, matches native byte-for-byte, within size budget."
  exit 0
fi
log "wasm-package gate: FAILED (size_fail=${size_fail} parity_fail=${parity_fail})."
exit 1
