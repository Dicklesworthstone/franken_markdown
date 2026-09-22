# Embedded fonts in EPUB documents and books

EPUB exports now honor explicitly supplied font assets. The same font slots
used for PDF and HTML can produce font resources in both `render_epub` and
`render_book_epub`, including the retained `BookRenderer` and browser `FmdBook`
paths. There is no new dependency, filesystem lookup, or network font fetch.

## Opt in with the existing font API

Native single-document exports accept the same font slots, custom stylesheet,
and local image assets as the library:

```bash
fmd manual.md --to epub --out manual.epub --css publication.css \
  --pdf-font body-regular=Book-Regular.ttf \
  --pdf-image diagrams/overview.svg=assets/overview.svg
```

Relative local images are loaded automatically from the Markdown directory;
explicit `--pdf-image` mappings override them. EPUB does not fetch remote images.
The existing `--max-pdf-image-bytes` limit applies before an export is written.
With no supplied font face, the default EPUB remains free of embedded fonts.

For Rust callers, populate `HtmlOptions.font_assets` before calling an EPUB
renderer. The bytes must be supported TrueType/sfnt with `glyf` outlines:

```rust
use franken_markdown::{FontAssetSlot, FontAssets, HtmlOptions, parse_markdown};

// font_bytes is an explicitly supplied Vec<u8> owned by the host.
let fonts = FontAssets::default()
    .with_slot(FontAssetSlot::BodyRegular, font_bytes)?;
let options = HtmlOptions { font_assets: fonts, ..HtmlOptions::default() };
let epub = franken_markdown::epub::render_epub(
    &parse_markdown("# Manual\n\nRegular and **bold** text."), &options,
)?;
```

For a retained Rust book, set `renderer.options_mut().font_assets` before
`renderer.render_epub()`. Browser callers use the existing `fontAssets` option
or `session.setFont()`:

```js
import { createBook } from "@franken-suite/franken-markdown/book";

// fontBytes is a Uint8Array explicitly selected or supplied by the host.
const session = await createBook([
  { path: "start.md", source: "# Manual\n\nText with **emphasis**.\n" },
  { path: "end.md", source: "# Résumé\n\nLater-chapter glyphs.\n" }
], {
  title: "Manual",
  fontAssets: [{ slot: "body-regular", bytes: fontBytes }]
});
try {
  const output = session.renderEpub();
  // Offer output.blob() for explicit download.
} finally {
  session.dispose();
}
```

The EPUB worker export receives the same `BookOptions`. Rebuild the matching
Rust/WASM binary: this is rendering logic in Rust, not a JavaScript substitute
or a newly published package. No additional font-upload control is added to the
workbench by this change. The native book CLI has no new font-file flag; its
font-free defaults remain unchanged.

## What is packaged

Supplying at least one face opts into publication-wide font embedding. Missing
slots use the shared bundled registry: body regular, bold, italic, bold-italic,
and monospace, plus the existing symbol fallback. With no supplied face bytes,
the historical font-free EPUB output is retained. Changing a theme family or
setting a weight pin alone does not opt into embedding.

The repertoire is gathered from every rendered chapter after embedded images
have become archive references, plus book/chapter titles and stylesheet text.
This includes later-chapter Unicode, emitted MathML text, and referenced-note
labels, without copying base64 image data into the repertoire. ASCII and common
generated list/backlink markers are seeded. Numeric XML character references
are decoded. The collector may include harmless extra markup characters; it is
not a second HTML or Markdown parser. CSS escape syntax, arbitrary script-created
content, external fonts, and arbitrary raw-HTML extensions are not interpreted.

Each face is subset once for that union of characters. Byte-identical subsets
share one archive resource even when multiple font roles use them. The number
of font files does not multiply with the number of chapters.

The archive contains:

- `OEBPS/fonts/font-N.ttf`: deterministic static TrueType subsets.
- `OEBPS/embedded-fonts.css`: shared font-face and font-family rules.
- Manifest entries for each font (`font/ttf`) and the added stylesheet.

Every chapter and the navigation document link to the shared stylesheets.
Fonts are resources, not extra chapters or spine items. Subset bytes and the
CSS mapping contribute to the existing content-derived publication identifier;
this fingerprint is not an authenticity signature. The OCF mimetype entry
remains first, stored, and byte-exact. Fonts and their CSS use the existing ZIP
DEFLATE writer. `HtmlOptions.html_font_format` still governs HTML embedding;
EPUB uses the fixed `.ttf` resource format for this implementation.

## Weight, style and stylesheet behavior

A supplied `wght` variable font is instanced before subsetting, using the slot's
existing weight pin. When bold has no own bytes, a variable regular face also
supplies bold at the bold slot's weight. Instance or subset failure is an error,
not permission to silently ship the uninstanced font. Static faces ignore weight
pins. CSS descriptors identify semantic normal/bold and upright/italic roles;
the selected weight pin changes that role's actual outlines.

Generated family names are `FmdEpubBody`, `FmdEpubMono`, and `FmdEpubSymbols`.
The normal fallback family reflects the selected sans/serif theme. No font
embeds characters that are absent from the source face; existing symbol fallback
and the reading system's fallback behavior still matter for missing glyphs.

`style.css` remains byte-for-byte the caller's custom CSS, including an explicitly
empty stylesheet. With custom CSS, the generated font stylesheet is linked
first so author rules can override the generated family choices. With default
CSS, the font stylesheet follows the historical serif/monospace rules so those
rules do not undo the supplied fonts. Custom CSS can refer to the generated
family names directly. It is not rewritten, concatenated into generated CSS,
or used to authorize additional file/network reads.

Reading systems can override publisher fonts and have differing font/CSS
support. This change does not establish identical visual layout across readers,
EPUBCheck conformance, or accessibility conformance. No font obfuscation is added;
hosts must supply font bytes they are authorized to embed and distribute.

## Limits and failure handling

Font admission runs before copying renderer options. Both setter-provided and
directly assigned `FontAssets` fields are validated. Limits are 32 MiB per
supplied face, 128 MiB combined supplied slots, 32 MiB per subset, and 128 MiB
unique subset bytes. The shared repertoire admits at most 64 MiB of text and
65,536 distinct Unicode characters. A font-enabled publication also enforces a
256 MiB uncompressed content budget, including fonts, markup, CSS, and packaged
images. Existing image and book limits still apply.

Exceeding a limit returns an error without a partial EPUB. These are logical
input/output/work limits, not a bound on all transient allocations. A retained
session's source and font options are not mutated by rendering. An invalid font
in a directly modified retained book is rejected at render time.

## Regression coverage and verification boundary

`src/epub/font_tests.rs` adds 13 cases for real subset cmaps, static/variable
weights, deduplication, Unicode entities, limits, stylesheet ordering, manifest
membership, actual compressed archive payloads, repeatability, immutable input,
and legacy single-document byte equality.

`src/epub/book_font_tests.rs` adds eight cases for shared book resources,
later-chapter/title-only Unicode, repeated-chapter resource stability,
custom-CSS precedence, font-sensitive identity, retained source bundles, image
and chapter-link preservation, and the font-free book path. ZIP tests inspect
actual payloads, CRCs, and sizes and decode raw DEFLATE using independently
expected bytes. They are not source-string tests or renderer doubles.

Run in the configured Rust/DSR environment:

```sh
cargo test --lib epub::embedded_fonts
cargo test --lib epub::book
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

These new Rust tests were not executed in the authoring environment: it has no
`cargo`, `rustc`, or DSR executable. Source/blob identity checks and lexical
checks do not establish compilation, generated-WASM parity, rendering quality,
or reading-system acceptance. No fonts or generated publications were exported
from that environment.
