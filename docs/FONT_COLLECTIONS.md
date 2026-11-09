# Font collections

`fmd render`, `watch`, `batch`, and `book` accept TrueType/OpenType collections
through the existing `--pdf-font SLOT=PATH` mappings. Select the face explicitly
with `--pdf-font-face SLOT=INDEX`. Indices start at zero. A bare index selects
`body-regular`, matching the shorthand used by `--pdf-font-weight`.

```sh
fmd render report.md --to both --out report.html \
  --pdf-font body-regular=./fonts/Family.ttc --pdf-font-face 0 \
  --pdf-font body-bold=./fonts/Family.ttc --pdf-font-face body-bold=1

fmd book ./manual --to epub --out manual.epub \
  --pdf-font body-regular=./fonts/Family.otc --pdf-font-face 2

fmd batch ./chapters --to both --out-dir ./publication \
  --pdf-font body-regular=./fonts/Family.ttc --pdf-font-face 0

fmd watch report.md --out report.html \
  --pdf-font body-regular=./fonts/Family.ttc --pdf-font-face 0
```

The selected face applies to HTML, PDF, SVG, and EPUB wherever the command
supports that output. Each slot can select its own face from the same
collection. Slots are `body-regular`, `body-bold`, `body-italic`,
`body-bold-italic`, and `mono-regular`.

## Selection and publication

Collection versions 1.0 and 2.0 are identified by their bytes, independently
of the filename extension. Every mapped collection needs a selector. An
ordinary standalone `.ttf` or `.otf` uses `--pdf-font` without
`--pdf-font-face`.

The loader extracts the selected face to a standalone font before ordinary
font validation, variable-font instancing, subsetting, or full-font embedding.
TrueType outlines and supported CFF1 outlines, including supported CID-keyed
faces, use the existing shared renderer. A variable TrueType face can combine
face selection with `--pdf-font-weight`:

```sh
fmd report.md --to pdf --out report.pdf \
  --pdf-font body-regular=./fonts/VariableFamily.ttc \
  --pdf-font-face 1 --pdf-font-weight 650
```

Font paths resolve from the command's working directory. Each original file
and each extracted face must fit within 32 MiB. Collections admit at most
1,024 faces and 4,095 tables per selected face. Extraction preserves shared
table contents, rebuilds standalone offsets and checksums, and removes
signatures invalidated by extraction. Existing renderer support and
publication font budgets still apply.

The original collection remains a protected input. Export destinations cannot
overwrite it. Watch mode tracks that file and reloads the configured index when
the collection changes. Later `--pdf-font` mappings retain their existing
replacement behavior; all read files stay protected. The selector identifies
the effective final font in the slot. Earlier standalone assignments are still
validated normally, and earlier collections use the slot's selected index.

## Errors

Invalid indices, duplicate selectors for one slot, selectors without a mapped
font, selectors on standalone fonts, and collections without a selector are
usage errors (exit 64). A selected index outside the collection is also a
usage error. Malformed collections, unsupported selected-face formats,
unreadable files, and font-size or validation failures are input errors
(exit 66). These failures occur before publication output is replaced.
Watch validates selector syntax before polling. Font-file failures retain its
usual recovery behavior: it reports the failed render, preserves the previous
output, and retries when a watched input changes.

## Library callers

The native loader keeps `FontAssets` unchanged: its slots contain standalone
font bytes. Library callers can use the same bounded extractor directly:

```rust
use franken_markdown::{FontAssetSlot, FontAssets, text};

# fn example(collection: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
let count = text::collection_face_count(collection)?;
assert!(count > 0);
let face = text::extract_collection_face(collection, 0)?;
let assets = FontAssets::default().with_slot(FontAssetSlot::BodyRegular, face)?;
# let _ = assets;
# Ok(())
# }
```

Both extractor functions return typed `CollectionError` values. Ordinary
`Font::parse` and renderer font slots continue to accept standalone fonts;
collection extraction is an explicit host preparation step.

## Browser and WASM callers

The browser package exposes the same bounded extractor:

```js
import {
  fontCollectionFaceCount,
  extractFontCollectionFace,
  renderPdf,
} from '@franken-suite/franken-markdown';

const collection = new Uint8Array(await file.arrayBuffer());
const count = await fontCollectionFaceCount(collection);
const selected = await extractFontCollectionFace(collection, 1);
const pdf = await renderPdf('# Report', {
  fontAssets: [{slot: 'body-regular', bytes: selected}],
});
```

`extractFontCollectionFace` defaults to face zero when its index is omitted.
Explicit indices must be integers from 0 through 1,023 and identify an existing
face. The functions accept an `ArrayBuffer` or an exact typed-array/DataView
range, copy that range before asynchronous initialization, and return owned
data. Shared and detached buffers, empty inputs and inputs over 32 MiB are
rejected. Neither function reads files, fetches resources, or discovers system
fonts. A caller may reuse or change its input after calling the function.

The selected bytes are an ordinary standalone font. Pass them through the
existing font slots for HTML, PDF, SVG, EPUB, books and worker exports. Font
validation, embedding permissions, variable-weight instancing, output-format
support and publication resource budgets still apply. Inspecting a collection
header alone does not validate every face's glyph programs.

Failures are `FontCollectionError` values with stable `code` fields. Native
extractor errors keep their `FONT_COLLECTION_` category and selected-face,
table and offset context. Older generated packages without these exports
produce `UNSUPPORTED_WASM_PACKAGE`; existing standalone-font rendering remains
available. These helpers are also available on the object returned by
`createRenderer`.
