#!/usr/bin/env bash
# DSR quality gate: fresh, retained WASM package; no cleanup or Actions.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export RCH_CARGO_WRAPPER_BYPASS=1
export CARGO_HTTP_USER_AGENT='OpenAI File Downloader, XaiImageApiFetch/1.0'
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
mkdir -p "$ROOT/tests/artifacts/wasm"
ART="$(mktemp -d "$ROOT/tests/artifacts/wasm/dsr.XXXXXXXX")"
PACKAGE="$ART/package"
mkdir -p "$PACKAGE/pkg" "$PACKAGE/demo" "$ART/parity"
cargo build --no-default-features --lib
cargo build --no-default-features --target wasm32-unknown-unknown --lib
cargo build --release --no-default-features --features wasm-bindgen --target wasm32-unknown-unknown --lib
wasm-bindgen "$TARGET_DIR/wasm32-unknown-unknown/release/franken_markdown.wasm" --target web --out-dir "$PACKAGE/pkg"
for file in franken_markdown.js franken_markdown.d.ts fmd-view.js fmd-view.d.ts \
  book.js book.d.ts book_session.mjs book-worker.js book-worker.d.ts book_worker.mjs book_worker_entry.js BOOK.md LIBRARY.md flow.js flow.d.ts flow_session.mjs flow_outlines.mjs flow-canvas.js flow-canvas.d.ts CANVAS.md FLOW.md \
  flow_export.mjs EXPORT.md SOURCE.md \
  flow-assets.js flow-assets.d.ts flow_raster.mjs ASSETS.md \
  flow-reader.js flow-reader.d.ts flow_reading.mjs READER.md \
  flow-worker.js flow-worker.d.ts flow_worker.js flow_worker_session.mjs flow_worker_protocol.mjs worker_transport.mjs WORKER.md \
  package.json README.md; do
  cp "wasm/$file" "$PACKAGE/$file"
done
cp wasm/demo/index.html wasm/demo/demo.js wasm/demo/web-component.html wasm/demo/sample.md "$PACKAGE/demo/"
cp wasm/demo/flow-canvas.html wasm/demo/flow-canvas.js wasm/demo/flow_preview_controller.mjs wasm/demo/local_image_sources.mjs wasm/demo/flow_reading_controls.mjs wasm/demo/flow_preview_export.mjs wasm/demo/flow_export_controls.mjs "$PACKAGE/demo/"
for file in book.html book.js book_collection.mjs book_controls.mjs book_library_store.mjs book_library_session.mjs book_library_controls.mjs flow-source.js flow_document.mjs flow_draft_store.mjs flow_draft_session.mjs flow_draft_controls.mjs; do
  cp "wasm/demo/$file" "$PACKAGE/demo/$file"
done
cp examples/showcase.md "$ART/parity/showcase.md"
node wasm/smoke.mjs "$PACKAGE" "$PACKAGE/pkg/franken_markdown_bg.wasm" "$ART/parity" 1700000000 "$ART/parity/showcase.md"
node wasm/flow_smoke.mjs "$PACKAGE" "$PACKAGE/pkg/franken_markdown_bg.wasm"
node wasm/flow_worker_smoke.mjs "$PACKAGE" "$PACKAGE/pkg/franken_markdown_bg.wasm"
node wasm/flow_outlines_smoke.mjs "$PACKAGE" "$PACKAGE/pkg/franken_markdown_bg.wasm"
node wasm/flow_export_smoke.mjs "$PACKAGE" "$PACKAGE/pkg/franken_markdown_bg.wasm"
cargo build --bin fmd
for ext in html pdf; do
  SOURCE_DATE_EPOCH=1700000000 "$TARGET_DIR/debug/fmd" "$ART/parity/showcase.md" --no-config --to "$ext" --out "$ART/parity/native.$ext"
  cmp "$ART/parity/native.$ext" "$ART/parity/showcase.wasm.$ext"
done
python3 - "$PACKAGE" <<'PY'
import gzip,json,pathlib,sys
p=pathlib.Path(sys.argv[1]);d=(p/'pkg/franken_markdown_bg.wasm').read_bytes()
assert len(d)<=4_750_000, len(d)
assert len(gzip.compress(d,mtime=0))<=2_100_000
for name in json.loads((p/'package.json').read_text())['files']:
    assert (p/name).is_file(), name
print('WASM bytes:',len(d),'gzip:',len(gzip.compress(d,mtime=0)))
PY
printf 'Verified package: %s\n' "$PACKAGE"
