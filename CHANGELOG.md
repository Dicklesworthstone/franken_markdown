# Changelog

All notable changes to `franken_markdown` and the `fmd` CLI will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added

- `fmd-math` honours `\color` and `\textcolor`. They used to parse and then be
  silently ignored, and `\textcolor` failed as untiered.
  - The colour surface is plain xcolor's: the 19 base names, mixes such as
    `red!30!blue` and `blue!70`, a leading `-` for the complement, and the
    `[rgb]`, `[RGB]`, `[HTML]` and `[gray]` models. Mixes are linear in RGB,
    checked against xcolor v3.02. Anything else is a precise parse error
    naming the supported set.
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

### Changed

- `fmd-math` sets mathematics in the faces TeX uses. `fmd-font` bundles the AMS
  Type 1 Computer Modern math fonts, converted to TrueType and renamed (cmmi and
  cmsy at 10, 7 and 5 pt, cmex10, cmr7 and cmr5; SIL OFL; see
  `fmd-font/fonts/cm-math/`).
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
- `fmd-math` sets math-mode family 0 (digits, `+ = ( ) [ ]`, upright Greek,
  operator names, `\mathrm`, and the accents) from cmr10 itself at display and
  text size. `fmd-font` bundles it like cmr7 and cmr5 (`FmdCmRoman10.ttf`), and
  the other nine converted faces reproduce byte for byte.
  - CM Unicode's Regular had stood in for cmr10, but its glyphs are not
    cmr10's: its `+` and `=` carry 0.57 of cmr10's ink (bars 0.023 em against
    TeX's 0.4 pt rule) and its brackets 1.48. franken_manim's ink oracle
    measured this in every formula that uses them (franken_manim fm-8zy2).
  - Text mode, including TexText runs, keeps CM Unicode.
  - Boxes, kerns and positions are unchanged, since both faces read cmr10's TFM.
    Eleven layout goldens moved: the faces and outlines of those glyphs only,
    and the accents, which now draw cmr10's own accent at scale 1.
- `fmd-math` lays out those faces from TeX's own font metrics (`fmd-math/src/tfm.rs`,
  transcribed from Knuth's cmmi, cmsy, cmex and cmr TFM files at 10, 7 and 5 pt
  by `scripts/font-oracles/extract-cm-tfm.py`). In math mode a glyph of a TeX
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
  Reference preamble. 17 of the 19 layout goldens moved.
- `fmd-math`'s clean boxes keep a lone character's italic correction in their
  width, as real TeX's `\showbox` shows. tex.web §721 frees the kern node but
  not the width its hpack gave the box. This covers fraction parts, scripts,
  limits, radicands, over- and underlined bodies, and accent bases:
  `\frac{f}{2}`'s numerator and `\tilde f`'s base are f plus δ wide, and a
  limit centres δ-wide. Three related corrections land with it:
  - An accent's box is its TFM character's width plus italic correction
    (char_box, §709) at the size of the style it is set in, with that
    character's height. `\vec` sits 0.077 em further left.
  - A radical's box includes the overbar's kern of one rule thickness above
    the rule (§705, §737).
  - Each change is checked against TeX's own `\showbox` boxes.
- `fmd-math` gains an `svg_dump` example for visual checks of layout changes.

### Fixed

- Editor and LSP source ranges now follow the semantic Markdown parser, keeping
  diagnostics and navigation aligned after frontmatter, lazy blockquotes,
  multiline mathematics, definition lists and removed reference definitions.
  Original UTF-8 byte offsets, CRLF line endings and tabs remain intact.
- Native render and watch outputs preserve every loaded input. SVG files and
  search-index sidecars now share the source-overwrite guard with HTML, PDF and
  EPUB; included Markdown, CSS, configuration, fonts and local images are also
  protected, including symlink and hard-link aliases. A collision fails before
  rendering or writing any publication output.
- `fmd-math` reads rule 18a's baseline drops at the script size, as TeX does
  (tex.web §756). The drops come from cmsy7 and cmsy5, not from cmsy10 scaled
  to the base's size. A superscript on a non-character nucleus now sits
  0.139 em higher in display and text style: a display integral's upper limit,
  a `\sum`'s superscript in text style, `{(a)}^2`. The subscript is
  unchanged there. In script and scriptscript styles both drops move to TeX's
  values. `MathConstants` replaces `sup_drop`/`sub_drop` with the cmsy7 and
  cmsy5 values and `script_drops`. Checked against TeX's `\showbox`
  (franken_manim fm-4ptw).
- `fmd-math` sets `\cong` (≅) as LaTeX's `\@vereq` stack, cmsy's `∼` over
  cmr's `=`, instead of drawing the Noto Sans Math fallback glyph. The rows
  sit 0.5 pt closer than touching, and the stack is lowered 0.5 pt, in every
  style. The construction is two glyphs, matching TeX's boxes to 1e-5 em in
  text, script and scriptscript style. Every standard relation is now drawn
  from Computer Modern (franken_manim fm-shqq).
- `fmd-math` takes the small variant of a `\left`/`\right` parenthesis or
  bracket from math family 0, as TeX does: cmr10 at display and text size,
  cmr7 and cmr5 in scripts. At display and text size it used CM Unicode's
  glyph, whose brackets carry 1.48 times cmr10's ink. Tall delimiters were
  already cmex's own glyphs. Checked against TeX's `\showbox`
  (franken_manim fm-u5sb).
- `fmd-math` fills `\overbrace` and `\underbrace` the way LaTeX's
  `\upbracefill` and `\downbracefill` do: cmex's four brace pieces in two
  pairs, each pair joined by a rule as high as the pieces' row, instead of a
  drawn band. The band was lighter than cmex's ink and curled differently.
  The boxes, rules and emission order match TeX's `\showbox`. The drawn band
  remains only for rosters without cmex10 (franken_manim fm-u5sb).

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

[Unreleased]: https://github.com/Dicklesworthstone/franken_markdown/compare/v0.6.0...HEAD
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
