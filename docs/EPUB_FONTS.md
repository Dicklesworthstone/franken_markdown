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
renderer. Supported inputs are TrueType/sfnt with `glyf` outlines and name-keyed
CFF1 OpenType fonts. Compact subsetting uses the bounded Type 2 outline support
described below; full embedding preserves the admitted font program.
The same slots can mix TrueType and CFF faces:

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
workbench by this change. Native book exports accept the existing host-font
flags. Default EPUB exports remain font-free without supplied faces.

## Choose compact subsets or complete font programs

`HtmlOptions.font_embedding` selects the same policy for HTML and EPUB:

| Policy | Font payload | Intended use |
| --- | --- | --- |
| `FontEmbedding::Subset` (default) | The existing compact, repertoire-based subset | Small self-contained exports with the established output bytes |
| `FontEmbedding::Full` | The complete admitted static font, or the existing pinned variable-font instance | Preserve the font's complete cmap and OpenType layout program for the browser or reading system |

```rust
use franken_markdown::{FontEmbedding, HtmlOptions};

// Optional host font_assets replace the bundled faces used by Full mode.
let options = HtmlOptions {
    font_assets: fonts,
    font_embedding: FontEmbedding::Full,
    ..HtmlOptions::default()
};
```

Native exports select the same policy with `--font-embedding subset|full`:

```bash
fmd manual.md --to epub --out manual.epub \
  --pdf-font body-regular=SourceSans3-Regular.otf --font-embedding full
```

The compact subsetters do not retain the source's `GSUB`, `GPOS`, `GDEF`, or
legacy `kern` tables. Full embedding preserves these tables, original glyph
identities, and the complete cmap, including characters a browser may select
during canonical normalization. This lets the reading system apply the source
font's ligatures, kerning, mark positioning, and other supported layout rules.
It does not add a shaping engine to the EPUB renderer or guarantee support in
every reading system. PDF and SVG shaping behavior is unaffected by this option.

Full programs can be much larger than a document's subset and include unused
glyphs. The choice is explicit and the default remains compact. EPUB font errors
return an error without switching policies or writing a partial archive. For
static fonts, the raw font resource is byte-for-byte the supplied program.
Variable fonts use the existing static instancing path before embedding; Full
does not expand that path's supported axes or variation capabilities.

Setting Full also opts EPUB into embedding without host font assets. Unsupplied
slots use the selected theme's bundled body faces, bundled monospace face, and
symbol fallback. In HTML, `html_font_format` still chooses raw sfnt, WOFF1, or
WOFF2; containers preserve the selected complete program's tables. The existing
editable HTML font path already embeds complete faces.

## What is packaged

Supplying at least one face or selecting Full opts into publication-wide font
embedding. Missing slots use the shared bundled registry: body regular, bold,
italic, bold-italic, and monospace, plus the existing symbol fallback. In the
default Subset mode, exports without supplied faces retain the historical
font-free EPUB output. Changing a theme family or setting a weight pin alone
does not opt into embedding.

The repertoire is gathered from every rendered chapter after embedded images
have become archive references, plus book/chapter titles and stylesheet text.
This includes later-chapter Unicode, emitted MathML text, and referenced-note
labels, without copying base64 image data into the repertoire. ASCII and common
generated list/backlink markers are seeded. Numeric XML character references
are decoded. The collector may include harmless extra markup characters; it is
not a second HTML or Markdown parser. CSS escape syntax, arbitrary script-created
content, external fonts, and arbitrary raw-HTML extensions are not interpreted.

In the default Subset mode, each face is subset once for that union of
characters. Full mode keeps each complete font program independently of the
repertoire; collection and publication limits still apply. In either mode,
byte-identical programs share one archive resource even when multiple font
roles use them. The number of font files does not multiply with the number of
chapters.

The archive contains a resource matching each program's actual outline format:

| Font program | Resource | Manifest media type | CSS format |
| --- | --- | --- | --- |
| TrueType | `OEBPS/fonts/font-N.ttf` | `font/ttf` | `truetype` |
| CFF1 OpenType | `OEBPS/fonts/font-N.otf` | `font/otf` | `opentype` |

`OEBPS/embedded-fonts.css` carries the shared font-face and font-family rules.
The manifest declares each font and the stylesheet. Five semantic slots using
identical CFF bytes share one `.otf` resource; a bundled TrueType symbol fallback
remains a separate `.ttf` resource. Existing TrueType subset bytes, filenames,
CSS descriptors and manifest entries retain their prior format.

Every chapter and the navigation document link to the shared stylesheets.
Fonts are resources, not extra chapters or spine items. Font bytes and the
CSS mapping contribute to the existing content-derived publication identifier;
this fingerprint is not an authenticity signature. The OCF mimetype entry
remains first, stored, and byte-exact. Fonts and their CSS use the existing ZIP
DEFLATE writer. `HtmlOptions.html_font_format` still governs HTML embedding;
EPUB uses raw `.ttf` or `.otf` resources according to the font program.

## Weight, style and stylesheet behavior

A supplied TrueType `wght` variable font is instanced before either subsetting
or full embedding, using the slot's existing weight pin. When bold has no own
bytes, a variable regular face also
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
supplied face, 128 MiB combined supplied slots, 32 MiB per emitted font program,
and 128 MiB of unique font bytes in either embedding mode. The shared repertoire
admits at most 64 MiB of text and
65,536 distinct Unicode characters. A font-enabled publication also enforces a
256 MiB uncompressed content budget, including fonts, markup, CSS, and packaged
images. Existing image and book limits still apply.

Exceeding a limit returns an error without a partial EPUB. These are logical
input/output/work limits, not a bound on all transient allocations. A retained
session's source and font options are not mutated by rendering. An invalid font
in a directly modified retained book is rejected at render time.

CFF1 fonts use strict subsetting in the default Subset mode. Cubic curves, hints, flex operators and
local/global subroutines are supported within the decoder's work limits;
generated subsets contain deterministic unhinted cubic outlines. CID-keyed CFF,
CFF2, `seac`, computational operators, unsupported transforms and ink outside
the supported coordinate range are refused. A structurally admitted face can
still fail when a requested glyph uses an unsupported operation. The EPUB error
retains the typed font cause and glyph context under `epub_fonts:`; the renderer
does not return an archive with that glyph silently removed. Invalid unused
glyph programs do not block a subset that does not request them. Full mode
retains the complete program after the same structural font admission and
delegates glyph execution to the reading system; subset-specific used-glyph
decoder checks do not apply. It still refuses structurally unsupported faces
such as CID-keyed CFF and CFF2.

## Regression coverage and verification boundary

`src/epub/font_tests.rs` covers real subset cmaps, static/variable
weights, deduplication, Unicode entities, limits, stylesheet ordering, manifest
membership, actual compressed archive payloads, repeatability, immutable input,
and legacy single-document byte equality. The CFF cases use the pinned Adobe
Source Sans 3 fixture to verify exact cubic outlines and metrics, shared
all-slot subsets, mixed TrueType/CFF resource metadata, later-chapter glyphs,
and explicit refusal of a used unsupported glyph. A separate compatibility
case compares TrueType payloads directly with the legacy subset API and checks
their unchanged resource metadata. Full-mode cases check exact CFF and TrueType
source bytes in actual compressed archive entries, bundled fallback programs,
static instances before deduplication, shared book resources, font-free defaults,
explicit bundled-font embedding for documents and books, structural admission,
and output limits. `tests/html_full_fonts_test.rs` also
checks every semantic slot and all source tables through raw sfnt, WOFF1, and
WOFF2 embedding.

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
cargo test --test html_full_fonts_test
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
```

These checks do not establish generated-WASM parity, EPUBCheck conformance, or
visual acceptance in EPUB reading systems; those require their own executable
checks.
