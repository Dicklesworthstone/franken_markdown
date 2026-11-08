# Native font collections

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
`Font::parse` and existing browser font transports continue to accept
standalone fonts; a host can extract its selected face before passing those
bytes to an existing API.
