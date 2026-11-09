# Changelog

All notable changes to `franken_markdown` and the `fmd` CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- The book workbench reopens externally edited source folders using current
  Markdown files and the exported manifest's chapter order, include-only roles
  and applied settings. A cancellable review validates the entire selection
  before replacing the book; intervening edits invalidate the review. Images
  and fonts require separate authorization. See [Book source folders](wasm/BOOK_DIRECTORY.md).
- SVG exports render linked tables of contents at explicit Markdown markers
  or automatically before the first body heading, with depth filtering and
  exact heading identity. Native commands, MCP, browser workers and Flow share
  the options; additive WASM bindings preserve existing signatures. Bounded
  inventories and cumulative expansion retain source text on refusal. See
  [SVG tables of contents](docs/SVG_TOC.md).
- The language server completes footnote and explicit reference-link labels,
  including image references, unfinished suffixes and mid-label edits. Parser
  ownership and UTF-16 ranges preserve surrounding text; bounded inventories
  retain definition order, Unicode reference identity and existing heading
  fragment completion. See [Label completion](docs/LSP_LABEL_COMPLETION.md).
- The book workbench downloads editable source ZIPs without folder-access or
  rendering-engine support. Archives retain exact source bytes under `sources/`
  plus a reopenable source-only project with order, roles and applied settings.
  Bounded, cancellable preparation rejects portable-path collisions and retires
  stale downloads when the source changes. Existing edition ZIP bytes remain
  unchanged. See [Book source folders](wasm/BOOK_DIRECTORY.md).
- EPUB citations, trailing notes and return links carry source-owned EPUB and
  accessibility semantics. Deterministic note targets avoid collisions with
  body and note headings while preserving authored link destinations, chapter
  isolation and ordinary HTML markup. See [EPUB notes](docs/EPUB_NOTES.md).
- Tagged PDF lists expose each source item's marker as a separate label before
  its body, with explicit bullet/decimal/custom numbering metadata. Labels stay
  attached to nested, empty and page-spanning items, including wrapped ordinals;
  body headings, tables, code and figures retain their own semantic roles.
- Portable native workspaces can select TTC/OTC faces from their Document fonts
  dialog. Collection parsing stays in disposable workers; cancellation, stale
  selections and rejected faces preserve the prior document. Save HTML embeds
  only the selected standalone faces for subsequent HTML/PDF/SVG/EPUB exports.
- The browser book font picker imports explicit TTC/OTC faces in background
  workers, then validates the whole proposed batch before replacing any role.
  Selected standalone faces survive previews, PDF/EPUB/site publication and
  portable backups; source-only projects still grant no font access.
- Browser and WASM callers can inspect TTC/OTC collections and extract an
  explicitly selected standalone font with the same bounded Rust implementation
  as native commands. The helpers own exact input byte ranges before async
  initialization, preserve typed error categories, and feed existing font slots
  across HTML, PDF, SVG, EPUB and books. See [Font collections](docs/FONT_COLLECTIONS.md).
- Book PDFs expose fidelity diagnostics from the layout that actually produced
  their bytes, including missing figures, font/math fallbacks and final
  pagination limitations. Additive native and WASM reported exports reuse the
  measured layout; browser workers, bounded caches, proofs and complete editions
  retain immutable reports. Older packages expose diagnostics as unavailable,
  and truncated browser reports say so explicitly. The native CLI now preserves
  the final pagination findings in its book output receipt.
- SVG exports link footnote citations to the complete note body and note
  labels back to the first painted citation. Ordinary `#fn-ID` and `#fnref-N`
  fragments resolve to actual note destinations. Typed transient provenance
  preserves the shared AST, rich note content and existing endnote transform;
  ambiguous source aliases remain inactive. See [SVG links](docs/SVG_LINKS.md).
- Book publishing controls expose literal PDF line spacing, horizontal and
  vertical table padding, and footnote return links. Editing one option keeps
  unrelated imported typography settings intact, including numeric precedence;
  native defaults now clear all typography overrides. Source and portable
  projects restore these controls without changing resource authorization.
- Browser books retain an explicit `commonmark-gfm` or `gfm-plus` dialect
  across chapter previews, PDF/EPUB/site exports, source updates, workers and
  atomic publications. The workbench exposes a dialect selector and saves
  explicit choices in source-project schema 7. Additive native bindings and
  matched output receipts reject unsupported or stale package combinations;
  omitted profiles keep existing defaults and older project schemas.
- Authors can place `<!-- pagebreak -->` on its own line to request an
  explicit PDF page boundary, including in books, includes and nested
  containers. HTML/EPUB preserve the print boundary, Flow shows its source
  location, and continuous SVG omits the control. The default and GFM+ profiles
  enable it; explicit CommonMark/GFM keeps ordinary comment behavior. Parser
  ASTs and source spans stay unchanged; see [Page breaks](docs/PAGE_BREAKS.md).
- Standalone equations accept explicit `\tag{...}` and `\tag*{...}` labels in
  HTML, EPUB, PDF, SVG and Flow. Labels retain text styling and math islands,
  stay beside the centered equation within its containing column, and fit or
  scroll with the complete formula at narrow widths. Additive `fmd-math`
  display APIs preserve the existing intrinsic math layout contract; see
  [Explicit equation labels](docs/DISPLAY_EQUATION_LABELS.md).
- Browser HTML, PDF/PDF-A, SVG and EPUB exports now accept the same explicit
  `commonmark-gfm` and `gfm-plus` authoring profiles as native rendering.
  Workers, atomic publications and batches retain the selected dialect.
  Additive bindings preserve existing callers, and older generated packages
  reject unsupported profile requests instead of silently ignoring them.

### Fixed

- Connected source-project files preserve their actual schema during save
  comparison, allowing PDF/A, baseline-grid and authoring-dialect settings to
  survive verified disk saves. Equivalent supported versions share source
  identity while external file edits still block overwrites.
- HTML and EPUB mathematics preserve complete conditions inside TeX text
  boxes, including nested subscripts, fractions, text styling, colors and
  explicit spacing. Ordinary text and equation labels share one semantic
  MathML path; script and fraction contexts select the same mathematical
  branches as vector layout. Plain text output keeps its existing bytes; see
  [Mathematics in HTML and EPUB](docs/HTML_MATHEMATICS.md).
- Standalone PDF exports resolve ordinary `#fn-...` and `#fnref-...` links to
  the same notes and first citations as HTML, using their final page positions.
  These links preserve citation ordering and page-note placement. Ambiguous
  IDs stay inert, and source links cannot capture unrelated SVG image links.
- Batch exports discover image ownership across all inputs before workers run,
  including missing paths and originals hidden by explicit overrides. Every
  publication checks source identities, so one job cannot create another job's
  missing image or replace a source through a symlink or file alias.
- Native render and watch exports preserve every admitted local image source,
  including missing files, dangling symlink entries and targets, and originals
  hidden by explicit overrides. All intended outputs, including combined-export
  sidecars, are checked before publication.
- Definition lists and footnotes preserve physical line breaks while parsing
  inline syntax. Authored hard breaks, code-span spaces, HTML attributes and
  TeX comments retain their meaning; malformed multiline destinations no longer
  become active links or images after whitespace folding.
- Parser diagnostics follow the same block and reference boundaries as parsing.
  Valid multiline references, footnotes and literal HTML/math/code no longer
  produce false warnings, and unclosed fences inside lists and quotations report
  their actual container boundary with original source coordinates.
- Literal NUL characters in Markdown become U+FFFD before parsing, including
  reference identities, destinations, literal blocks and frontmatter metadata.
  HTML and EPUB export receive normalized text while block spans, diagnostics,
  destination fragments, reference-rename locations and include ownership retain
  original byte offsets.
- Native book exports preserve referenced image paths even when loading fails,
  including oversized files, missing assets and rejected symlinks. Publication
  also checks directory-entry and Unix hard-link identities, protects missing
  manifest/configuration inputs, and refuses every conflicting output before
  creating directories or replacing any files.
- Formatted link labels and image alternate text retain their authored trailing
  spaces, matching plain labels. Emphasis, code, entities and reference syntax
  no longer erase word separation inside labels; paragraph-end trimming and
  editable destination spans retain their existing behavior.

## [0.7.0] - 2026-11-03

A feature release (331 commits since 0.6.0: 322 feature and fix commits, then
the version commit, seven lint, test and snapshot commits, and this changelog). It
changes the public Rust API of all three crates and some default output, so it
is a minor-version bump. `fmd-font` 0.4.0 and `fmd-math` 0.3.0 ship with it.
GitHub release binaries are built for x86_64 and aarch64 Linux (gnu), x86_64
and aarch64 macOS, and x86_64 Windows (msvc). All three crates now declare
`rust-version = "1.88"`: they use let-chains, which the 1.85 declared by 0.6.0
cannot compile ([#18](https://github.com/Dicklesworthstone/franken_markdown/issues/18)).

No browser (WASM) tarball is published with this release; npm `@franken-suite/franken-markdown` stays at 0.5.0.
Browser package changes (`wasm/`, including `<fmd-view>`) are not part of this release.
The iOS app (`ios/`) is not shipped.

`fmd batch`, `fmd mcp` and `fmd-lsp` exist only in builds with the cargo
features `batch`, `mcp` and `lsp`; the entries for them apply to such builds.

### Breaking changes

- **Rust 1.88:** `franken_markdown`, `fmd-font` and `fmd-math` set
  `rust-version = "1.88"` (it was 1.85, which cannot compile their let-chains),
  so Cargo refuses older toolchains before building ([#18](https://github.com/Dicklesworthstone/franken_markdown/issues/18), [c4df3f7](https://github.com/Dicklesworthstone/franken_markdown/commit/c4df3f78d3354237aeba494ab1abbefb8348ead0)).
- **Non-ASCII heading anchors change:** heading IDs keep Unicode letters,
  lowercased, on every surface: HTML, EPUB, PDF destinations, bookmarks and
  contents, the search index, `fmd stats`, book validation and SVG link
  targets. `# Café` gets the ID `café` and `# 中文 Guide` gets `中文-guide`.
  0.6.0 dropped non-ASCII characters and gave a heading with no ASCII letters
  or digits the ID `section`, so links to the old IDs of such headings no
  longer resolve. Fragment links are percent-decoded once; malformed escapes,
  invalid UTF-8 and control characters are rejected. The new `anchors` module
  exposes `heading_slug`, `text_slug` and `decode_fragment` ([57807a2](https://github.com/Dicklesworthstone/franken_markdown/commit/57807a2d4b7201d51c7186c0f9b5a1baef535341)).
- **`--pdf-a` needs a PDF target:** `fmd render`, `fmd watch` and `fmd batch`
  exit 64 with "`--pdf-a and --pdf-a-strict require --to pdf or --to both`"
  when either flag is given with another target.
  `fmd render --to html --pdf-a 2b` was not rejected before ([3b3b72f](https://github.com/Dicklesworthstone/franken_markdown/commit/3b3b72feb5eaf1234938d13eaf7c6eac9b00dfd3)).
- **`fmd batch` applies each document's frontmatter:** title, author (PDF),
  `lang`, `toc` and `toc_depth` come from each input's frontmatter after
  include expansion, as in `fmd render`. An explicit flag wins, then
  frontmatter, then native config, so `toc: false` in a document overrides a
  configured TOC. 0.6.0 batch did not read frontmatter and applied config
  `lang`, `toc` and `toc_depth` to every file. Library batch callers'
  `HtmlOptions` and `PdfOptions` count as explicit options, and native config
  is not read for them ([09bcc80](https://github.com/Dicklesworthstone/franken_markdown/commit/09bcc805ad528f61079270173a6bdc121bcae4ab)).
- **`fmd batch` fetches remote images by default:** `--to pdf`, `--to both` and
  `--to svg` jobs now download http(s) image destinations, including those in
  included fragments and supported raw HTML, with the system `curl` or `wget`
  as `fmd render` does: once per distinct URL per document, within
  `--max-pdf-image-bytes` and `--remote-image-timeout-secs`, and stopped by the
  batch deadline. A failed fetch keeps the alt text and prints a warning naming
  the input. Pass `--no-remote-images` to keep batch offline. HTML and EPUB
  jobs do not fetch, and the library batch entry points stay offline
  ([8029c89](https://github.com/Dicklesworthstone/franken_markdown/commit/8029c89d4321308c7ba52510051c30f9c9ef7a86)).
- **Downloader config files are ignored:** the image fetcher of `fmd render`
  and `fmd batch` starts curl with `--disable` and wget with `--no-config`, so
  personal curl and wget config files no longer apply; proxy and certificate
  environment variables still do ([8029c89](https://github.com/Dicklesworthstone/franken_markdown/commit/8029c89d4321308c7ba52510051c30f9c9ef7a86)).
- **The `fmd` binary embeds a CJK font:** the default `cli` feature now enables
  the new `cjk-face` feature, so `fmd` carries a 9,867,212-byte Noto Sans SC
  subset. CJK text in PDF and SVG draws glyphs instead of `.notdef` boxes with a
  `missing_glyphs` warning, and HTML and EPUB output for documents with CJK
  text embeds a CJK subset. Builds without `cli` leave it off and can enable
  `cjk-face` ([140f77d](https://github.com/Dicklesworthstone/franken_markdown/commit/140f77d1ab7bed88fcf840e44371b43555e0ad05)).
- **PDF/A-2b rejects CMYK images:** an archival export that uses an Adobe CMYK
  or YCCK JPEG fails with `pdf_a_cmyk_image` (the output profile is sRGB) and
  leaves existing output files untouched; unused image mappings do not
  trigger it ([44c88dc](https://github.com/Dicklesworthstone/franken_markdown/commit/44c88dc5a29dc6edab0ea3e2ba690107bc97b544)).
- **Numbered `align` is refused:** fmd-math used to set `\begin{align}` and drop
  its equation numbers. It is now refused by name, like `equation`, `gather`
  and `multline`; `align*` and `aligned` still work. In `fmd render`, PDF and
  SVG output print a warning naming `env:align` and show the formula's TeX
  source in its place; HTML output carries the TeX source in its `math
  display` block and prints no warning ([662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3)).
- **Config defaults reach `fmd book`:** the persistent config values
  `font_scale`, `lang`, `html_font_format`, `toc`, `toc_depth`, `microtype`,
  `pdf_page_numbers`, `pdf_optimal_pagination` and the three typography
  defaults now apply to books; explicit flags still win. `--check-links` now
  conflicts with the new publishing flags ([47f6bc0](https://github.com/Dicklesworthstone/franken_markdown/commit/47f6bc0f9f52d1416bc7a1ed1de111b2b3f51d87)).
- **`FMD_TAIL_*` switches are gone:** `FMD_TAIL_NOGLYPH`, `FMD_TAIL_NOHEX` and
  `FMD_TAIL_NOPAIR` no longer change PDF output (see Removed) ([c97d10e](https://github.com/Dicklesworthstone/franken_markdown/commit/c97d10e40769f98ee87db6753893f8b10dac7cee)).
- **Default PDF and SVG bytes change:** math uses the Computer Modern faces and
  TeX's font metrics, CJK text draws real glyphs, default pagination keeps a
  heading's wrapped lines and underline together, and the PDF layout fixes
  below move content. Pin 0.6.0 if you depend on byte-identical PDFs
  ([ddebab7](https://github.com/Dicklesworthstone/franken_markdown/commit/ddebab74973bb11e952e64992ddc5c2752583436), [4315c77](https://github.com/Dicklesworthstone/franken_markdown/commit/4315c778e5a19d08cf6ac96012aa060cb7cd96b6), [b972ddb](https://github.com/Dicklesworthstone/franken_markdown/commit/b972ddb89c4fdc9dd54ddcba9de3825ba75f4c5e)).
- **Some edge-case Markdown parses differently:** a lone CR is a line break,
  table body rows need no pipes, a reference definition can run past eight
  lines, reference labels match under Unicode full case folding, and complete
  type-7 HTML tags start HTML blocks. Documents that relied on the old reading
  render differently (see Changed and Fixed) ([f88f0ea](https://github.com/Dicklesworthstone/franken_markdown/commit/f88f0ea138475b6ffa654988c92b3d31555dec27), [f32548e](https://github.com/Dicklesworthstone/franken_markdown/commit/f32548e71141091a546b4e1852014d1f9d44462f), [2b02598](https://github.com/Dicklesworthstone/franken_markdown/commit/2b02598111dd46af7660775bb209a8655321beb6)).
- **fmd-math resource budgets:** `Engine::typeset` refuses a formula source
  over 65,536 bytes, or a layout over 65,536 glyphs or 262,144 path segments,
  with the new `MathError::Budget`, and the free `parse` functions apply the
  same source limit. The nesting bound (64 levels) and the macro budgets report
  `Budget` where they reported `Malformed`. `fmd` uses these defaults
  ([9bbb40a](https://github.com/Dicklesworthstone/franken_markdown/commit/9bbb40ad5472e729ae2fe79052b4b039b9e0f7c1)).
- **New public fields on option structs:** `HtmlOptions::font_embedding`,
  `PdfOptions::strict_pagination`, `PdfEmitOptions::compression`,
  `PageStyle::baseline_grid`,
  `WasmRenderOptions::{strict_pagination, font_embedding}` and
  `config::RenderDefaults::{font_embedding, pdf_strict_pagination, pdf_baseline_grid_pt}`.
  No struct in the three crates
  is `#[non_exhaustive]`, so struct literals must set these fields or end with
  `..Default::default()` ([9fd8f0f](https://github.com/Dicklesworthstone/franken_markdown/commit/9fd8f0fad6b8ad49c7100e743ff584d468b575c8), [bcba460](https://github.com/Dicklesworthstone/franken_markdown/commit/bcba460e9e2e684ea17bf69acc62e2a367517a23), [868359b](https://github.com/Dicklesworthstone/franken_markdown/commit/868359b5678c3a0715f3730bc537dc23622bee57)).
- **New public fields in the display list and `fmd-font`:**
  `display::DisplayTextRun` gains `fill`, `baseline_y` and `math_fallback`,
  `flow_display::FlowInlineRun` gains `math`, and `fmd_font::OwnedTextRun`
  gains `vertical_metrics` ([2fee8ba](https://github.com/Dicklesworthstone/franken_markdown/commit/2fee8badc9ae83ad3e3337ea8876bde6ee64d190), [691b11a](https://github.com/Dicklesworthstone/franken_markdown/commit/691b11a679a33539a821b2e1599ea47d0e868b4d)).
- **New public fields in `fmd-math`:** `PlacedGlyph`, `PlacedRule` and
  `PlacedPath` gain `color: Option<Color>`, and `FracSpec` gains `thickness`
  and `cfrac` (start from `FracSpec::PLAIN` or `FracSpec::BINOM`)
  ([d9c13d7](https://github.com/Dicklesworthstone/franken_markdown/commit/d9c13d7651a22e991fae8203deab3d5d7b5a057d), [19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e)).
- **`RenderWarning` gains five variants:** `TextShapingUnsupported`,
  `TextShapingLimit`, `PaginationFallback`, `ParagraphPaginationLimited` and
  `MathHeightOverflow`. No public enum in the three crates is
  `#[non_exhaustive]`, so exhaustive matches need new arms
  ([ab170ac](https://github.com/Dicklesworthstone/franken_markdown/commit/ab170acf2748ffacefa71851427158bea6e3a542), [a341e9c](https://github.com/Dicklesworthstone/franken_markdown/commit/a341e9c84910cf6e54f45ea0a4706accca37d7fe), [8f4156e](https://github.com/Dicklesworthstone/franken_markdown/commit/8f4156ed90a2516804eea517dd48a24fbc4571cb)).
- **`RenderError::Pagination` and `ast::Align::Decimal`** are new variants
  ([bcba460](https://github.com/Dicklesworthstone/franken_markdown/commit/bcba460e9e2e684ea17bf69acc62e2a367517a23), [607b233](https://github.com/Dicklesworthstone/franken_markdown/commit/607b233364bb24c8215403207356e7e2769fb07b)).
- **Display list variants:** `display::DisplayItem::Path`,
  `AccessibleReadingRole::{Math, Diagram}`,
  `DisplayQueryError::{InvalidPath, PathBudgetExceeded}` and
  `flow_display::DisplayBlock::MathBlock` are new ([edb3921](https://github.com/Dicklesworthstone/franken_markdown/commit/edb39212f45477648937213f48f2595901f2f62d), [691b11a](https://github.com/Dicklesworthstone/franken_markdown/commit/691b11a679a33539a821b2e1599ea47d0e868b4d)).
- **`fmd-math` enum variants:**
  `NodeKind::{AmsSymbol, Boxed, Cancel, Classed, DisplayKern, Logo, MathChoice, Skip}`,
  `TextStyle::{Italic, Roman, Typewriter, SansSerif}`, `MathFont::{Script, Fraktur}` and
  `MathError::Budget` are new ([19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e), [662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3), [9bbb40a](https://github.com/Dicklesworthstone/franken_markdown/commit/9bbb40ad5472e729ae2fe79052b4b039b9e0f7c1)).
- **`fmd-font` `bundled::ALL_FACES` has 29 entries:** its type changes from
  `[(&str, &[u8]); 10]` to `[(&str, &[u8]); 29]`. The first ten entries are
  unchanged; the 19 new ones follow them: `cm-math-italic-10/-7/-5`,
  `cm-math-symbols-10/-7/-5`, `cm-math-extension-10`, `cm-roman-10/-7/-5`,
  `ams-a-10/-7/-5`, `ams-b-10/-7/-5` and `euler-fraktur-10/-7/-5`. The optional
  emoji and CJK faces are not in the array ([ddebab7](https://github.com/Dicklesworthstone/franken_markdown/commit/ddebab74973bb11e952e64992ddc5c2752583436), [a341047](https://github.com/Dicklesworthstone/franken_markdown/commit/a3410479dda20a135b7a35455b0d6707b5ce22c3), [662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3)).
- **`fmd-math` `NodeKind::ColorChange` carries a `ColorSpec`** instead of a
  `String`: the MathML spelling plus the resolved `Color`, or the error that
  layout reports for a colour xcolor does not define ([d9c13d7](https://github.com/Dicklesworthstone/franken_markdown/commit/d9c13d7651a22e991fae8203deab3d5d7b5a057d)).
- **`fmd-math` `MathConstants.sup_drop` and `sub_drop` are removed:** rule 18a's
  drops depend on the script style; call `MathConstants::script_drops(style)`.
  `MathConstants::at(style)` and `metrics::size_fonts(style)` are new, and the
  `metrics::CM` values are now read from the TFM font parameters, so their last
  digits differ ([4db6f43](https://github.com/Dicklesworthstone/franken_markdown/commit/4db6f439f5ed5873fd310122a719c17d942d0b3d), [abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d)).
- **`fmd-math` `MacroSet::canonical_bytes` uses framing `v4`** (was `v1`):
  length-framed names and bodies, a native-binding tag, and the ligature,
  physics and line-width settings. Every macro set, the empty one included,
  serializes differently, so cache keys built from these bytes change. In
  `MacroSet::pack("default")`, `\minus` is a native binding instead of a
  substitution for `-` ([9b457a3](https://github.com/Dicklesworthstone/franken_markdown/commit/9b457a3895d77ebe25d2d45512a838e07556b424), [33e4bb4](https://github.com/Dicklesworthstone/franken_markdown/commit/33e4bb4a4b111aeeba1a00fea14193b099ea9ce7), [61304cc](https://github.com/Dicklesworthstone/franken_markdown/commit/61304ccc480756132af268fc558804ef6e45da34)).
- **Browser ABI signatures (feature `wasm-bindgen`):**
  `wasm_abi::render_pdf_configured_archival` takes a trailing
  `baseline_grid_pt: Option<f64>`, and
  `wasm_abi::render_html_configured_advanced`,
  `wasm_abi::render_epub_configured_advanced` and
  `book::site_publication_browser::render_book_site_publication` take a
  trailing `font_embedding: Option<String>` ([dfe21da](https://github.com/Dicklesworthstone/franken_markdown/commit/dfe21da52910145900db698c28b2d033a72ab6f5), [8525747](https://github.com/Dicklesworthstone/franken_markdown/commit/8525747be9a414b1cf0c1ff2a39ca35e54d400e4)).

### Added

- **PDF text shaping:** PDF text positions supported Latin combining marks and
  stacked accents with the embedded font's OpenType data. Measurement, drawing,
  font subsets and exact text extraction share the retained glyph run,
  including transformed SVG text and running headers and footers. Actual ink
  grows line and margin clearances; paragraph and table wrapping preserve
  shaping context. Unsupported sequences and bounded shaping limits produce
  explicit verification findings ([ab170ac](https://github.com/Dicklesworthstone/franken_markdown/commit/ab170acf2748ffacefa71851427158bea6e3a542)).
- **Strict PDF pagination:** `--pdf-strict-pagination` (bare, `=true` or
  `=false`) in `fmd render`, `watch`, `batch` and `book`, and the config key
  `pdf_strict_pagination`. It implies `--pdf-optimal-pagination` and makes the
  exact page planner's hard constraints binding: two-line minimums on split
  paragraphs, headings kept together and with what follows, forced breaks, no
  split inside a wrapped table row or between a table header and its first
  row, and protected hyphen breaks. When no plan meets them, rendering stops
  before writing with exit 70 and a `pagination_*` code, and an existing PDF
  stays in place. With `--fit-to-pages`, only exact plans count. Library:
  `PdfOptions::strict_pagination`, `RenderError::Pagination` ([bcba460](https://github.com/Dicklesworthstone/franken_markdown/commit/bcba460e9e2e684ea17bf69acc62e2a367517a23)).
- **PDF baseline grid:** `--pdf-baseline-grid-pt <POINTS>` (1 to 144; `0` turns
  off a configured grid) and the config key `pdf_baseline_grid_pt` align body
  text, headings, code, tables, lists, quotes and page notes to a grid that
  starts at each page's top content edge, in render, watch, batch and book.
  Figures keep their measured height and the grid resumes after them; running
  headers and footers keep their own placement. If less than 40 pt of body
  height would remain, rendering fails instead of clipping, and a grid cannot
  be combined with `--fit-to-pages`. Library: `BaselineGrid`,
  `PageStyle::baseline_grid` ([a35d2a8](https://github.com/Dicklesworthstone/franken_markdown/commit/a35d2a8345d2e51e0fd90882ba58453e1c9ec495)).
- **PDF pagination reports:** `render_pdf_reported` and
  `render_pdf_document_reported` return the PDF bytes with a
  `PdfPaginationReport` built from the same layout: which page-break strategy
  ran (`ExactHeight`, `Legacy` or `Greedy`), the planner's failure when it fell
  back, and what the paragraph-alternative search tried. With
  `--pdf-optimal-pagination`, the PDF warnings and `fmd verify` findings name a
  fallback (`pagination_infeasible`, `pagination_budget_exceeded` and the other
  `pagination_*` codes) and `paragraph_pagination_limited`. The strategy
  selection is unchanged; the report records which path ran ([a341e9c](https://github.com/Dicklesworthstone/franken_markdown/commit/a341e9c84910cf6e54f45ea0a4706accca37d7fe)).
- **Financial tables:** `---.` in a pipe-table delimiter row aligns that column
  on the decimal point, and a following row of `---` or `===` markers draws a
  single or double rule under the cell above. PDF (tagged and paginated), HTML,
  EPUB and SVG draw them; plain GFM tables are unchanged. See
  `docs/FINANCIAL_TABLES.md` ([607b233](https://github.com/Dicklesworthstone/franken_markdown/commit/607b233364bb24c8215403207356e7e2769fb07b)).
- **Wide PDF tables:** a table wider than the page is split into successive
  column panels. The first column repeats as row context when at least two
  columns fit; on very narrow pages each panel holds one column in source
  order. Alignment, financial rules, links, empty cells, accessible headers and
  vertical pagination carry into the panels ([78252eb](https://github.com/Dicklesworthstone/franken_markdown/commit/78252ebbdb669b721a7bccd72e7b6e96a2127e86)).
- **PDF bookmarks follow the heading hierarchy:** a skipped heading level nests
  under the nearest preceding lower-level heading. Alert labels no longer take
  heading anchors or bookmarks, so local links, the table of contents and book
  links reach the real headings ([bf2b706](https://github.com/Dicklesworthstone/franken_markdown/commit/bf2b706fb02a1531606d4c7fbb13fa73921e0030)).
- **GIF images:** GIF87a and GIF89a files work as local images, `--pdf-image`
  bindings (extensionless keys too), batch and book inputs, and running header
  and footer images. HTML, SVG and EPUB keep the original bytes, animation
  included; PDF paints the first frame ([860fb42](https://github.com/Dicklesworthstone/franken_markdown/commit/860fb42dea1ef4a2a7ef235b6ee188ac5d5cd1ef)).
- **Embedded `data:` images:** bounded PNG, JPEG, GIF and SVG `data:` URLs in
  Markdown images and supported HTML `<img>` elements (in prose, headings and
  table cells) are drawn in PDF, kept as image resources in HTML and SVG, and
  packaged once in EPUB and multi-chapter books. An explicit `--pdf-image` or
  `image_assets` binding takes precedence, even an invalid one. `data:` links
  stay blocked ([8b0d88d](https://github.com/Dicklesworthstone/franken_markdown/commit/8b0d88dbd5b121536460d3e818e01badb364815a), [bab48a8](https://github.com/Dicklesworthstone/franken_markdown/commit/bab48a807c9d0c57a99a3e433884bb892755a332)).
- **CMYK JPEGs in PDF:** baseline and progressive Adobe CMYK and YCCK JPEGs are
  embedded as DeviceCMYK with matching decode parameters and their EXIF
  orientation, also inside SVG images, inline content and running bands.
  Four-channel JPEGs with missing or conflicting colour metadata keep the
  earlier fallback ([44c88dc](https://github.com/Dicklesworthstone/franken_markdown/commit/44c88dc5a29dc6edab0ea3e2ba690107bc97b544)).
- **JPEG in SVG figures, and EXIF orientation:** PDF draws JPEG `data:` URLs
  inside SVG figures through its JPEG image path. JPEG EXIF orientation (all
  eight cases) applies to standalone, inline, gallery, running-band and SVG
  images: layout uses the displayed size, and the original JPEG bytes are
  embedded ([b692f04](https://github.com/Dicklesworthstone/franken_markdown/commit/b692f04789b123eff275d5d2203719e56c32c853), [29eb12a](https://github.com/Dicklesworthstone/franken_markdown/commit/29eb12a1de96ff5e97de86799392235a7b4155aa), [cc470d1](https://github.com/Dicklesworthstone/franken_markdown/commit/cc470d1a0eaaec94634deb290faf3b9714e9bff2)).
- **CFF OpenType host fonts:** `--pdf-font SLOT=PATH` and `FontAssets` accept
  static, name-keyed CFF1 OpenType (`.otf`) faces in all five slots. PDF embeds
  them as CIDFontType0 subsets, SVG draws their cubic outlines, HTML keeps the
  OpenType flavour in raw, WOFF and WOFF2 containers, and EPUB packages them as
  `font/otf`. CFF2 and CID-keyed CFF are not supported, and glyph programs the
  subsetter cannot carry (such as `seac`) are a typed error
  ([008c624](https://github.com/Dicklesworthstone/franken_markdown/commit/008c62460deb551c9e38ba53bf9a2650d73de23c), [09bc66c](https://github.com/Dicklesworthstone/franken_markdown/commit/09bc66c592435ae759a75f4e300844887f8a0960), [a8b0cae](https://github.com/Dicklesworthstone/franken_markdown/commit/a8b0caecbbc50f7f7086f197be364e7c6a3614e3)).
- **Complete font embedding:** `--font-embedding subset|full` (config key
  `font_embedding`, MCP field `fontEmbedding`) for `fmd render`, `batch`,
  `watch`, and the HTML and EPUB targets of `fmd book`. `full` keeps the whole
  cmap and the GSUB, GPOS, GDEF and kern tables of HTML and EPUB fonts, so
  browsers and reading systems can apply ligatures, kerning and mark
  positioning; EPUB then also embeds the bundled faces when no host fonts are
  given. `subset` stays the default. `fmd render` also accepts the flag with
  `--to pdf` and `--to svg`, and their output is the same under either value.
  Library: `FontEmbedding`, `HtmlOptions::font_embedding` ([9fd8f0f](https://github.com/Dicklesworthstone/franken_markdown/commit/9fd8f0fad6b8ad49c7100e743ff584d468b575c8), [78d474f](https://github.com/Dicklesworthstone/franken_markdown/commit/78d474f259253eb226a7cc9d8ab7de777d332fe6)).
- **Bundled CJK fallback:** a Noto Sans SC subset (28,215 mapped characters:
  BMP Han, hiragana, katakana, CJK punctuation and fullwidth forms) backs CJK
  text. PDF and SVG draw its outlines; HTML and EPUB embed a document-specific
  subset and leave it out of Latin-only output; `--font-embedding full` and
  editable HTML keep the whole face. `fmd doctor fonts --corpus` reports Han as
  fallback-covered. The face has Simplified Chinese glyph forms and horizontal
  layout only: no Hangul, no supplementary-plane Han, no vertical text.
  Library: `fonts::cjk_bytes`, `fonts::load_cjk`, and `fmd-font`'s
  `bundled::noto_sans_cjk_sc` behind its new `cjk-face` feature
  ([140f77d](https://github.com/Dicklesworthstone/franken_markdown/commit/140f77d1ab7bed88fcf840e44371b43555e0ad05), [884532c](https://github.com/Dicklesworthstone/franken_markdown/commit/884532cc17d9352534b34390794f4cc4e9a1b77e), [86ee682](https://github.com/Dicklesworthstone/franken_markdown/commit/86ee6821dd1db3292e8465101bb251503aba560d)).
- **Mermaid pie charts:** `pie` fences (optional `showData` and `title`, quoted
  unique labels, positive values with up to two decimals) become vector
  figures in HTML, tagged PDF, SVG and EPUB. Limits: 64 KiB of source, 64
  slices and 512 bytes per label; anything else keeps the fence as source
  ([fca51e3](https://github.com/Dicklesworthstone/franken_markdown/commit/fca51e36215e1935ed108f6ecf2a91004ae08782)).
- **Mermaid flowcharts:** chained and grouped links (`A & B --> C & D`),
  semicolon-separated statements, later node declarations, open, dotted and
  thick edges with in-link text, `RL` and `BT` directions, and circle nodes.
  Limits: 64 KiB, 256 nodes and 1,024 expanded edges; subgraphs, style, class
  and click directives and extended shapes keep the whole block as source
  ([807a22b](https://github.com/Dicklesworthstone/franken_markdown/commit/807a22ba824890ed4a3a4f173463b2a3503bfe3f)).
- **Mermaid sequence diagrams** are read as a whole: participant order and
  aliases, `actor`, all ten standard arrows, self-messages, activation bars,
  nested `loop`, `alt`, `opt`, `par`, `critical` and `break` frames, notes,
  `autonumber`, titles and accessibility metadata. Limits: 64 KiB, 64
  participants, 1,024 events and 16 nested frames; an unsupported statement
  keeps the fence as source ([71f15b6](https://github.com/Dicklesworthstone/franken_markdown/commit/71f15b6a8bf9612182d70f6e6cc3a40ac1dc06bf)).
- **Interactive HTML keeps document presentation:** when a non-default theme,
  custom CSS, supplied fonts, a TOC or a non-default HTML font format is set,
  `--to interactive-html` shows the document in a sandboxed `iframe` with a
  restrictive Content-Security-Policy, so its CSS and heading IDs cannot style
  or imitate the editor's controls. Custom CSS replaces the default document
  stylesheet, and supplied images and fonts survive edit, save and reopen.
  Without these options the output is as before ([398245a](https://github.com/Dicklesworthstone/franken_markdown/commit/398245a433523636681dd34e02744a54a4909294)).
- **Book publishing:** native book publication supports configured
  typography, optimal pagination, running titles and logos, PDF/A-2b, and page
  budgets checked against the actual emitted PDF. Persistent font scale,
  language, contents and HTML font settings carry through books, with explicit
  flags taking precedence. Failed budgets or archival rendering preserve prior
  combined outputs, and every mapped image remains protected even when a later
  mapping replaces it ([47f6bc0](https://github.com/Dicklesworthstone/franken_markdown/commit/47f6bc0f9f52d1416bc7a1ed1de111b2b3f51d87), [74660d6](https://github.com/Dicklesworthstone/franken_markdown/commit/74660d6ea4bb05a84317fc8f09253fee780a08e8)).
  - New `fmd book` flags include `--microtype`, the `--typography-*` modes,
    `--pdf-optimal-pagination`, `--fit-to-pages` (alias `--target-pages`),
    `--pdf-base-font-size`, `--pdf-page-numbers`, the `--pdf-header-*` and
    `--pdf-footer-*` running content, repeatable `--pdf-image DEST=PATH`,
    `--pdf-a {2b,off}` and `--pdf-a-strict`.
- **Archival PDF in `fmd batch`:** `--to pdf` and `--to both` accept
  `--pdf-a 2b` and `--pdf-a-strict`, which 0.6.0 refused. Each file uses
  `fmd render`'s emission and strict URI checks; a strict rejection fails that
  file (exit 70 without `--continue-on-error`) and leaves both outputs of its
  HTML/PDF pair untouched. Library: `batch::render_batch_pdfa`,
  `batch::run_batch_blocking_pdfa`, `BatchPdfAError` ([3b3b72f](https://github.com/Dicklesworthstone/franken_markdown/commit/3b3b72feb5eaf1234938d13eaf7c6eac9b00dfd3)).
- **`fmd batch --to svg` accepts `--svg-width-pt`;** other targets still refuse
  it, now with "`--svg-width-pt requires --to svg in fmd batch`" ([8029c89](https://github.com/Dicklesworthstone/franken_markdown/commit/8029c89d4321308c7ba52510051c30f9c9ef7a86)).
- **MCP book tools:** `fmd.render_book` (explicit chapter and include sources;
  `to` is `pdf`, `epub`, `site` or `all`; artifacts returned as base64 in a JSON
  receipt within `maxOutputBytes`) and `fmd.verify_book` (the navigation report
  without rendering). `strictLinks` fails before output on navigation findings,
  and `pdfA` and `pdfAStrict` apply to `pdf` and `all`. `tools/list` now has 7
  tools, and `fmd.capabilities` reports `mcp_book`
  ([b741725](https://github.com/Dicklesworthstone/franken_markdown/commit/b741725b3ef570d0424acfa4f9919c3ac769e553), [0920e79](https://github.com/Dicklesworthstone/franken_markdown/commit/0920e79961aa44910e67554917f7330ab23da768), [d5cf342](https://github.com/Dicklesworthstone/franken_markdown/commit/d5cf3429439ca65cfc299c8877d03d007372086b)).
- **LSP heading rename:** the language server renames top-level heading titles
  and their real Markdown links across synchronized open documents. Versioned
  edits preserve source paths, queries, delimiters and Unicode coordinates;
  parser-verified source tokens and complete AST checks reject partial changes,
  shared image references, ambiguous file aliases and heading or footnote
  anchor collisions ([fe4367e](https://github.com/Dicklesworthstone/franken_markdown/commit/fe4367e2e24157b99fd2f9120da98d783f266c26), [57807a2](https://github.com/Dicklesworthstone/franken_markdown/commit/57807a2d4b7201d51c7186c0f9b5a1baef535341)).
- **LSP footnotes:** the optional language server supports exact footnote
  definitions, references, highlights and label-only renames through versioned
  document edits. Parser-proven source mapping excludes literal lookalikes;
  rename checks preserve Markdown semantics, document/session limits and
  generated anchors, including links through alternate spellings of an opened
  file URI ([e13275f](https://github.com/Dicklesworthstone/franken_markdown/commit/e13275f1e22d2f137fc25759e84ebf8e356c1b25)).
- **LSP heading references and workspace diagnostics:** `fmd-lsp` answers
  references and document highlights for headings across open buffers. It
  reports file-addressed fragment links between open buffers that are missing,
  invalid or ambiguous (`workspace_missing_anchor`,
  `workspace_invalid_fragment`, `workspace_ambiguous_anchor`), or that it cannot
  check yet (`workspace_target_unsynchronized`,
  `workspace_link_analysis_incomplete`), and refreshes them when the target
  changes ([d2b4360](https://github.com/Dicklesworthstone/franken_markdown/commit/d2b4360e64ae188aa9bfda06a4798a4d2ca57bdb), [398008d](https://github.com/Dicklesworthstone/franken_markdown/commit/398008d09b4fdba382a6f737071517bda2546123)).
- **LSP reference-label rename:** `prepareRename` and `rename` for Markdown
  reference labels (definitions and full, collapsed and shortcut links and
  images). Visible text, destinations and titles are kept; the server parses
  the edited document again, requires it to match, and refuses collisions and
  partial mappings. Like the other renames it needs a client with versioned
  `documentChanges`; limits are in `docs/LSP_REFERENCE_RENAME.md` ([e9f6a10](https://github.com/Dicklesworthstone/franken_markdown/commit/e9f6a1059a3a78d6c98a0220162438df02c9e72a)).
- **Page geometry API:** `PageSize::named` (`letter`, `a4`, `a5`, `legal`,
  `tabloid`) and `PageStyle::from_points(size, margins)` expose the paper
  catalog and margin checks that native config and MCP use ([448bbe3](https://github.com/Dicklesworthstone/franken_markdown/commit/448bbe3913970c9c6bf954c119b3b1865ddfb8d6)).
- **Book PDF/A API:** `BookRenderer::render_pdf_pdfa` and
  `BookRenderer::render_pdf_with_options` emit chapter-aware PDF/A from a
  retained book ([74660d6](https://github.com/Dicklesworthstone/franken_markdown/commit/74660d6ea4bb05a84317fc8f09253fee780a08e8)).
- **Link analysis API:** `DocumentLinkAnalysis::with_file_references` and
  `DocumentLinkAnalysis::resolve_fragment` ([b165d3d](https://github.com/Dicklesworthstone/franken_markdown/commit/b165d3df8e82de996c2c7682bd2b0be034ba1486), [a1a785e](https://github.com/Dicklesworthstone/franken_markdown/commit/a1a785e3f859d9029e2efc5059ad0554f522a0d1)).
- **Parser span API:** `parse::DestinationSpan`, `parse::destination_spans` and
  `DestinationSpan::fragment_range` are public, and
  `parse::reference_label_spans` lists reference-label occurrences with their
  form ([fe4367e](https://github.com/Dicklesworthstone/franken_markdown/commit/fe4367e2e24157b99fd2f9120da98d783f266c26), [e9f6a10](https://github.com/Dicklesworthstone/franken_markdown/commit/e9f6a1059a3a78d6c98a0220162438df02c9e72a)).
- **Threaded PDF compression:** the opt-in feature `native-compression` (no
  dependencies; not in `default`, `cli` or `batch`) lets a native host set
  `PdfEmitOptions::compression` to `PdfCompression::NativeWorkers` with 1 to 4
  workers, which compress page streams once at least 256 KiB of uncompressed
  streams exist. `PdfCompression::Sequential` is the default; other worker
  counts, or workers without the feature or on WASM, are
  `RenderError::InvalidInput`. No CLI flag or config key selects it
  ([868359b](https://github.com/Dicklesworthstone/franken_markdown/commit/868359b5678c3a0715f3730bc537dc23622bee57)).
- **Flow display list:** the `flow_display` and `display` modules typeset `$$`
  blocks, `math` fences and inline `$…$` as vector paths from `fmd-math`, shape
  CJK and bundled emoji text when `cjk-face` or `emoji-face` is compiled in, and
  draw Mermaid flowchart, sequence and pie figures with stroked paths; reading
  nodes keep the exact source. No shipped surface consumes these yet
  ([edb3921](https://github.com/Dicklesworthstone/franken_markdown/commit/edb39212f45477648937213f48f2595901f2f62d), [2fee8ba](https://github.com/Dicklesworthstone/franken_markdown/commit/2fee8badc9ae83ad3e3337ea8876bde6ee64d190), [691b11a](https://github.com/Dicklesworthstone/franken_markdown/commit/691b11a679a33539a821b2e1599ea47d0e868b4d)).
- **`fmd-font` AMS and Euler Fraktur faces:** behind `bundled-faces`, nine more
  faces converted from the AMS Type 1 fonts and renamed (SIL OFL): AMS-A
  symbols (msam) and AMS-B symbols and blackboard capitals (msbm) at 10, 7 and
  5 pt, and Euler Fraktur (eufm) at 10, 7 and 5 pt, as `bundled::AMS_A_*`,
  `AMS_B_*` and `EULER_FRAKTUR_*`. Hashes are in
  `fonts/cm-math/PROVENANCE.json`, and `fmd-math` packages `OFL-AMS.txt`
  ([a341047](https://github.com/Dicklesworthstone/franken_markdown/commit/a3410479dda20a135b7a35455b0d6707b5ce22c3), [662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3)).
- **`fmd-font` bounds and run metrics:** `Font::font_bbox`,
  `Font::cff_glyph_bbox` and `TextRunMetrics` (re-exported at the crate root)
  ([09bc66c](https://github.com/Dicklesworthstone/franken_markdown/commit/09bc66c592435ae759a75f4e300844887f8a0960), [2fee8ba](https://github.com/Dicklesworthstone/franken_markdown/commit/2fee8badc9ae83ad3e3337ea8876bde6ee64d190)).
- **`fmd-math` colour:** `fmd-math` honours `\color` and `\textcolor`. They used
  to parse and then be silently ignored, and `\textcolor` failed as untiered
  ([d9c13d7](https://github.com/Dicklesworthstone/franken_markdown/commit/d9c13d7651a22e991fae8203deab3d5d7b5a057d), [77a822f](https://github.com/Dicklesworthstone/franken_markdown/commit/77a822fc49155866acb9098e3571b475d8fee3a1)).
  - The colour surface is plain xcolor's: the 19 base names, mixes such as
    `red!30!blue` and `blue!70`, a leading `-` for the complement, and the
    `[rgb]`, `[RGB]`, `[HTML]` and `[gray]` models. Mixes are linear in RGB.
    Anything else is a precise parse error naming the supported set.
  - Scoping is TeX's: a declaration colours the rest of its group, and
    `\textcolor{c}{…}` is `{\color{c}…}`. Colour never changes the layout.
  - API: `PlacedGlyph`, `PlacedRule` and `PlacedPath` gain
    `color: Option<Color>` (RGB from 0 to 1; `None` keeps the consumer's own
    colour). `NodeKind::ColorChange` carries a `ColorSpec`: the MathML spelling
    plus the resolved `Color`, or the error that layout reports for a colour
    xcolor does not define. MathML's `mathcolor` keeps a bare colour verbatim,
    so HTML still hands CSS names and `#hex` to the browser, and writes
    `#rrggbb` for an explicit `[model]`. PDF and SVG now preserve each glyph,
    fraction rule and constructed path's resolved colour. Uncoloured parts
    keep the theme or surrounding link/quote ink, and scoped declarations
    cannot recolour following prose. Geometry and uncoloured output stay
    unchanged.
- **`fmd-math` text font switches:** `\textit`, `\textrm`, `\textup`,
  `\textnormal`, `\texttt` and `\textsf` set their whole argument in one face,
  in math and text mode. `\textit` and `\emph` end with cmti10's italic
  correction, except before a period or comma ([19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e), [da2607d](https://github.com/Dicklesworthstone/franken_markdown/commit/da2607d5836ba038fb6ff6d69a1a994d147c1b2f)).
- **`fmd-math` constructs:** `\not` (cmsy's negation slash, as a relation),
  `\lvert`, `\rvert`, `\lVert`, `\rVert`, `\hspace{len}` and `\hspace*{len}`
  (pt, pc, in, bp, cm, mm, dd, cc, sp, em and ex; a rubber length is an
  error), `\boxed`, `\cfrac[l|c|r]`, `\genfrac`, `\mathscr`, and the `gather*`,
  `gathered` and `equation*` environments. MathML output covers them
  ([19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e)).
- **`fmd-math` `\mathfrak` and cancel strokes:** `\mathfrak` draws Euler
  Fraktur, and `\cancel`, `\bcancel` and `\xcancel` strike their argument
  (MathML `<menclose>`) ([662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3)).
- **`fmd-math` `\TeX`, `\LaTeX` and `\mathchoice`:** the logos are built from
  Computer Modern glyphs in math and text mode, and `\mathchoice{D}{T}{S}{SS}`
  takes the branch for the current style. `\LaTeXe` is not supported
  ([e6da6ee](https://github.com/Dicklesworthstone/franken_markdown/commit/e6da6ee1a73e636df49484fec27a6daea495732e)).
- **`fmd-math` `\iiiint`** joins the multiple-integral family (see Changed)
  ([439a910](https://github.com/Dicklesworthstone/franken_markdown/commit/439a910ce7c13d96489dcf95e525fb195facd4ef)).
- **`fmd-math` dots:** `\dotsm` and `\dotsi` are accepted, and `\dots`,
  `\dotso`, `\dotsc`, `\cdots` and `\dotsb` look at the next token
  ([1ffcadc](https://github.com/Dicklesworthstone/franken_markdown/commit/1ffcadc92e9f49ebd73c2c7104c246815465c906), [d99fee9](https://github.com/Dicklesworthstone/franken_markdown/commit/d99fee964a47fa13565fca55cc2f995e16d9e230)).
- **`fmd-math` class primitives:** `\mathord`, `\mathbin`, `\mathrel`,
  `\mathopen`, `\mathclose`, `\mathpunct` and `\mathinner` ([33e4bb4](https://github.com/Dicklesworthstone/franken_markdown/commit/33e4bb4a4b111aeeba1a00fea14193b099ea9ce7)).
- **`fmd-math` `amssymb` names:** amssymb's symbol names that have a single
  Unicode character are accepted and draw from the bundled AMS faces at the
  family and code `amssymb.sty` gives them. The six without one (`\nleqslant`,
  `\ngeqslant`, `\nleqq`, `\ngeqq`, `\nsubseteqq`, `\nsupseteqq`) are not
  supported ([19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e), [ea0bd24](https://github.com/Dicklesworthstone/franken_markdown/commit/ea0bd243d049e69344c1e79408eb6e1f4311cb8d), [683e4cc](https://github.com/Dicklesworthstone/franken_markdown/commit/683e4cccb66bc8ea4a412d2665663d2c47f159f1)).
- **`fmd-math` resource budgets:** `Budgets` (`DEFAULT`: 65,536 source bytes,
  65,536 glyphs, 262,144 path segments), `Engine::with_budgets`,
  `Engine::budgets`, `BudgetKind` and `MathError::Budget` ([9bbb40a](https://github.com/Dicklesworthstone/franken_markdown/commit/9bbb40ad5472e729ae2fe79052b4b039b9e0f7c1)).
- **`fmd-math` macro sets:** `MacroSet::overlay_definitions` copies another
  table's definitions over this one, keeping native bindings and the
  receiver's ligature policy; `MacroSet::physics()` and `MacroSet::line_width()`
  report the new default-pack settings (see Changed)
  ([c81208f](https://github.com/Dicklesworthstone/franken_markdown/commit/c81208f19a81103daf41aa1d8e4a6153c22f7b71), [33e4bb4](https://github.com/Dicklesworthstone/franken_markdown/commit/33e4bb4a4b111aeeba1a00fea14193b099ea9ce7), [61304cc](https://github.com/Dicklesworthstone/franken_markdown/commit/61304ccc480756132af268fc558804ef6e45da34)).
- **`fmd-math` TFM tables:** the public `tfm` module holds `TfmFont`, `TfmChar`
  and the metrics of cmmi, cmsy and cmr at 10, 7 and 5 pt, cmex10 and cmex7,
  cmti10, msam and msbm at 10, 7 and 5 pt, and eufm at 10, 7 and 5 pt
  ([4315c77](https://github.com/Dicklesworthstone/franken_markdown/commit/4315c778e5a19d08cf6ac96012aa060cb7cd96b6), [abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d), [a341047](https://github.com/Dicklesworthstone/franken_markdown/commit/a3410479dda20a135b7a35455b0d6707b5ce22c3)).
- **`fmd-math` `svg_dump` example** for visual checks of layout changes
  ([ddebab7](https://github.com/Dicklesworthstone/franken_markdown/commit/ddebab74973bb11e952e64992ddc5c2752583436)).

### Changed

- **HTML blocks follow CommonMark 0.31.2 (conformance fix):** a line holding
  only a complete open or closing tag whose name is not one of CommonMark's
  block-level names, such as `<my-tag>`, now starts an HTML block of type 7
  (spec §4.6, start condition 7) that runs to the next blank line, as in spec
  Examples 162 and 163. Before, it rendered inside a paragraph
  (`<p><my-tag>…</p>`). A type 7 block still cannot interrupt a paragraph.
  A line starting `<!` that is not a comment (`<!--`), a CDATA section
  (`<![CDATA[`) or a declaration (`<!` followed by an ASCII letter) no longer
  opens an HTML block: `<!1>` is paragraph text, rendered as
  `<p>&lt;!1&gt;</p>` (§4.6 start condition 4, §6.6). Before, it passed
  through as raw HTML. Documents containing such lines render differently
  ([67edda4](https://github.com/Dicklesworthstone/franken_markdown/commit/67edda4e22bb9098ac5bea901acab196094859de); tests updated in
  [7b44244](https://github.com/Dicklesworthstone/franken_markdown/commit/7b44244aba9cdf2c58357d2880fee50e3aafc8ca)).
- **Optimal pagination covers more documents:** with `--pdf-optimal-pagination`
  (`PdfOptions::optimal_pagination`), joint paragraph and page selection now
  also runs in documents with page-bottom footnotes and inside blockquotes, and
  offers alternative shapes for list paragraphs (one marker, hanging indent,
  task boxes, nesting) and for paragraphs with inline images or formulas, whose
  sizes never shrink ([dd92b4e](https://github.com/Dicklesworthstone/franken_markdown/commit/dd92b4ec671437c22e8441977674e15adc120257), [cd6ba8c](https://github.com/Dicklesworthstone/franken_markdown/commit/cd6ba8caba64fb38010bfe40ea9a22f7716d9638), [b28ff87](https://github.com/Dicklesworthstone/franken_markdown/commit/b28ff870803632fe06c8f7789d4bbf6ae97d6798)).
- **Captions and introductions stay with their subjects:** in exact pagination
  (`--pdf-optimal-pagination` or `--pdf-strict-pagination`), a complete
  paragraph of one to three lines that introduces a figure, table, code block
  or list is kept together and with what follows; it never overrides an
  explicit page break, and strict exports reject groups that cannot be placed.
  The planner also records whether a break continues a hyphenated word, so
  strict exports of missing-value tables, source code and hard-break prose
  ending in a hyphen no longer fail ([e2e7c81](https://github.com/Dicklesworthstone/franken_markdown/commit/e2e7c81c72ee24ea39ae3e259cf46c259b56c45e), [64a5419](https://github.com/Dicklesworthstone/franken_markdown/commit/64a5419a736f259a05b20b423b1942dc806200b2)).
- **`--pdf-image` covers every format:** `DEST=PATH` bindings apply to HTML,
  SVG and EPUB as well as PDF, and to stdin and `--text` input; `DEST` may be an
  extensionless key or a URL; watch also watches mapped files; and
  `--max-pdf-image-bytes` applies to every format. HTML takes the image type
  from the bytes (PNG, JPEG, or SVG with a real `<svg>` root) instead of the
  file extension ([39601d6](https://github.com/Dicklesworthstone/franken_markdown/commit/39601d64a79daf6cf0bc5b3d1c693993857dfeb0)).
- **Theme colours agree across formats:** HTML, PDF and EPUB take code-token
  colours from one palette (`Theme::code_colors`) that follows the effective
  theme. A fixed HTML appearance no longer yields to the reader's
  `prefers-color-scheme`. PDF text, math, code and page background colours come
  from the effective theme, and a non-white page background is painted;
  backgrounds that are not 3- or 6-digit hex keep the white page ([33a43b2](https://github.com/Dicklesworthstone/franken_markdown/commit/33a43b2f0f5cbf149c3688c373d32b7ababe8a58)).
- **Running band errors:** `pdf_running_header_does_not_fit` and
  `pdf_running_footer_does_not_fit` also name `--margin-top-pt` and
  `--margin-bottom-pt`, and `fmd capabilities --json` lists
  `"image_positions":["left","right"]` under `pdf_running_content`
  ([aeeef16](https://github.com/Dicklesworthstone/franken_markdown/commit/aeeef16f1e843cef2b3c0654b0300a90b038df3a)).
- **`fmd book --toc-depth`** works for HTML and EPUB targets and limits the
  contents inside each chapter; 0.6.0 rejected it outside PDF with
  `unsupported_target_option` ([47f6bc0](https://github.com/Dicklesworthstone/franken_markdown/commit/47f6bc0f9f52d1416bc7a1ed1de111b2b3f51d87)).
- **`fmd watch --serve` previews come from the export:** the preview HTML is
  captured in the normal render, so PDF, SVG and EPUB watches show the same
  metadata, profile, theme, fonts and images, and the preview changes only
  after every requested export succeeds. `fmd watch --measure` returns the
  render failure status instead of ignoring it ([8cc1800](https://github.com/Dicklesworthstone/franken_markdown/commit/8cc18007698ab2b1a171da21ec5b56d029fbbb45)).
- **SVG posters split wide tables:** a table too wide for the poster continues
  in column panels down the page, keeping its headers and repeating the first
  column when two columns fit. Text size is unchanged ([72e8926](https://github.com/Dicklesworthstone/franken_markdown/commit/72e8926c5880d94a6593fec4ee7855cef6714dd8)).
- **MCP page sizes:** `page.size` accepts `a5`, `legal` and `tabloid` besides
  `letter` and `a4`, and `fmd.render_book` accepts `page` for `pdf` and `all`
  ([448bbe3](https://github.com/Dicklesworthstone/franken_markdown/commit/448bbe3913970c9c6bf954c119b3b1865ddfb8d6)).
- **Reference labels use Unicode full case folding** (Unicode 17, from generated
  tables with no runtime dependency) in the parser, the offline interactive
  renderer and the LSP, instead of lowercasing. First-definition precedence and
  source spans are unchanged ([2b02598](https://github.com/Dicklesworthstone/franken_markdown/commit/2b02598111dd46af7660775bb209a8655321beb6)).
- **One line-break definition:** LF, CRLF and a lone CR are the same line break
  in block parsing, frontmatter, source spans, inline destination maps and the
  offline interactive renderer, without rewriting the source bytes
  ([f88f0ea](https://github.com/Dicklesworthstone/franken_markdown/commit/f88f0ea138475b6ffa654988c92b3d31555dec27)).
- **GFM table rows without pipes:** after a valid header and delimiter row,
  body rows need no pipes, and missing cells stay empty. A single-column table
  needs a pipe in its header or delimiter row, so `Title` over `---` is still a
  setext heading. A table synthesizes at most 512K empty cells ([f32548e](https://github.com/Dicklesworthstone/franken_markdown/commit/f32548e71141091a546b4e1852014d1f9d44462f)).
- **Reference definitions of any length:** the eight-line lookahead is replaced
  by a bounded scan shared by block parsing and container tails ([34da441](https://github.com/Dicklesworthstone/franken_markdown/commit/34da4415089a94183801feffb8da316637940133)).
- **`fmd-font` `Font::glyph_bbox` handles CFF1:** it returns ink bounds from the
  cubic outlines, rounded outward, for CFF fonts, and `None` for a CFF glyph it
  cannot decode ([09bc66c](https://github.com/Dicklesworthstone/franken_markdown/commit/09bc66c592435ae759a75f4e300844887f8a0960)).
- **Crate package:** the `franken_markdown` package no longer includes the
  `ios/` app sources, so the `.crate` stays under the crates.io 10 MiB upload limit ([c4df3f7](https://github.com/Dicklesworthstone/franken_markdown/commit/c4df3f78d3354237aeba494ab1abbefb8348ead0)).
- **`fmd-math` faces:** `fmd-math` sets mathematics in the faces TeX uses.
  `fmd-font` bundles the AMS Type 1 Computer Modern math fonts, converted to
  TrueType and renamed (cmmi and cmsy at 10, 7 and 5 pt, cmex10, cmr7 and cmr5;
  SIL OFL; see `fmd-font/fonts/cm-math/`). PDF and SVG math from `fmd` changes
  with them ([ddebab7](https://github.com/Dicklesworthstone/franken_markdown/commit/ddebab74973bb11e952e64992ddc5c2752583436)).
  - Math letters and lowercase Greek come from cmmi instead of the text italic.
    θ no longer draws as ϑ, and α no longer reads as a Latin a.
  - Relations, operators, arrows and `{ } |` come from cmsy instead of the Noto
    fallback.
  - Big operators use cmex's own text and display glyphs instead of a scaled
    fallback glyph.
  - `\big`-family and `\left…\right` delimiters use cmex's sizes and
    extensible pieces instead of drawn paths.
  - Scripts use the optically designed 7- and 5-point cuts.
  - `\mathcal` is cmsy's calligraphic alphabet.
  - `\neq`, `\notin` and `\not\equiv` overprint cmsy's negation slash, as TeX
    does.
  - Noto Sans Math remains the last-resort fallback.
  - Layout goldens moved with the faces.
- **`fmd-math` family 0:** `fmd-math` sets math-mode family 0 (digits,
  `+ = ( ) [ ]`, upright Greek, operator names, `\mathrm`, and the accents) from
  cmr10 itself at display and text size. `fmd-font` bundles it like cmr7 and
  cmr5 (`FmdCmRoman10.ttf`); the other nine converted faces are unchanged
  ([49466bc](https://github.com/Dicklesworthstone/franken_markdown/commit/49466bc2dc4e9a34e070904bf6524a5c2a451168)).
  - CM Unicode's Regular had stood in for cmr10, but its glyphs, `+`, `=` and
    the brackets among them, are not cmr10's (franken_manim fm-8zy2).
  - Text mode, including TexText runs, keeps CM Unicode.
  - Boxes, kerns and positions are unchanged, since both faces read cmr10's TFM.
    Eleven layout goldens moved: the faces and outlines of those glyphs only,
    and the accents, which now draw cmr10's own accent at scale 1.
- **`fmd-math` TFM metrics:** `fmd-math` lays out those faces from TeX's own
  font metrics (`fmd-math/src/tfm.rs`, transcribed from Knuth's cmmi, cmsy,
  cmex and cmr TFM files at 10, 7 and 5 pt by
  `scripts/font-oracles/extract-cm-tfm.py`). In math mode a glyph of a TeX
  face, with CM Regular standing for cmr10, now gets:
  - Boxes: the TFM width, height, depth and italic correction instead of
    measured ink. `-` is 0.583 em tall with 0.083 depth, `\cdot` 0.444,
    `\rightarrow` 0.367, digits 0.644, and letters have no overshoot depth, as
    in TeX. `y` is 0.526 em wide with its 0.036 italic correction.
  - Kerns: those of the TFM lig/kern program. An Ord character kerns with the
    next atom's character from its font, whatever that atom's class (`d f` is
    1/6 em tighter, `V,` likewise).
  - Italic corrections: kept between letters of cmmi, cmsy and cmex, whose
    interword space is zero, and dropped inside a word of cmr or a text face.
  - Accents: an accent over a single character is skewed by that character's
    kern with its font's skewchar.
  - Big operators: their TFM boxes are centred on the axis. The display `\sum`
    box is 1.6 em, against 1.4 em of ink.

  Text mode keeps measured metrics. Ligatures stay off, as under the
  Reference preamble. All 17 layout goldens moved ([4315c77](https://github.com/Dicklesworthstone/franken_markdown/commit/4315c778e5a19d08cf6ac96012aa060cb7cd96b6), [abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d)).
- **`fmd-math` clean boxes:** `fmd-math`'s clean boxes keep a lone character's
  italic correction in their width, as TeX does: tex.web §721 frees the kern
  node but not the width its hpack gave the box. This covers fraction parts,
  scripts, limits, radicands, over- and underlined bodies, and accent bases:
  `\frac{f}{2}`'s numerator and `\tilde f`'s base are f plus δ wide, and a
  limit centres δ-wide. Two related corrections land with it ([216c7c3](https://github.com/Dicklesworthstone/franken_markdown/commit/216c7c3b0d99ddb56d85d8ff04b81a0984cfea8e)).
  - An accent's box is its TFM character's width plus italic correction
    (char_box, §709) at the size of the style it is set in, with that
    character's height. `\vec` sits 0.077 em further left.
  - A radical's box includes the overbar's kern of one rule thickness above
    the rule (§705, §737).
- **`fmd-math` AMS glyphs:** `\hbar`, `\hslash`, `\checkmark`, `\nmid`,
  `\nshortmid`, `\circlearrowleft`, `\circlearrowright` and `\minus` draw their
  authored glyph at its TeX code position in the AMS-A or AMS-B face, and
  `\mathbb` letters come from AMS-B ahead of Noto Sans Math. Scripts use the 7
  and 5 pt cuts and their TFM widths. `\minus` is narrower than `-`, which
  keeps its Computer Modern glyph. MathML keeps the Unicode character
  (`NodeKind::AmsSymbol`). A face roster with no AMS face keeps the Unicode
  fallback ([a341047](https://github.com/Dicklesworthstone/franken_markdown/commit/a3410479dda20a135b7a35455b0d6707b5ce22c3), [9b457a3](https://github.com/Dicklesworthstone/franken_markdown/commit/9b457a3895d77ebe25d2d45512a838e07556b424), [3a64b58](https://github.com/Dicklesworthstone/franken_markdown/commit/3a64b587ed83b6ab476b2e7aa720f92c89d632f7)).
- **`fmd-math` multiple integrals:** `\iint`, `\iiint` and the new `\iiiint` are
  cmex's `\intop` repeated, each after the first drawn back by `\intkern@`
  (−6 mu, and a further −3 mu in display style), following amsmath's
  `\MultiIntegral`; with `\limits` the limits centre over the whole row.
  `\iint` used to be a fallback double-integral glyph. `\oiint` keeps its own
  glyph, and `\idotsint` is still unsupported ([439a910](https://github.com/Dicklesworthstone/franken_markdown/commit/439a910ce7c13d96489dcf95e525fb195facd4ef)).
- **`fmd-math` `\vec`, tall radicals and tall delimiters:** `\vec` is cmmi's
  accent character at the style's optical size instead of a scaled fallback
  arrow. A radical sign taller than cmex's 3.0 em size is built from cmex's
  extensible pieces instead of drawn. Extensible pieces sit at their TFM height
  below their slot, which removes a 0.02 em gap under the top piece of tall
  parentheses ([439a910](https://github.com/Dicklesworthstone/franken_markdown/commit/439a910ce7c13d96489dcf95e525fb195facd4ef)).
- **`fmd-math` fractions are Ord atoms:** `\frac`, `\binom`, `\dfrac`, `\tfrac`,
  `\cfrac` and `\genfrac` no longer get Inner spacing, so a fraction next to an
  Ord, Open or Punct atom loses the thin space on each side ([ea0bd24](https://github.com/Dicklesworthstone/franken_markdown/commit/ea0bd243d049e69344c1e79408eb6e1f4311cb8d)).
- **`fmd-math` `\mod`, `\pmod` and `\pod`** use amsmath's style-dependent kern
  (18 mu in display style; 12 mu for `\mod` and 8 mu for `\pmod` otherwise) and
  set "mod" upright ([4b705d1](https://github.com/Dicklesworthstone/franken_markdown/commit/4b705d147c10cebb52866f37dbdb1492ce8505ea)).
- **`fmd-math` long arrows** (`\longrightarrow`, `\Longrightarrow`, `\mapsto`,
  `\hookrightarrow`, and their left, two-way and long-mapsto forms) are
  composed from Computer Modern relation pieces instead of one Noto glyph, and
  `\overset` and `\underset` take their class from amsmath's `\binrel@`, so
  `x\overset{?}{=}y` keeps its relation spacing ([094d4f6](https://github.com/Dicklesworthstone/franken_markdown/commit/094d4f6de4aeac2cf8ddcb593ef9dedb850a6a99)).
- **`fmd-math` delimiter sizes:** delimiters step through TeX's size chain
  (small variant, cmex7 and cmex10 successors, then an extensible);
  `\nulldelimiterspace` (1.2 pt) is no longer scaled in scripts; a radical's
  bar is the sign's height; and a bar extensible covers modest heights instead
  of starting at 3.0 em ([784a4c5](https://github.com/Dicklesworthstone/franken_markdown/commit/784a4c5c7e634846e43f9bf1a5d8e395c6508daf), [04d811d](https://github.com/Dicklesworthstone/franken_markdown/commit/04d811db8dbf9c55ae3e156cfb67482dcc7c3254)).
- **`fmd-math` script Greek capitals,** dotless i and ligatures use cmr7's and
  cmr5's own designs instead of scaled CM Regular ([4f2bcc9](https://github.com/Dicklesworthstone/franken_markdown/commit/4f2bcc959b21c53dd83efaecd5f14f908bf2ecda)).
- **`fmd-math` operator names** (`\log` and the rest) keep their last
  character's italic correction in the box ([33e4bb4](https://github.com/Dicklesworthstone/franken_markdown/commit/33e4bb4a4b111aeeba1a00fea14193b099ea9ce7)).
- **`fmd-math` `align*`** spreads its column pairs across the line width, as
  amsmath does, instead of placing them 10 pt apart; `aligned` is unchanged
  ([662c3f9](https://github.com/Dicklesworthstone/franken_markdown/commit/662c3f97017ddacfecdc7d28a175d136bc18d9e3)).
- **`fmd-math` `\dddot` and `\ddddot`** are amsmath's operator over a row of
  text periods instead of accents ([1ffcadc](https://github.com/Dicklesworthstone/franken_markdown/commit/1ffcadc92e9f49ebd73c2c7104c246815465c906)).
- **`fmd-math` refusal messages:** constructs it knows but does not support
  (`\tag`, `\sideset`, `\cancelto`, the numbered environments, `\genfrac` with
  one delimiter) cite franken_manim's `fm-tex-numbered-displays-bxl9`; `\dx`
  still cites `fm-j5t` ([2d820c5](https://github.com/Dicklesworthstone/franken_markdown/commit/2d820c52b718bd11f9ff8e6cdd1ca0b7938a06a8), [19a7d64](https://github.com/Dicklesworthstone/franken_markdown/commit/19a7d64b77ed40ee500e3f40cd7105268e05068e)).
- **`fmd-math` default macro pack** (`MacroSet::pack("default")`, which
  franken_manim uses and `fmd` does not): `\cos(x)`, the other trig functions,
  `\log` and `\ln` take an auto-sized parenthesis pair, `\cos[2](x)` raises the
  2, and `\exp`, `\det` and `\Pr` take braced, bracketed or parenthesized
  arguments. TexText paragraphs break at a 345 pt `\linewidth` with a port of
  TeX's line breaker (no hyphenation), a blank line ends a paragraph, and `\\`
  drops the glue before it. The empty and basic packs never break lines
  ([33e4bb4](https://github.com/Dicklesworthstone/franken_markdown/commit/33e4bb4a4b111aeeba1a00fea14193b099ea9ce7), [61304cc](https://github.com/Dicklesworthstone/franken_markdown/commit/61304ccc480756132af268fc558804ef6e45da34), [b4515fe](https://github.com/Dicklesworthstone/franken_markdown/commit/b4515fe2d7425e329d4b52e9997c641dad6eb8db)).

### Fixed

- **Book chapter links:** native HTML books resolve chapter links by their
  exact source identity. Missing nested siblings, alternate extensions and
  flattened-name lookalikes no longer redirect to unrelated chapters; real
  chapter aliases retain query strings and fragments across standalone and
  combined publication ([b26cb75](https://github.com/Dicklesworthstone/franken_markdown/commit/b26cb758204d028f91d0e284ede7018f66dfb5b2)).
- **Book images in raw HTML:** `fmd book` embeds `<img src>` images in chapter
  Markdown for HTML, PDF and EPUB, with per-chapter identity and the existing
  path, symlink and size protections ([ef57eb6](https://github.com/Dicklesworthstone/franken_markdown/commit/ef57eb62d6743f5caf3a2f72a44d4539ae5e7b62)).
- **Book includes and navigation:** HTML `href` and `src` attributes in included
  files resolve against the included file's directory across HTML sites, PDF
  and EPUB, and count in link reports, unresolved-link receipts and
  `--deny-broken-links`. EPUB navigation is built from the same headings as the
  chapter bodies, removed code links no longer create phantom chapter titles,
  and directory watch protects the included image paths ([3ea5bdf](https://github.com/Dicklesworthstone/franken_markdown/commit/3ea5bdffb501a5b91e8532270c5e31fc0a23885d)).
- **Book PDFs keep page footnotes with chapter links:** book links bind to the
  PDF writer's real heading destinations instead of assuming bookmark order,
  and linked page-bottom notes are kept when local chapter links exist
  ([85af626](https://github.com/Dicklesworthstone/franken_markdown/commit/85af626f8155d29e3818e8ab189de570acd4d156)).
- **Batch includes:** native batch rendering expands recursive includes, named
  snippets and line selectors through the single-file resolver, preserving
  fragment-relative images and links. Include failures remain per-file input
  errors; consulted sources are protected across jobs, including missing files
  and dangling aliases, so publishing outputs cannot change another job's
  include result ([6a423e5](https://github.com/Dicklesworthstone/franken_markdown/commit/6a423e55d9d4236a3411d837a8bc130b1bc4b21b)).
- **Batch outputs:** batch outputs preserve all planned source documents and
  loaded image, CSS, font and configuration files, including another job's
  inputs and filesystem aliases. Unsafe jobs fail before publishing either
  combined output; fresh destinations avoid the extra image scan ([a40ebde](https://github.com/Dicklesworthstone/franken_markdown/commit/a40ebde186c1c08cc713e020d668e4af5179ac32)).
- **Watch and fonts:** watch rebuilds when an explicitly supplied font changes
  or a missing font appears, including documents sharing that font in a
  watched directory ([a40ebde](https://github.com/Dicklesworthstone/franken_markdown/commit/a40ebde186c1c08cc713e020d668e4af5179ac32)).
- **Render and watch outputs:** native render and watch outputs preserve every
  loaded input. SVG files and search-index sidecars now share the
  source-overwrite guard with HTML, PDF and EPUB; included Markdown, CSS,
  configuration, fonts and local images are also protected, including symlink
  and hard-link aliases. A collision fails before rendering or writing any
  publication output ([1bf88e4](https://github.com/Dicklesworthstone/franken_markdown/commit/1bf88e471df537f8c8100aa2e2ed0b2d7a7e4e17)).
- **`fmd watch` on a directory refuses outputs that overwrite inputs:** before
  the first publication and after each refresh, outputs are checked against
  every root's Markdown, includes, images and host resources, including
  aliases, hard links and dangling symlinks. A blocked document reports
  "refusing to overwrite watched input …" as a usage error and is retried once
  the conflict is repaired, while other exports continue ([33d0fb2](https://github.com/Dicklesworthstone/franken_markdown/commit/33d0fb22aed50a86ced9c9382fd8770a882a1df5)).
- **`fmd diff --out` and `fmd verify --links-cache`** refuse a destination that
  aliases an input revision, the active config or the Markdown input (exit
  64), and write through a staged replace, so a failed write keeps the
  previous file ([48f5350](https://github.com/Dicklesworthstone/franken_markdown/commit/48f53508aee1ed5017641d6a6ca83522160003d1), [d661ee1](https://github.com/Dicklesworthstone/franken_markdown/commit/d661ee15d926f2ddeba994e94458e6758157e504)).
- **Tagged PDF tables:** tagged PDF tables retain blank headers, empty data
  cells and entirely blank rows in their original grid positions. Each data
  cell names its column header through a stable identifier and a sorted
  document lookup, including wrapped, repeated, nested and cached tables,
  without adding text or changing layout ([af4fc95](https://github.com/Dicklesworthstone/franken_markdown/commit/af4fc95757e0c68f125e5648a629bccedf364c3d)).
- **Tagged PDF across pages:** tagged PDF paragraphs, links, code blocks,
  quotes, lists and tables retain their logical identity across page breaks.
  Continued table headers are pagination artifacts, with annotations still
  owned by their original header cells; source rows, footnote containers and
  page-local formulas stay distinct ([defa7cd](https://github.com/Dicklesworthstone/franken_markdown/commit/defa7cde36bc87c7cbb4152227b3f828125a415b)).
- **Small-font footnote tables** no longer panic when adaptive scaling would
  place the body-table minimum above their nominal font size ([defa7cd](https://github.com/Dicklesworthstone/franken_markdown/commit/defa7cde36bc87c7cbb4152227b3f828125a415b)).
- **PDF inline images fit their container:** images are limited to the
  paragraph or table-cell width through measurement, line breaking, painting,
  links and tagged figure bounds, keeping their aspect ratio and bytes
  ([271c987](https://github.com/Dicklesworthstone/franken_markdown/commit/271c98750ad7147cb25cbafae19fc8e21f8a1dd1)).
- **PDF SVG geometry:** fractional `viewBox` units and the full precision of the
  outer SVG matrix are kept for figures, inline images, headers and footers, so
  small-coordinate artwork is not shrunk and large-coordinate SVGs are not
  rounded to zero scale ([e8fcc38](https://github.com/Dicklesworthstone/franken_markdown/commit/e8fcc38735dbcc5fe806c0ec699bdc911be80348)).
- **Inline `$$…$$` in PDF** is laid out in display style; it used to produce the
  same geometry as `$…$` ([2865b74](https://github.com/Dicklesworthstone/franken_markdown/commit/2865b741df24154716d60feaf3d5dbab6ee693a6)).
- **PDF inline formulas fit their measure:** an oversized inline formula scales
  down to the paragraph, heading, list, quote or table-cell width, and drawing,
  line metrics, links, Formula bounds and ActualText use the same placement;
  `math_overflow` reports the scaling. Inline code keeps repeated, leading and
  trailing spaces as fixed-width glyphs, wraps at separators or character
  boundaries, and no longer gets inserted hyphens or justification stretch
  ([fff29cd](https://github.com/Dicklesworthstone/franken_markdown/commit/fff29cdce123dc70912ca0616f1a5c209fc05066)).
- **PDF inline-math cache is bounded:** besides its 4,096-entry ceiling it has a
  32 MiB payload budget, and sources over 64 KiB are refused before they reach
  the cache ([e3a7218](https://github.com/Dicklesworthstone/franken_markdown/commit/e3a72189587247487074436fb2d48eef18922597)).
- **Tall inline formulas** are scaled to the printable page height instead of
  losing content, with the new `math_height_overflow` warning (also in
  `fmd verify`) ([8f4156e](https://github.com/Dicklesworthstone/franken_markdown/commit/8f4156ed90a2516804eea517dd48a24fbc4571cb)).
- **PDF text, code and lists stay inside the page:** prose and headings fit
  the width left after container indentation, with measured emergency breaks
  for overlong words and URLs; fenced code (text diagrams, line-number gutters,
  nested indentation) is kept or wrapped within the page; deeply nested lists
  and wide markers stay inside it ([c68d2db](https://github.com/Dicklesworthstone/franken_markdown/commit/c68d2db7f24973a06835420d74041ff6f6b77b83), [ff5878a](https://github.com/Dicklesworthstone/franken_markdown/commit/ff5878a1b2ee6a26669eab319d2bfde734628a21), [c6decc2](https://github.com/Dicklesworthstone/franken_markdown/commit/c6decc28892e962ad67dabaae50f2390df9c7e12)).
- **PDF nested figures and display math** fit the remaining width and the
  finished page geometry ([4607165](https://github.com/Dicklesworthstone/franken_markdown/commit/46071658970144cd0bd06d3864686a6b4abaf83d)).
- **PDF nested quotations keep room for text:** deeper quote borders may share
  positions to leave at least four body ems of text where the page allows, and
  quoted headings under list indentation stay inside their quote panels
  ([98b4ab4](https://github.com/Dicklesworthstone/franken_markdown/commit/98b4ab4c3dcb5a1f984e3d5159f8c5810d74fe24), [2d31dc4](https://github.com/Dicklesworthstone/franken_markdown/commit/2d31dc41c7a984d814eb2e9d5867fd35157849ad)).
- **PDF repeated table headers:** a continuation header repeats only when the
  page can also hold the first body row under it ([f24a81f](https://github.com/Dicklesworthstone/franken_markdown/commit/f24a81f34b25b5da0237f16fa1e910869b35f33c)).
- **PDF footnotes on a baseline grid:** page footnotes, separators and endnotes
  use the final baseline-grid metrics ([1b07ad4](https://github.com/Dicklesworthstone/franken_markdown/commit/1b07ad425c4113c857cd2daa2e964a4758d7cbd3)).
- **PDF headings stay with their text:** default pagination keeps a heading's
  wrapped lines and its H1/H2 underline on one page, and a heading followed by
  a complete figure may share the page. Page breaks move in affected documents
  ([b972ddb](https://github.com/Dicklesworthstone/franken_markdown/commit/b972ddb89c4fdc9dd54ddcba9de3825ba75f4c5e), [6120feb](https://github.com/Dicklesworthstone/franken_markdown/commit/6120feb47725a87bb9a77184e80116fd26190500)).
- **Raw-HTML table cells** keep nested rows, list markers, links, images and
  preformatted lines when lowered for PDF and SVG ([3b3fd49](https://github.com/Dicklesworthstone/franken_markdown/commit/3b3fd498f8a6d57e9e3eb3b99d151aa7035188e6)).
- **One table-of-contents owner:** HTML and PDF share marker discovery and
  automatic TOC insertion, so nested quotes, lists and diff fragments no longer
  inject duplicate outlines, and explicit nested markers fill the body outline
  ([b7683b9](https://github.com/Dicklesworthstone/franken_markdown/commit/b7683b9fc4646c0e03f58fb3040514294c6b1235)).
- **Footnote hard breaks:** two trailing spaces or a backslash at the end of a
  footnote line are kept as a hard break in the AST, HTML (`<br>`) and PDF
  ([607b233](https://github.com/Dicklesworthstone/franken_markdown/commit/607b233364bb24c8215403207356e7e2769fb07b)).
- **EPUB missing images and invalid characters:** unavailable images become
  text placeholders that keep their alt text, title, source and links.
  Characters XML 1.0 forbids are rejected in XHTML, navigation and package
  metadata with a diagnostic naming the member, before an archive is returned,
  and a failed CLI export keeps the existing file ([f40b7f7](https://github.com/Dicklesworthstone/franken_markdown/commit/f40b7f7b5624b75c3c91c2a3b6cdf7af5c457a9a)).
- **Flowchart self-loops and feedback edges** use separate return lanes outside
  node interiors and keep their arrow styles and labels in every direction;
  forward-only graphs keep their geometry ([50e47a4](https://github.com/Dicklesworthstone/franken_markdown/commit/50e47a47a5e07f257494070cfb8193117e5d2606)).
- **Editor and LSP source ranges:** editor and LSP source ranges now follow the
  semantic Markdown parser, keeping diagnostics and navigation aligned after
  frontmatter, lazy blockquotes, multiline mathematics, definition lists and
  removed reference definitions. Original UTF-8 byte offsets, CRLF line endings
  and tabs remain intact ([f79c6a4](https://github.com/Dicklesworthstone/franken_markdown/commit/f79c6a43ceeee1ef60911899770bc4545d9bda9f)).
- **Reference definitions:** a malformed destination (a second `<` inside angle
  brackets, an unbalanced parenthesis, an ASCII control character), a label
  over 999 characters or an unescaped `(` inside a parenthesized title no
  longer makes a definition, and the text stays visible. Definitions no longer
  merge the text around them, so ordered-list starts, list ownership and
  indented code are kept ([c8c5a77](https://github.com/Dicklesworthstone/franken_markdown/commit/c8c5a77f055583759ce1c999454ed3d78d974e0c), [b96ea73](https://github.com/Dicklesworthstone/franken_markdown/commit/b96ea73667a4b479a3874c47fb601d4f32093040), [709ebb7](https://github.com/Dicklesworthstone/franken_markdown/commit/709ebb7082fefa5a53fda886dcfd106006a94ab8)).
- **Link tails:** destination and title recognition is shared between bracket
  matching and AST building, so an accepted link cannot swallow later links or
  the brackets of an enclosing linked image ([ec144c5](https://github.com/Dicklesworthstone/franken_markdown/commit/ec144c525fe876bbd982d36bbb4b409c5ec4ceab)).
- **Container boundaries:** lazy list continuation stops at headings, code
  fences, HTML blocks and dedented unclosed fences; blockquote continuation
  follows the current leaf block; and reference pre-collection follows
  paragraph ownership after hanging indentation, so phantom definitions no
  longer shadow real links ([5d2830d](https://github.com/Dicklesworthstone/franken_markdown/commit/5d2830ddc95502ffc78ee48cbc5150e6008a013b), [7a3dae3](https://github.com/Dicklesworthstone/franken_markdown/commit/7a3dae3984de82b993ea2e3ac36535032c455e2e), [2b19865](https://github.com/Dicklesworthstone/franken_markdown/commit/2b19865155221941be63280a654ec1a0f9367ba7)).
- **Shared block admission:** table rows, display-math boundaries and raw HTML
  block tags are recognized the same way by parsing, reference collection and
  container continuation. GFM `\|` in code spans inside table cells is
  decoded, and reference-shaped lines inside formulas stay literal
  ([fcb0d82](https://github.com/Dicklesworthstone/franken_markdown/commit/fcb0d8207ba95959bffd3ddd442b0a9ccdd29727), [12b4c09](https://github.com/Dicklesworthstone/franken_markdown/commit/12b4c092130dacf5e3546ca88455c5968f869ef3), [67edda4](https://github.com/Dicklesworthstone/franken_markdown/commit/67edda4e22bb9098ac5bea901acab196094859de)).
- **Includes inside fences:** `expand_includes` no longer takes a list-owned
  closing code fence for a new top-level fence, which could hide a later
  include or expand literal include examples ([7d97686](https://github.com/Dicklesworthstone/franken_markdown/commit/7d97686f718dc50873dd220cb3cdb99192f71b39)).
- **Strikethrough:** `~~` uses the emphasis delimiter stack, so code spans, link
  destinations and titles, HTML attributes, math, escapes and entities stay
  opaque, nesting with emphasis works, and longer tilde runs stay literal
  ([e6f558b](https://github.com/Dicklesworthstone/franken_markdown/commit/e6f558b4851e03136a846149a13f657592178ec2), [41b0414](https://github.com/Dicklesworthstone/franken_markdown/commit/41b0414a18d64d1b3796660d732daeb344c5b3b5)).
- **Bounded inline work:** unmatched code spans, math, strikethrough, autolinks
  and inline HTML, link destinations and titles, bracket matching, and bare
  email and URL scans share the per-run inline work budget. When it runs out,
  the whole run stays literal source, including constructs that would
  otherwise parse, and speculative destination spans roll back. Repeated
  underscore boundaries and list-item collection no longer rescan earlier
  text ([f54f16a](https://github.com/Dicklesworthstone/franken_markdown/commit/f54f16aa8c0aca19ddd8fa97039862d8716e00c9), [8633f66](https://github.com/Dicklesworthstone/franken_markdown/commit/8633f6634fffa4377aed9d81a800cbb4528e435b), [ae0295c](https://github.com/Dicklesworthstone/franken_markdown/commit/ae0295c98675c77982c5ff4ce7cca32299fb140d)).
- **Offline interactive renderer, blocks:** the `--to interactive-html` preview
  keeps block ownership across lazy continuation, recognizes all seven
  CommonMark HTML block types, and ignores a leading BOM and valid frontmatter
  when admitting blocks ([7fd1680](https://github.com/Dicklesworthstone/franken_markdown/commit/7fd1680d6327e35a57f6dd4cf149ff64c88584fe), [df1e641](https://github.com/Dicklesworthstone/franken_markdown/commit/df1e641b7a62862e892d29c18332bfa11fd1b46f), [18987fc](https://github.com/Dicklesworthstone/franken_markdown/commit/18987fcbc81b5b129ec8e0ee365c3f5365d813ee)).
- **Offline interactive renderer, notes, tables and links:** footnote IDs
  compare exactly (`[^A]` and `[^a]` are different notes), indented note bodies
  after blank lines are kept, table cells honour code-span pipes, and code
  spans and the full link grammar prevent phantom links and notes
  ([f41d1b4](https://github.com/Dicklesworthstone/franken_markdown/commit/f41d1b44df446be4cd14b538502bf76321742e15), [6fe2f5a](https://github.com/Dicklesworthstone/franken_markdown/commit/6fe2f5a64442ec99d553d4f90810b386e112a0b3), [5934462](https://github.com/Dicklesworthstone/franken_markdown/commit/5934462776348accb4ca3ff2e1a64db467ccb7de)).
- **Offline interactive renderer, incomplete links:** inline link targets are
  scanned with bounded lookahead, so a 50,000-character incomplete link renders
  literally ([a2fca4b](https://github.com/Dicklesworthstone/franken_markdown/commit/a2fca4bca7021b32de575c6afa76cd11d4e19ef0)).
- **Offline interactive images:** percent-encoded, UTF-8 and charset-qualified
  `data:` images survive editing, printing, Save HTML and reopening, and a
  rejected image override stays rejected ([1409ede](https://github.com/Dicklesworthstone/franken_markdown/commit/1409ededcba7a4cd2162fb4981db4292d3411d2a)).
- **Offline interactive editing:** pending image imports are cancelled when
  composition starts, the editor becomes read-only or detached, or the page is
  left; Open Markdown and Undo source replacement stop when editing access
  ends ([62cd455](https://github.com/Dicklesworthstone/franken_markdown/commit/62cd45586deeb10a7a07bdb1ceb2e31c2050a70a), [2d095af](https://github.com/Dicklesworthstone/franken_markdown/commit/2d095af1d21d396c00756e11396e242f14c87f94)).
- **`fmd-math` script drops:** `fmd-math` reads rule 18a's baseline drops at the
  script size, as TeX does (tex.web §756). The drops come from cmsy7 and cmsy5,
  not from cmsy10 scaled to the base's size. A superscript on a non-character
  nucleus now sits 0.139 em higher in display and text style: a display
  integral's upper limit, a `\sum`'s superscript in text style, `{(a)}^2`. The
  subscript is unchanged there. In script and scriptscript styles both drops
  move to TeX's values. `MathConstants.sup_drop` and `sub_drop` are gone; see
  Breaking changes. Tracked as franken_manim fm-4ptw ([4db6f43](https://github.com/Dicklesworthstone/franken_markdown/commit/4db6f439f5ed5873fd310122a719c17d942d0b3d), [abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d)).
- **`fmd-math` `\cong`:** `fmd-math` sets `\cong` (≅) as LaTeX's `\@vereq`
  stack, cmsy's `∼` over cmr's `=`, instead of drawing the Noto Sans Math
  fallback glyph. The rows sit 0.5 pt closer than touching, and the stack is
  lowered 0.5 pt, in every style. The construction is two glyphs. Every
  standard relation is now drawn from Computer Modern (franken_manim fm-shqq)
  ([83d363c](https://github.com/Dicklesworthstone/franken_markdown/commit/83d363c5d4995f6e7c02349f1ec27a2f047e0bad)).
- **`fmd-math` small delimiters:** `fmd-math` takes the small variant of a
  `\left`/`\right` parenthesis or bracket from math family 0, as TeX does:
  cmr10 at display and text size, cmr7 and cmr5 in scripts. At display and
  text size it used CM Unicode's glyph. Tall delimiters were already cmex's own
  glyphs (franken_manim fm-u5sb) ([b8f687d](https://github.com/Dicklesworthstone/franken_markdown/commit/b8f687db403d8d102c6477440b61a2514a93a09c)).
- **`fmd-math` braces:** `fmd-math` fills `\overbrace` and `\underbrace` the way
  LaTeX's `\upbracefill` and `\downbracefill` do: cmex's four brace pieces in
  two pairs, each pair joined by a rule as high as the pieces' row, instead of
  a drawn band. The band was lighter than cmex's ink and curled differently.
  The drawn band remains only for rosters without cmex10 (franken_manim
  fm-u5sb) ([2f1e00c](https://github.com/Dicklesworthstone/franken_markdown/commit/2f1e00c08eb48e4084661be7dd255819747812fc)).
- **`fmd-math` font parameters:** every Appendix G parameter and the math unit
  `mu` are read from the symbol and extension fonts at the current style's size
  (cmsy10/7/5, cmex10/7) instead of hand-copied constants. `\quad`, `\qquad`
  and the control space are text-font glue, so `\quad` in a script is cmr10's
  quad at the declared size (10 pt at 10 pt) instead of 7 pt ([abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d)).
- **`fmd-math` nesting and unclosed arguments:** layout at the 64-level nesting
  bound no longer overflows a 2 MiB thread stack in unoptimized builds, and a
  chain of unclosed `\ln(` openers is parsed once instead of re-parsing the
  enclosing arguments at each level. A nesting-budget error inside a
  speculative argument is the named `Budget` error ([9bbb40a](https://github.com/Dicklesworthstone/franken_markdown/commit/9bbb40ad5472e729ae2fe79052b4b039b9e0f7c1), [db22126](https://github.com/Dicklesworthstone/franken_markdown/commit/db22126c3d54cf079a509ca4d97cdbd89aa1b507)).
- **`fmd-math` `\ex@` without `powf`:** amsgen's unit is computed with repeated
  0.97 steps in basic IEEE arithmetic, so layout at sizes other than 10 pt no
  longer depends on the platform math library ([8667819](https://github.com/Dicklesworthstone/franken_markdown/commit/8667819a0a5a344c75b6cded0f0c47bc7547aa53)).
- **`fmd-font` variable-font deltas:** packed `gvar` delta runs had their zero,
  byte and word flags reversed, distorting outlines or leaving glyphs at the
  default weight. Runs are bounded to their tuple data, and truncated or
  oversized runs are rejected ([23a63bb](https://github.com/Dicklesworthstone/franken_markdown/commit/23a63bb0816f93f50436880b0753fc71c79b7651)).
- **`fmd-font` coincident IUP points:** OpenType's rule for coincident
  reference points is applied (equal deltas propagate, unequal ones infer
  zero), and static TrueType glyphs are encoded with compact deltas and
  repeated flags ([884532c](https://github.com/Dicklesworthstone/franken_markdown/commit/884532cc17d9352534b34390794f4cc4e9a1b77e)).
- **`fmd-font` large cmap subsets:** a large BMP subset no longer overflows the
  format-4 cmap; segments are compacted, and a format-12 cmap is written when
  the table still does not fit ([86ee682](https://github.com/Dicklesworthstone/franken_markdown/commit/86ee6821dd1db3292e8465101bb251503aba560d)).

### Removed

- **`FMD_TAIL_NOGLYPH`, `FMD_TAIL_NOHEX` and `FMD_TAIL_NOPAIR`:** these
  environment variables were read once per process and changed the PDF
  (missing glyph IDs, a literal `<0000>` hex run, or zero kerning), so a
  process that inherited one wrote a different document from the same source.
  Shaping now always uses the face's glyph index, the mapped glyph's hex and
  the kern pair. `FMD_SUBSET_TAIL` and `FMD_WRAP_PERF` remain as diagnostic
  instrumentation ([c97d10e](https://github.com/Dicklesworthstone/franken_markdown/commit/c97d10e40769f98ee87db6753893f8b10dac7cee)).
- **`fmd-math` `MathConstants.sup_drop` and `sub_drop`** (see Breaking changes)
  ([4db6f43](https://github.com/Dicklesworthstone/franken_markdown/commit/4db6f439f5ed5873fd310122a719c17d942d0b3d), [abbb957](https://github.com/Dicklesworthstone/franken_markdown/commit/abbb957d1b48296c8bebede97526ede88e4a286d)).

## [0.6.0] - 2026-10-14

A feature release (139 commits since 0.5.0). It changes the public Rust
API, the `fmd-math` API and some default output, so it is a minor-version bump.
`fmd-font` 0.3.4 and `fmd-math` 0.2.0 ship with it.

### Security

- Default (safe) HTML now emits the raw-HTML subset GitHub's sanitizer allows,
  re-serialized as live markup. Only a few attributes survive, `href`/`src` pass
  the safe-URL policy, `<script>`, `<iframe>`, `<style>`, event handlers and
  other tags stay escaped text, and nesting is capped at 32 levels. Review this
  if your policy for untrusted Markdown is stricter than GitHub's (109537b,
  3b62050, fa5e904).
- SVG data-URI images check byte budgets before each write and reject a
  declared MIME type that does not match the data (a58201c, 2ce5fc6). EPUB book
  exports count all content against the 256 MiB ceiling, including books
  without embedded fonts (1ff975f).

### Breaking changes

- `fmd-math` 0.2.0: `Layout` has a new public `order: Vec<Placed>` field (TeX's
  emission order; new `Placed` enum) and `MathConstants` has new
  `display_integral_scale`, `script_space` and `jot` fields; struct literals
  must set them. `MacroSet::canonical_bytes` gains a record when ligatures are
  off (the default pack), so cache keys built from it change. Layout
  coordinates move as boxes follow TeX (see Fixed), and rule contours start
  at the bottom-right corner (9abff25, 68fe29b, 57da845, 66d6087, 3ed1d33,
  a81fa70).
- Public structs gained fields: `PdfRunningBand.image`, `FmdConfig.render` and
  five typography and font-format fields on `WasmRenderOptions`. Construct them
  with `..Default::default()` (f444bba, 22d501e, 21a18a8).
- The `wasm-bindgen` feature alone no longer exports `FmdFlowSession` or the
  book and interactive-workspace bindings. Enable `wasm-flow`, `wasm-book`,
  `wasm-workspace`, or `wasm-full` for all three, as the package builders do.
  Calls to a missing binding reject with `UNSUPPORTED_WASM_PACKAGE` (#16,
  a787b1a, f800a09, a3e996c; see docs/WASM_PROFILES.md).
- Default PDF and SVG bytes change: page-bottom footnotes with superscript
  marks instead of `[1]`, typeset inline math, drawn inline images, lowered raw
  HTML, Mermaid figures, and new PDF heading and link tags. A link to `#notes`
  resolves only when the host renders `doc.with_endnotes()`. Pin 0.5.0 if you
  depend on byte-identical PDFs (2a80cd8, fc973ad).

### Highlights

- **Book publishing:** atomic multi-format editions, retained workers, chapter
  previews, publishing profiles, outlines, scoped search, undo/redo and saving.
- **PDF:** page-bottom footnotes, inline math and images, Mermaid figures,
  emoji, header/footer logos, and paper sizes and margins.
- **Browser and editors:** syntax-highlighted flow sessions, separate WASM
  build profiles, and cross-buffer link navigation in `fmd-lsp`.
- **fmd-math 0.2.0:** box sizes and spacing much closer to TeX's.

### Added

- **Book editions:** `createBookPublicationWorker` publishes PDF, EPUB and HTML
  site from one captured input over one worker; later formats reuse that input
  without resending sources, fonts or images, and output is returned only when
  the whole edition succeeds (941042e, 98fccf3; see wasm/BOOK_PUBLICATION.md).
- **Retained book sessions:** opt-in `retainBook` workers keep one native book
  across formats, `renderSources` sends only changed chapter/include text, and
  `replaceSources` changes chapter membership and order in one transaction
  (0d81423, 9b9b826, 4a5a24f, 228d42d, 8ebb265, f6b833f). The reader renders
  one chapter at a time without building the site ZIP (24d96e3, 6410bcb,
  3ed8595, 36c5957).
- **Publishing profiles:** `FmdBook.setPdfOptions`/`setHtmlFontFormat` set
  typography, text sizes, TOC depth, page targets, code line numbers, metadata
  epoch and running headers/footers for browser books. Profiles survive
  workers, save in source and portable projects, and have a workbench panel
  (6069490, f8a3da6, a3ca221, b28ebac).
- **Book navigation and search:** HTML sites and previews show the current
  chapter's section outline, previous/next links and `aria-current="page"`;
  hosts can call `chapter_outline` (640874d). Offline search matches whole
  sections, treats composed and decomposed Unicode alike, takes `path:`/`title:`
  filters and `-word`/`-"phrase"` exclusions, and keeps bookmarkable query
  links with Back/Forward (7f1ddd0, ccea282, 6663914, 3740805).
- **Book workbench:** book-wide undo/redo across chapters and includes
  (Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z, Ctrl+Y), and Open, Save, Save as, Reload and
  Disconnect for source-project files; saving will not overwrite a file changed
  elsewhere without consent (96414b8, edf4c26, 3174623, d16708b). EPUB stores an
  image shared by several chapters once (c690d82).
- **Flow engine and browser editor:** opt-in syntax highlighting of fenced code
  in the flow display list and Canvas preview, kept across edits; workers take
  `initialCodeHighlighting` (84206bf, 0860ff1, 38f532e, 8140cce, aefeaf9,
  398fc54). Persistent sessions export SVG, and PDF with paper, margins and
  running bands; stateless PDF workers take running bands and logos (f23ad77,
  4642f47, 126a530). The raw WASM is 323 KB smaller with one reflow engine
  (42a3fe3) and uses wasm-bindgen 0.2.129 (df90e2b).
- **PDF footnotes and pagination:** notes sit at the foot of the page citing
  them, each superscript mark links to its note, and notes taller than 40% of a
  page or unreferenced stay in the trailing Notes section (fc973ad, cd71c04;
  see docs/PDF_FOOTNOTES.md). `--pdf-optimal-pagination` reserves note space
  and may pick a paragraph shape one line shorter or longer (e9d0084, ef89e0b).
- **PDF inline math and images:** `$…$` is typeset with fmd-math as an
  unbreakable `/Formula` box with its source as `/Alt`, and a new
  `math_overflow` warning names formulas wider than the measure (f059cac).
  Images in paragraphs, headings, lists and table cells are drawn instead of
  their alt text and tagged `/Figure` (b5b45af).
- **PDF running images:** `--pdf-header-image`/`--pdf-footer-image DEST` draw a
  PNG, JPEG or SVG logo from `--pdf-image DEST=PATH`, placed and sized with
  `--pdf-{header,footer}-image-position` and `-image-height-pt`; also in watch,
  batch, the browser APIs and MCP (f444bba).
- **Mermaid and emoji:** PDF and SVG draw flowchart and sequence fences as
  vector figures (101de1f, fb4ddca). HTML and EPUB turn `:rocket:` shortcodes
  into emoji (49ef49a, f4568a4). Native PDF and SVG draw ~360 common emoji with
  a curated Noto Emoji face, embedded only when used; `fmd-font` 0.3.4 exposes
  it as `bundled::NOTO_EMOJI_CURATED` behind a new `emoji-face` feature
  (ac5bc68; see docs/EMOJI_FALLBACK.md).
- **Raw HTML in paged output:** PDF, SVG and EPUB lower the GitHub subset to
  native nodes; digit-only `<sup>`/`<sub>` become Unicode digits (x², H₂O)
  (109537b, 06dbd52). CommonMark conformance rises to 589/652 (3b62050).
- **CLI:** `--page-size letter|a4|a5|legal|tabloid|WxH` and
  `--margin-{top,right,bottom,left}-pt` in render, watch, batch and book
  (c105648). Config keys set render defaults (`lang`, `toc`, `font_scale`,
  `microtype`, `typography_*`, `page_size`, ...); flags and frontmatter win
  (22d501e). `fmd batch --to epub|svg` writes one file per input (bfe2b98).
  WASM `renderPdf`/`renderHtml` and MCP accept the typography, optimal
  pagination, expansion and HTML font-format options (21a18a8).
- **LSP:** inline and reference links resolve and complete across open buffers
  and inside quotes, lists, tables and footnotes; `workspace/symbol` ranks
  headings from all open buffers (21722ed, 30b0b8b, 357ebed, 5aacee6).
- **fmd-math:** `\pmod`, `\tbinom` and `\dbinom`; text mode sets TeX's quote
  and dash ligatures unless the macro set disables them, as the default pack
  does; `MacroSet::ligatures()` reports which (973ba20, a8aab0d, 0a02812,
  3ed1d33).

### Fixed

- `install.sh` took the release body as the version on compact latest-release
  JSON; it now reads the top-level `tag_name` (#17, 2ed6a33). Piped installs
  work under macOS Bash 3.2 (856441a).
- `fmd watch` and `fmd batch` accept the same rendering flags as `fmd render`,
  and batch refuses `--pdf-a`, `--svg-width-pt` and `--to interactive-html`
  instead of ignoring them (5c54572, bfe2b98). `fmd --to svg` uses host images
  and fonts like PDF, prints warnings and gains `--svg-width-pt` (cf711db).
- `--profile commonmark-gfm` was a no-op; it now keeps definition lists as
  literal text, as GitHub does (0d26725). `fmd help book` lists the real flags
  (26edc68).
- Tagged PDF: headings get `/H1`-`/H6` from their source level, and links are
  `/Link` runs inside their paragraph instead of whole lines (3f70995, 90ce7d6).
- Unsupported Mermaid types (pie, gantt, ...) stay highlighted code instead of
  being read as flowcharts (101de1f).
- Parser: bare autolinks no longer swallow explicit or reference link labels,
  and nested scopes restore link source origins (148ec6c, db8473c).
- `fmd-math` follows TeX's measured boxes: italic corrections after lone
  characters (`f(`), `\scriptspace`, null-delimiter space by `\frac`, spacing
  across explicit glue, and text-mode spaces (7bf65e8, 4c8caa2, 57da845,
  656af88, a3578a0); cmex10 sizes and widths for display integrals, `\big` to
  `\Bigg`, delimiters and big operators, LaTeX struts in arrays and cases, and
  1.8 em brace fills (68fe29b, 2244415, e0dd66e, 29385b3, 80cbadc, bd00a24,
  f84a459); `\vec`, primes, ellipsis dots, `\vdots`/`\ddots` and `\root`
  degrees as TeX draws them, and tall drawn parentheses no longer break apart
  (3337884, 8798a86, 973ba20, d45a8a4, bfb3221, a8aab0d); multi-line `Tex`
  rows align at `&` like `align*`, open up by `\jot`, and a trailing `\\` adds
  no row (66d6087, 798de11, e4e8e2e).
- iOS app: document commands and async open/save results stay in their own
  window, and Command-O opens a document (80c95bf).

## [0.5.0] - 2026-10-02

A large feature release (413 commits since 0.4.5). It changes the public Rust
API and some default output, so it is a minor-version bump.
`fmd-font` 0.3.3 and `fmd-math` 0.1.2 ship with it.

### Security

- Highlighted code in the default (safe) HTML could contain live markup. This
  was present since 0.4.4 and is fixed here (details under Fixed). Upgrade if
  you render untrusted Markdown to HTML through the CLI, MCP server, WASM
  package, books or EPUB.
- The MCP server bounds untrusted requests (frame size, recursion and render
  budgets). Recursive inline parsing and nested-link retries are also bounded.

### Breaking changes

- `PdfOptions` has a new public `running` field (running header/footer
  bands). Construct it with `..PdfOptions::default()`. Several other public
  option and result structs gained fields for the same reason.
- `mcp::handle_tool_call` now returns a typed `ToolError` instead of an
  `(i32, String, &str)` tuple. The MCP module is split into `tools` and
  `transport` submodules; `read_frame`, `write_frame`, `handle_tool_call` and
  `tools_list_result` are still re-exported from `mcp`.
- The MCP server uses standard stdio framing. It answers in newline-delimited
  JSON by default, accepts `Content-Length` framing, and rejects unknown tool
  arguments with JSON-RPC error `-32602`.
- `fmd book` validates `book.toml` strictly. Unknown keys, chapters missing
  from an explicit `order`, and non-UTF-8 file names are errors. In 0.4.5
  they were ignored.
- Default PDF and SVG output bytes change: display math is typeset as vector
  geometry with the shared TeX engine, glyph expansion is off unless
  microtype is enabled, and pagination uses an exact mixed-height planner.
  Pin 0.4.5 if you depend on byte-identical PDFs.

### Highlights

- **Books and publishing:** `fmd book` renders multi-chapter books to PDF,
  EPUB and offline HTML sites. Chapter links resolve across chapters, each
  chapter keeps its own footnotes and assets, PDF chapter navigation and page
  counts are exact, and HTML sites include offline full-text search. EPUB
  export packages images, MathML/SVG, embedded font subsets and themes. ZIP64
  is supported for large exports.
- **Includes:** bounded line and named-snippet transclusion. Include
  expansion keeps original source locations and resource origins.
- **Browser/WASM:** reusable book sessions and cancellable worker exports,
  persistent flow-editor sessions with paged layout, a Canvas preview with
  source navigation, a semantic reader with Unicode search, document and
  flow workers with cancellation and stale-output fencing, and typed
  TypeScript entry points.
- **Interactive HTML workspace:** editable offline workspaces with native
  WASM rendering. They support image import/paste/drop, document settings,
  font authoring, source open/find/replace, background previews, cancellable
  PDF/HTML/EPUB/SVG exports, Document Lab reports, PDF proofs and recovery
  copies.
- **Layout and PDF:** an exact mixed-height pagination planner,
  Knuth-Plass breaking for SVG posters, typeset display equations, composed
  Latin accents (original Unicode kept as ActualText), rich footnote bodies,
  and opt-in running headers and footers (#13).
- **Engine:** a resumable flow-display engine with shaped reflow,
  table/code-fence flow, incremental dependency invalidation and source maps.
- **Language support:** incremental lexers with capability rows and
  adversarial split corpora for Rust, Go, Shell, C++, JavaScript, TypeScript,
  JSX and TSX. Streaming checkpoints are bounded and validated.
- **Editors:** an opt-in `fmd-lsp` language server (feature `lsp`) with
  diagnostics, outlines, folding, link/footnote checks, completion and
  definitions. `fmd watch` tracks image and include dependencies.
- **fmd-font 0.3.3:** owned text runs with caret and hit testing, a native
  shaping route contract, and an optional macOS CoreText adapter (additive).
- **fmd-math 0.1.2:** `\newcommand` optional defaults, bounded macro
  expansion, a text-mode `\\` that drops the space before it, a public `mbox`
  module, and an `Engine` that is `Send + Sync`.

### Added

- Opt-in PDF running header and footer (#13): left/center/right text slots per
  band with a closed token set (`{page}`, `{pages}`, `{title}`, `{author}`,
  `{date}` from `metadata_epoch_seconds`/`SOURCE_DATE_EPOCH`, never the clock),
  optional hairline rules, first-page skip, ellipsis on overflow, and a
  render error when a band does not fit its margin. Available as
  `PdfOptions.running`, `--pdf-header-*`/`--pdf-footer-*`/
  `--pdf-running-skip-first` flags, the MCP `running` object, and the WASM
  `renderPdf({running})` option (`renderPdfConfiguredRunning`). Default output
  is unchanged; `page_numbers` is now sugar for `footer.center = "{page}"`.
  Slot text uses the same CJK and symbol fallback faces as body text.
  Band images are not supported yet.

- Render supported decomposed Latin accents in native PDFs with the same glyphs
  and geometry as precomposed text. Preserve each occurrence's original Unicode
  through PDF ActualText, keep base/mark clusters together through paragraph,
  table, and code wrapping, and use the same font-aware composition for embedded
  SVG text. Missing-glyph diagnostics now inspect the face actually rendered.

### Fixed

- Security: highlighted code in the default (safe) HTML could contain live
  markup. The HTML/XML/SVG lexer reports a whole `<!...>` or `<?...?>`
  construct as one keyword token, and keyword, type, function and number
  tokens were written without escaping, so a fence such as
  `<?x><img src=x onerror=alert(1)>?>` produced an executing `<img>` in CLI,
  MCP, WASM, book and EPUB output (present since 0.4.4). Every highlighted
  token is now escaped; this also restores `<!DOCTYPE ...>`, `<?php ... ?>`
  and JSX/TSX entities (`&amp;`) as visible text.
- C++ raw-string highlighting no longer searches the rest of the input for
  `(` after each `R"` (quadratic on repeated `R"`); delimiters are at most
  16 bytes, so the search is bounded.
- Tag PDF page numbers as `/Pagination` artifacts. They were previously marked
  with an `/MCID` that no structure element owned, and the font subset carried
  all ten digits even when a document used fewer.
- Key shared PDF ASCII tables by the underlying immutable font objects so
  rendering different font families in one process cannot reuse stale glyph
  IDs or metrics and corrupt subsequent PDFs.
- Use the selected pagination policy consistently for PDF table-of-contents
  page numbers, page-budget fitting, and verification, including fitted font
  sizes and positions.
- Keep glyph expansion and contraction contiguous across styled PDF word
  fragments, including font changes, links, strikethrough, ligatures, and symbol
  fallback. Position text and decoration extents from the same rounded horizontal
  scale that the PDF paints.
- Pass each PDF block's justification policy into the paragraph optimizer, so
  ragged headings cannot exceed the print measure by borrowing space compression
  that is never drawn. Preserve justified body text when the layout workspace
  is reused across headings and paragraphs.
- Fix many bugs across the parser, footnotes, PDF lists and figures, SVG
  text and endnotes, `fmd watch`, the EPUB/book pipelines and the WASM worker
  lifecycle. See the git history for `fix(...)` commits since v0.4.5.

## [0.4.5] - 2026-09-15

- Add `fmd-font` 0.3.2 strict Result-based subsets with typed failures and
  explicit embedding formats; preserve legacy TrueType bytes and APIs (#11).
- Decode and deterministically subset name-keyed CFF1/Type 2 fonts, preserving
  cubic outlines and sparse glyph remaps. Check every Bravura 1.392 glyph
  against FontTools; unsupported charstrings receive typed refusals (#10).
- Add explicitly configured Latin/basic-Arabic shaping with advances, offsets,
  UTF-8 clusters, logical-text recovery, and bounded unsupported-feature errors.
  Verify joining, RTL, ligatures, and combining marks against HarfBuzz (#12).
- Keep all new font mechanics dependency-free and portable to WASM. Existing
  Markdown/PDF layout continues using its established rendering path.


## [0.4.4] - 2026-09-15

- Fix Go malformed hexadecimal escapes splitting UTF-8 characters (#9).
- Fix Python and shared lexer whitespace loops on form-feed/vertical-tab (#8).
- Make standalone tests self-contained, removing the sibling checkout requirement (#7).
- Fix shell word offsets, SQL Unicode span boundaries, HTML partial-tag buffering,
  JavaScript template boundaries, and TypeScript primitive highlighting.
- Include the incremental language lexers and capability APIs added since 0.4.3.
- Pin the WASM adapter to the documented wasm-bindgen CLI version.
- Restore formatting and strict Clippy checks across the new lexer/test modules.


## [0.4.3] - 2026-08-31

### Fixed

- **Multi-platform release restoration** (#5): releases again ship macOS
  (aarch64 + x86_64), Linux aarch64, and Windows x86_64 (MSVC, cross-built via
  cargo-xwin) alongside Linux x86_64. v0.4.0 shipped Linux-only and
  v0.4.1/v0.4.2 lacked Linux arm64 and Windows because the release tooling's
  per-host paths and target lists had drifted; the dsr configs are reconciled
  (repos.d as source of truth) and the mac build host path now points at the
  real checkout. Linux gnu builds keep the glibc-2.28 floor via cargo-zigbuild.

### Added

- **SOTA typography optimization (Verna DocEng '25 + Hàn Thế Thành pdfTeX):** (Verna DocEng '25 + Hàn Thế Thành pdfTeX):**
  - Added `--typography-homogeneous`: gradual adjacent demerits in the Knuth-Plass line breaker. Replaces the coarse 4-class binary fitness check with a linear penalty proportional to the fine-grained spacing-ratio difference between consecutive lines (`fitness_ratio_milli` + `line_demerits` gradual arm in `src/layout.rs`), producing more homogeneous inter-word spacing in justified paragraphs. Default off — byte-identical classic KP output.
  - Implemented the microtype expansion emitter: `--microtype expansion` renders the justifier's ±1.5‰ glyph-elasticity credit as true horizontal glyph scaling via the PDF `Tz` operator (uniform per line — exact, because the credit distribution in `glue_adjustments_into` is proportional to box width), instead of flat letter-spacing (`build_segs_adjusted` + `append_text_segment_operator_with_render_mode` in `src/pdf.rs`). `--microtype protrusion` now carries both effects.
  - Tests pin the contract: default renders byte-identical (no `Tz`), opt-in deterministic, every `Tz` factor within the ±1.5% budget, page counts unchanged (`tests/gradual_demerits_test.rs`, `tests/microtype_test.rs`).
  - Added `--typography-antiriver`: river-seed demerits — the KP inner loop penalizes break candidates whose previous line's last inter-word space aligns (within 1% of the measure, natural-width prefix sums) with a space in the candidate line, breaking up vertical whitespace channels before they form. `tests/river_penalty_test.rs` pins: off = classic byte-identical; on never increases detected seeds across a fixed corpus and measurably changes breaks; on deterministic. Works for justified and ragged text.
  - Added `--pdf-optimal-pagination`: Plass-style optimal pagination (Plass & Li, 1981) — a document-wide DP over page partitions minimizing the total of the same void-badness + keep-penalty costs the greedy breaker applies per page. Forced boundaries stay hard; the final page stays unpenalized; break penalties are precomputed per index so each edge is O(1). In-file `plass_pagination_tests` pin plan legality, total-cost dominance over greedy, and a crafted myopia fixture where the DP strictly wins by trading a little void on one page for keeping a captioned block whole on the next; `tests/optimal_pagination_test.rs` pins the public contract. SMAWK acceleration was evaluated and deliberately not implemented (cost terms violate total monotonicity by construction — see docs/MICROTYPOGRAPHY.md).

  - Table layout optimality proof: `table_alloc_optimality_tests` verify the min-plus column allocator (hard budget equality + per-column minimum constraints, height-sensitive wrap-badness objective — the practical core of Marx & Stuckey's constrained table layout) against exhaustive enumeration of every legal grid allocation with identical sub-unit finish handling.  - Added `--typography-pareto`: multi-objective line breaking (Holkner) — bounded per-candidate fronts of non-dominated states over structure and hyphenation demerit dimensions; final pick stays min-scalar so the scalar total can never exceed classic. Sweep over 280+ hyphenation-bearing fixtures pins dominance and determinism.

- **Multi-Chapter Book & Site Builder (`fmd book`, epic `7tus`, beads `qqst`, `qpqv`, `j0o4`):**
  - Added the `fmd book <dir> [--out-dir DIR] [--to html|pdf|both] [--json]` command to compile a directory of Markdown files into a unified multi-page HTML site and/or a single merged PDF book from one shared AST representation (`src/book.rs`, `src/cli.rs`).
  - Added frontmatter metadata parsing (`title=`, `author=`, `lang=`, `toc=`, `toc_depth=`) supporting key-value frontmatter fences (`---`) before block parsing (`src/parse/frontmatter.rs`).
  - Added the recursive transclusion engine (`{{#include relative/path.md}}`) with cycle detection, depth limit of 16, and canonical document root sandboxing to prevent directory traversal (`src/transclude.rs`).
  - Implemented automatic cross-file Markdown link rewriting (`other.md#anchor` → `other.html#anchor`) for site generation, preserving non-book links (`rewrite_links_for_site` in `src/book.rs`).
  - Implemented shared zero-JavaScript sidebar navigation injection (`<nav class="fmd-book-nav">`) with active chapter markers (`class="current"`), breadcrumb links, and inlined styles (`inject_book_nav` in `src/book.rs`).
  - Added deterministic `index.html` generation with meta refresh redirection pointing directly to the first chapter in the reading order.
  - Added multi-chapter PDF merge (`book_pdf_document`) inserting `Block::PageBreak` between chapter ASTs to guarantee chapter start on a fresh page while preserving continuous page numbering and generating a unified global outline tree.
  - Added machine-readable JSON receipt output on stdout (`--json`, schema `fmd-book-receipt-v1`) with chapter counts, per-file byte counts, page metrics, and unresolved link tallies.
  - Added optional manifest support (`book.toml`) for explicit chapter ordering (`order = [...]`) and book title overrides (`parse_book_manifest` in `src/cli.rs`).

- **Semantic AST Diff Engine (`fmd diff`, bead `64v4`):**
  - Added AST-level diffing (`src/diff.rs`) computing longest common subsequence (LCS) alignments across block and inline nodes.
  - Classifies AST changes into `Unchanged`, `Inserted`, `Deleted`, and `Modified` blocks, with granular word-level diffing and structural similarity ratio calculations.
  - Added multi-target diff renderers: side-by-side or unified HTML diff (`to_html`), ANSI color terminal output (`to_terminal`), Markdown report format (`to_markdown`), and machine-readable JSON schema (`to_json`).

- **Pure-Rust Zero-Dependency Mermaid Diagram Compiler (`src/diagrams.rs`, bead `y0vu`):**
  - Added a clean-room compiler transforming Mermaid `flowchart` and `sequenceDiagram` source fences directly into standalone SVG vector graphics for HTML and PDF rendering without external JavaScript or browser runtimes.
  - Flowcharts: supports layout directions (`TB`/`TD`, `BT`, `LR`, `RL`), node shapes (rectangle, rounded, circle, diamond, cylinder), custom styles, edge labels, link arrowheads, subgraphs, layer assignment, topological cycle breaking, and deterministic node coordinates.
  - Sequence diagrams: supports participants, actors, solid/dotted message lines with open/filled heads, inline notes (left of, right of, over), and activation boxes.

- **Standalone Vector SVG Poster Output (`--to svg`):**
  - Added the `--to svg` rendering target emitting standalone vector SVG posters with glyphs converted to vector paths, eliminating runtime font dependencies.

- **EPUB 3 E-Book Exporter (`src/epub.rs`, `--to epub`, bead `28t8`):**
  - Added single-file binary EPUB 3 export with a dependency-free pure-Rust OCF ZIP packaging engine, generating valid `mimetype`, `META-INF/container.xml`, `package.opf`, `nav.xhtml`, and XHTML content documents.

- **Deterministic JSON Search Index Generator (`--search-index <path>`, bead `r9z4`):**
  - Added search index generation exporting headings and anchored paragraphs into a structured JSON schema (`fmd-search-index-v1`) for static documentation search engines.

- **Document Intelligence, Readability Scoring, and Structural Linting (`src/doc_stats.rs`, `fmd stats`, bead `b3df`):**
  - Added document telemetry metrics: word counts, character counts (total and non-whitespace), reading time estimates, sentence counts, and syllable estimations.
  - Added automated readability formulas: Flesch Reading Ease, Flesch-Kincaid Grade Level, Coleman-Liau Index, and Automated Readability Index (ARI).
  - Added structural health linting: flags broken footnote references (`[^ref]` without definition), orphan footnote definitions, unresolvable local anchor links, and heading level hierarchy skips (e.g. H1 directly to H3).
  - Exposed via `fmd stats <input>` with human-readable CLI reporting and structured `--json` payload.

- **Accessibility Auditing and Verification (`src/verify.rs`, `fmd verify --a11y`, bead `jqls`):**
  - Added document accessibility auditing rules detecting missing image alt text, heading hierarchy skips, generic link labels ("click here", "link"), and headerless tables.

- **Optical-Margin Microtypography Protrusion (`--microtype protrusion`, bead `544o`):**
  - Added optical-margin protrusion for justified PDF body paragraphs, allowing punctuation marks (hyphens, periods, commas, quotes) to hang into the margin for cleaner visual alignment.

- **Self-Hosting Interactive Single-File HTML Workspace (`--interactive-html` / `--self-hosting`):**
  - Added single-file HTML workspace compilation bundling a live editor, real-time preview, and client-side PDF export into a self-contained zero-dependency HTML document.

- **PDF/UA-1 Tagged Structure Verification Gate (`scripts/check-pdf-ua.sh`, bead `8b3i`):**
  - Added automated CI verification checking emitted tagged PDF structure trees against veraPDF ISO 14289-1 profiles.

### Performance

- **30-Pass Extreme Software Optimization Loop (`skill-loop` passes 1..30):**
  - *Pass 1:* Branchless table-based line scanner and byte classifiers in `scanner.rs`.
  - *Pass 2:* Consolidated inline emphasis parsing from an intrusive doubly-linked list into a contiguous `EmphasisArena` to eliminate heap fragmentation.
  - *Pass 3:* Fast-path entity decoding for common HTML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`) with zero heap allocations.
  - *Pass 4–5:* Pre-sized HTML string buffers, early-exit clean escape paths, and direct tag chunk emission in `html.rs`.
  - *Pass 6–7:* Fast-path ASCII loops in syntax highlighter whitespace, numeric literal, and identifier scanning in `highlight.rs`.
  - *Pass 8–10:* Unrolled sfnt font table checksum calculations in 16-byte blocks and pre-sized font subset emission buffers in `fmd-font`.
  - *Pass 11:* Utilized `Cow<[u8]>` for uncompressed WOFF1 table payloads to eliminate redundant clones during font subset wrapping.
  - *Pass 12–14:* Unrolled Adler-32 checksum calculations in 16-byte chunks in `compress.rs`.
  - *Pass 15–17:* Optimized Liang hyphenation trie root lookups using a direct 128-entry ASCII lookup table in `layout.rs`.
  - *Pass 18–21:* Implemented contiguous slice serialization for decimal numbers and fractions in `pdf.rs` to avoid `format!` allocations in content streams.
  - *Pass 22–24:* Added fast-path clean XML string emission in `pdfa.rs` bypassing escape scanners on ASCII alphanumeric strings.
  - *Pass 25–27:* Pre-allocated CLI input buffers using file metadata length hints.
  - *Pass 28–29:* Implemented branchless static lookup tables for hex nibble decoding in the SVG color parser.
  - *Pass 30:* Finalized regression benchmarks and verified byte-for-byte golden output fidelity across all targets.

- **30-Pass Extreme Software Optimization Loop, Round 2 (profile-driven, passes 1..30):**
  Every pass profiled first (samply + the `fmd_perf_harness` scenarios), changed one lever, and proved byte-identical output via paired before/after binaries, differential tests, and archived optimization proofs under `tests/artifacts/perf/round2-*`.
  - *Pass 1–2:* Process-wide memoization of the per-render 128×128 ASCII kern/advance/ligature tables (pointer-keyed, host fonts build locally), plus building the kern matrix by enumerating GPOS pair coverage instead of probing all 16,384 cells (6× faster per face) — `pdf-showcase` p50 −52%.
  - *Pass 3–6:* LZ77 scratch generation-epoch scheme (fresh-table equivalence with zero-fill grow-only init, replacing the 256 KiB sweep), shared scratch across PDF streams/font programs/WOFF1/ZIP/HTML font embedding, u32 hash-chain indices with a ≥4 GiB usize fallback, and batched literal-run Adler-32 — `compress-corpus` p50 −22%, `pdf-large` −8%.
  - *Pass 7–8:* ToUnicode CMap emission into one presized buffer (stage p50 −62%) and zero-copy `EmbeddedFace` kern/ligature borrows (−278 KiB + 129 allocations per render).
  - *Pass 9–16:* Single-pass incremental run shaping in `build_segs_adjusted` (O(k²)→O(k), −88% on long-run corpora, f32 bit-equality differential tests), `shape_run` scratch reuse with an ASCII fast route (shaping stage −42%), inline small-capacity structure-tree paths, u32 link-target interning (`Tok` 64→40 B), single-hash shaped-run cache keyed by precomputed FNV digests with collision fallback, additive `subset_glyphs_with_lookup` dense-vec API, LayoutCx cache carry across the TOC fixpoint (TOC documents −26%), and append-style image/annotation writers (−7% on image-heavy serialization).
  - *Pass 17:* Compile-time first-byte bucket index for highlighter keyword tables (binary search measured slower and rejected with data) — −83% per lookup, keyword-dense HTML −11%.
  - *Pass 18–22:* Word-fold memoized `FontCharSet::extend_text`, safe-URL trimmed-range cache (−82% per repeated link), bounded MathML cache keyed `(tex, display)` (math-heavy HTML −88%), exact integer fixed-point SVG float formatting with half-even tie reproduction (18.3 M differential comparisons, `html-large` −14%), and lazy footnote-definition collection (walk −99.99% on footnote-free documents).
  - *Pass 23–28:* Two-hit admission for the inline parse cache, char-slice nested inline parsing without String round-trips, compact u32 bracket-pair tables, single-collect code spans (−15.6% on code-span corpora), presized fenced/indented code bodies (reallocs → 0), and byte-class Unicode-whitespace trim scanners (13.4 M differential comparisons).
  - *Pass 29–30:* u64/i64 fast paths in the layout-unit converters (105 M differential comparisons vs the 128-bit oracle) and per-(face, glyph) math-metrics memoization.
  - Net end-to-end vs the round-2 baseline (same-session paired measurements per pass): `pdf-showcase` p50 −54%, `html-code-heavy` −24%, `font-subset` −26%, `html-showcase` −20%, `html-large` −17%, `hyphen-corpus` −14%, `compress-corpus` −11%.
- **Round-2 Reserve Pass R1 — MathML writer presize + stack attrs (`skill-loop` reserve):** the fmd-math MathML serializer's output String is presized from the TeX source length via the new additive `to_mathml_with_capacity` (capacity = 160 + 8 × tex bytes; ratio measured over the repo fixtures + math-heavy corpus, 120/120 shapes covered), the per-styled-group `Vec<(&str, &str)>` `<mstyle>` attribute vec becomes a fixed 4-slot stack array + len, and the per-group `format!` percent String becomes an itoa-style stack digit write. Direct-lever micro: `to_mathml` over 38 golden/styled/single-glyph formulas × 2 displays −47.7% (attrs/percent levers alone −39.4%), byte-identity locked by a ~35k-factor percent differential test, a hint-sweep differential test, triple-cmp micro dumps, and all 10 golden artifacts identical between paired binaries.
- **Round-2 Reserve Pass R2 — EPUB bulk-copy conversion + SWAR XML escapers (`skill-loop` reserve):** `html_fragment_to_xhtml` (the EPUB chapter-body converter) drops its per-character `chars().next()`/`len_utf8` copy loop for bulk `push_str` runs between `<` boundaries (std memchr-backed `find`; multi-byte UTF-8 never needs per-char decoding in a pass-through run), and the per-char `escape_xml_text`/`escape_xml_attr` loops become scanner-driven bulk-copy over clean runs via the existing `find_html_text_escape` and a new 5-needle `find_xml_attr_escape` SWAR scanner (`& < > " '`). Byte-identity locked by differential tests against the verbatim per-char reference bodies over hostile corpora (every escapable char, chunk-edge boundary sweeps, multi-byte UTF-8, long clean runs, truncated/stray `<` openers), the naive-oracle scanner tests, the epub fixtures, and before/after binary golden equality (all deterministic artifacts byte-identical). Direct-lever micro over `render_epub` (repo README + CHANGELOG + 4 hostile corpora): −16.8% per window (run 2 under noisier ambient: −17.5%), 5/5 rounds both runs; per-input −10.4% (README) to −29.3% (long clean runs).

---
## [0.4.2] - 2026-08-28

### Added
- **Typographic Scale Presets (`--type-size` / `--font-scale`):**
  - Added `TypeScalePreset` (`compact`, `standard`, `readable`, `large`, `xlarge`, `subheading`, `display`) and uniform proportional typographic scaling (`FontScale`) across HTML and PDF (`src/theme.rs`, `src/cli.rs`).
  - Added `renderHtmlConfiguredAdvanced` ABI exposing native font scaling controls to WASM host environments and mobile bridges (`src/wasm_abi.rs`).

### Fixed
- **PDF/A-2b ISO 32000-1 EOL Stream Separation:**
  - Fixed a compliance violation where ToUnicode CMAP streams omitted the required ISO 32000 newline before `endstream`, causing strict readers to count the stream's trailing newline as the separator and report stream length as `actual + 1` (veraPDF ISO 19005-2 clause 6.1.7.1 failure).

---

## [0.4.1] - 2026-08-28

### Added
- **MathML Core HTML Output:**
  - Added native `<math>` and `<annotation encoding="application/x-tex">` rendering in self-contained HTML output (`src/html.rs`, bead `lqxy`).
- **Multi-Language Hyphenation:**
  - Added Liang hyphenation pattern tables for German (`de`), French (`fr`), Spanish (`es`), and Dutch (`nl`), including French apostrophe elision handling (`src/layout.rs`, bead `38re`).
- **GFM-Plus Definition Lists:**
  - Added `<dl>`, `<dt>`, and `<dd>` parsing and rendering in HTML and PDF layout (`src/parse/mod.rs`, `src/html.rs`, `src/pdf.rs`, bead `ryu4`).
- **Caret Diagnostics:**
  - Added line/column caret rendering for parse warnings and syntax errors (`src/caret.rs`, bead `9wse`).
- **Table of Contents & PDF Contents Page:**
  - Added HTML TOC generation and PDF contents page with dot leaders and linked page numbers using two-pass layout convergence (`src/html.rs`, `src/pdf.rs`, bead `byqs`).
- **PDF Document Outline Bookmarks:**
  - Added hierarchical PDF outline bookmarks derived from Markdown heading levels (`src/pdf.rs`).
- **WOFF1 Embedded Font Subsets in HTML:**
  - Added TrueType font subset packaging in deterministic DEFLATE containers for HTML, reducing document size by ~18.4% (`src/woff1.rs`, `src/html.rs`, bead `ge1t`).
- **Recursive Watch Mode (`fmd watch`):**
  - Added zero-dependency recursive directory file watching with built-in loopback HTTP preview server and SSE live-reloading (`src/watch.rs`, bead `xjld`).
- **PDF/A-2b Archival Profile:**
  - Added XMP metadata embedding, sRGB OutputIntent dictionaries, `--pdf-a 2b`, and `--pdf-a-strict` validation (`src/pdfa.rs`, `src/pdf.rs`, bead `q6xc`).
- **Streaming Chunked PDF Emission:**
  - Added chunked page emission pipeline with bounded thread pool compression and monolithic byte-for-byte parity (`src/pdf.rs`, bead `u9jt.2`).
- **Corpus Glyph Coverage Auditor (`fmd doctor fonts`):**
  - Added glyph coverage auditor inspecting Markdown documents against bundled fonts with structured JSON reporting (`src/cli/font_coverage.rs`, bead `y5i9.1`).
- **Universal Apple iOS and Mac Catalyst Application:**
  - Added native iOS/Mac Catalyst workspace (`ios/`, bead `ybfn`) with live-highlighting Markdown editor, TOC navigation, and export bridges.

---

## [0.4.0] - 2026-08-25

### Added
- **Clean-Room TeX Mathematics Layout Engine (`fmd-math`, `fm-j5t`):**
  - Added pure-Rust TeX math layout engine implementing TeX Appendix G algorithms: atom classification, style stepping, synthesized Computer Modern metrics, stretchy delimiters, matrix environments, and math-alphanumerics (`\mathcal`, `\mathbb`).
- **Factored Font Subsystem Crate (`fmd-font` v0.2.0):**
  - Factored out `fmd-font` workspace crate with clean-room sfnt table reader, glyf outline decoder, CFF decoder, and bundled OFL fonts.
- **UAX #14 CJK Line Breaking:**
  - Implemented Unicode Standard Annex #14 CJK line breaking: inter-ideograph breaking rules with opening, closing, and non-starter character prohibitions.
- **Configurable PDF Typography:**
  - Added `base_font_size`, `heading_scale`, and `table_font_size` layout overrides backed by a materialized `TypeScale` model.
- **Microtype Elasticity Symmetry & Void Budgeting:**
  - Enforced solver-emitter elasticity symmetry to eliminate ragged-tail overhangs.
  - Added inter-block gap flexing to reclaim space when page void exceeds 12% of content height.

---

## [0.3.5] - 2026-07-23

### Added
- **CJK Line Breaking & Knuth-Plass Glue:**
  - Added UAX #14 CJK line breaking support with zero-width stretchable glue integration for the Knuth-Plass optimal paragraph breaking solver.

---

## [0.3.4] - 2026-07-10

### Added
- **Remote Image Fetching & Lossless JPEG PDF Embedding (#2):**
  - Added CLI remote image fetching with direct `/DCTDecode` JPEG stream embedding for compact PDF rendering.
- **Bundled Noto Sans Math Fallback Face (#3):**
  - Bundled a curated ~56 KiB Noto Sans Math font subset to provide fallback glyphs for math symbols and arrows.
- **SVG CSS & Paint Cascade Engine:**
  - Implemented CSS declaration parsing, quoted values, `!important`, opacity/paint inheritance, and `hsl()`/`hwb()` color support.

---

## [0.3.3] - 2026-07-09

### Added
- **Vector SVG Drawing Operators:**
  - Added vector pattern strokes, stroked text paths, `textPath`, non-scaling strokes, and `color-mix()` alpha transparency preservation in the PDF SVG renderer.
- **Self-Contained Local SVG Embedding:**
  - Local SVG image references automatically embed as base64 data URIs in HTML preview documents.

---

## [0.3.2] - 2026-07-08

### Added
- **PDF Reading Quality Enhancements:**
  - Added vector task checkboxes, URL and long-token break rules, and TeX-correct shrink semantics (`line_badness`).
- **NPM Package Release:**
  - Published `@franken-suite/franken-markdown` npm package with automated release workflows.

---

## [0.3.1] - 2026-07-07

### Fixed
- Fixed HTML base64 encoder padding and PDF zero-length segment drawing bounds.

---

## [0.3.0] - 2026-07-07

### Added
- **Vector SVG PDF Drawing Engine:**
  - Implemented direct SVG-to-PDF vector conversion for paths, shapes, text, transforms, linear/radial gradients, masks, clips, markers, and CSS variables.
- **Mermaid & MMD Syntax Highlighting:**
  - Added syntax highlighting for Mermaid and MMD code blocks in HTML and PDF.
- **Constrained Table Column Allocator:**
  - Added min/max column width measurement and wrapping-badness optimization for PDF tables.

---

## [0.2.0] - 2026-07-03

### Added
- **crates.io Publishing & Safety Controls:**
  - Configured crates.io package release metadata with zero dependencies for the core library.
  - Implemented staged native file writes using temporary files and automatic rollback on failure.
  - Added strict zlib and PNG header validation.

---

## [0.1.0] - 2026-06-29

### Added
- **Initial Cross-Platform Release:**
  - Published release archives for Linux (`x86_64-unknown-linux-gnu`), macOS Intel (`x86_64-apple-darwin`), macOS Apple Silicon (`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`).
  - Added automated installer scripts (`install.sh`, `install.ps1`) with SHA-256 checksum verification.
  - Integrated Asupersync for structured concurrency in native batch rendering.

---

## [0.0.1] - 2026-06-26

### Added
- **Foundation & Clean-Room Core Engine:**
  - Clean-room zero-dependency Markdown parser and HTML emitter.
  - Agent-first `fmd` CLI with `capabilities --json`, `doctor --json`, `robot-docs guide`, and stable exit codes.
  - Shared typed theme model driving coherent HTML and PDF formatting.
  - TrueType font reader, subsetter, GPOS kerning, and GSUB ligature engine.
  - Knuth-Plass optimal paragraph breaking and Liang/TeX hyphenation.
  - Deterministic PDF 1.7 writer with embedded subset fonts, measured tables, and accessible tagged-PDF logical trees.
  - WASM compilation target with byte-for-byte output parity.
  - CommonMark 0.31.2 conformance test harness and verification test suite.

[Unreleased]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.7.0...HEAD
[0.7.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.5...v0.5.0
[0.4.5]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.4...v0.4.5
[0.4.4]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.3...v0.4.4
[0.4.3]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.5...v0.4.0
[0.3.5]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Dicklesworthstone/franken_markdown/compare/8b66477...v0.1.0
