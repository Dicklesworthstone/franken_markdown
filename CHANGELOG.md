# Changelog

## Scope and methodology

This changelog reconstructs the project's history from the git log, the
repository files, the checked-in beads tracker (`.beads/`), and the docs under
`docs/`. It is organized by landed capability rather than raw commit order, with
a date-based timeline kept visible so chronology is never lost. Representative
commits are linked directly.

This changelog began as reconstructed pre-release development history and now
records shipped binary and crate releases. The GitHub release ships standalone
`fmd` CLI archives, the `franken_markdown` library is published to crates.io
starting with `0.2.0`, and the WASM/npm package is assembled by the separate
tag-gated workflow. Conformance and status numbers below are the measured,
ratcheted floors enforced in CI, not aspirational targets.

- Sources: `git log --reverse --no-merges` (2026-06-26 to 2026-07-10), the
  working tree, `.beads/issues.jsonl`, `docs/`, and the CI
  workflows under `.github/workflows/`.
- Version state: **`0.3.5` CJK (UAX #14) line-breaking patch.**
- Commit links use the form
  `https://github.com/Dicklesworthstone/franken_markdown/commit/<hash>`.

## Version timeline

| Date | Phase | Headline |
|---|---|---|
| 2026-06-26 | Scaffold + first capabilities | Zero-dependency Markdown to HTML engine, the `fmd` CLI, governance, the structured theme model, syntax highlighting, the clean-room font reader, and the deterministic PDF MVP all land in one day |
| 2026-06-27 | Typography deepens | Font subsetting and embedding, GPOS kerning, GSUB ligatures, Knuth-Plass breaking, hyphenation, measured tables, styled inline runs, FlateDecode compression, and a large parser-correctness fix wave |
| 2026-06-28 | Hardening + WASM assets | Browser/WASM package assets, justified hyphen breaks, list looseness/tightness fixes, theme-color unification across HTML and PDF, and CI claim-discipline + keep-with-next pagination |
| 2026-06-29 | Proof gates + accessibility + batch | Real WASM proof gate with native parity, CommonMark 0.31.2 conformance harness, hierarchical accessible tagged-PDF, deterministic render-tree golden, the performance-proof track, and the native Asupersync batch contract |
| 2026-06-30 | First binary release | `0.1.0` release prep fixes installer asset lookup, switches the optional Asupersync dependency to the published crate, and cuts GitHub release archives instead of forcing source builds |
| 2026-07-03 | Crates.io + hardening release | `0.2.0` enables the crates.io package, trims package contents, hardens staged native writes, validates zlib/PNG payloads more strictly, and tightens public JSON escaping |
| 2026-07-07 | SVG/PDF fidelity + speed release | `0.3.0` expands vector SVG PDF drawing, Mermaid/MMD highlighting, local PDF assets, safer staged writes, optional batch receipts, and a measured optimization wave across parser, HTML, layout, PDF, highlighting, and compression |
| 2026-07-07 | DSR patch release | `0.3.1` is the DSR-built publication tag for the same renderer wave, with the late HTML base64 and PDF empty-segment drawing passes included and the rejected PDF decimal-string trial left out of the shipped source |
| 2026-07-08 | PDF reading-quality release | `0.3.2` ships vector task-list checkboxes, long-token wrapping, TeX-correct shrink semantics, npm package publication, and more SVG text fidelity |
| 2026-07-09 | DSR all-platform patch | `0.3.3` ships the post-`0.3.2` SVG/PDF and HTML asset fidelity wave, measured parser/HTML/PDF speedups, coverage expansion, color-mix transparency correctness, and DSR archives for Linux, macOS Intel, macOS Apple Silicon, and Windows |
| 2026-07-10 | Issue-driven PDF fidelity patch | `0.3.4` closes the first two user-filed issues: hotlinked images render in PDF via CLI-side remote fetching plus native JPEG `/DCTDecode` embedding, and common math/arrow glyphs draw through a bundled Noto Sans Math symbol fallback face instead of .notdef boxes; also an SVG CSS/opacity/paint structural-parsing wave, `hsl()`/`hwb()` colors, and measured parser/HTML/PDF/compression passes |
| 2026-07-23 | CJK line breaking | `0.3.5` gives Chinese/Japanese/Korean text real break opportunities: UAX #14 inter-ideograph breaks with the closing/opening/non-starter and Hangul-cluster prohibitions, carried as zero-width stretchable glue so the Knuth-Plass optimizer fills the measure instead of overrunning it in narrow columns; Latin output is byte-identical and the break-point splitter is no longer quadratic |

## 0.3.5 - 2026-07-23

CJK line breaking. Chinese, Japanese, and Korean text is written without
interword spaces, so the whitespace-driven paragraph builder found *no* break
opportunity inside a run of ideographs and handed the optimizer one unbreakable
box. What kept such a paragraph on the page at all was the generic long-token
machinery meant for bare URLs: an emergency break every 14 characters, at a
2000 penalty. That is coarse enough to leave a third of a narrow measure empty,
it ignores every CJK punctuation rule, and in any column narrower than one
14-character chunk — a nested list or blockquote, a multi-column table cell, a
small page — it produced no usable break at all and the line ran past the right
margin
([#4](https://github.com/Dicklesworthstone/franken_markdown/issues/4)).

Line breaking is now guided by UAX #14 for the classes CJK typesetting actually
needs. A break is allowed between adjacent ideographs, kana, and Hangul
syllables, and at a CJK ↔ Latin script boundary. It is forbidden before a
closing bracket, sentence punctuation, or a non-starter (`）】、。，！？；：」』`,
small kana, `々`, `ー`), forbidden after an opening bracket (`（【「『`),
forbidden before a combining mark, and forbidden inside a conjoining Hangul
jamo cluster (LB26). ASCII closing/opening punctuation carries the same rule
when it sits next to CJK, so `中文,` never orphans the comma.

Each permitted break becomes zero-width, slightly stretchable glue — the
`\CJKglue` model — rather than a penalty. Zero natural width keeps the
character grid intact, and the stretch gives the Knuth-Plass optimizer the
budget it needs to *choose* a CJK line instead of declaring every non-exact
line infeasible and falling back to greedy first-fit. The glue never takes a
share of the justification (there is no space token on the page to widen), so a
justified CJK line ends up to one character short of the measure instead of
opening a gap no glyph would fill. Table cells hard-split over-wide runs with
the same prohibition table, so a closing `。` is never orphaned at the head of a
cell line.

Non-CJK text is untouched by construction: a break is only ever added when one
of the two characters around it belongs to a CJK script or to CJK punctuation.
A 176 KB Latin document renders byte-for-byte identically before and after the
change.

Measured effect on the same documents: a body paragraph in a 20-deep blockquote
went from 14 to 20 characters per line (67% → 96% of the measure), a heading
from 28 to 40 (68% → 97%), and the narrow-measure overflow is gone. The word
splitter that feeds break points was rewritten as a single forward pass, since
rescanning the word per break point is quadratic once nearly every character is
a break opportunity: a 76,800-character single-paragraph Chinese document went
from 205 s to 2.3 s (debug build) with byte-identical output.

New public API in `franken_markdown::layout`: `cjk_break_allowed`,
`cjk_break_prohibited`, `is_cjk_char`, and `cjk_break_glue`. No third-party
dependency was added — the classification is a small explicit range table over
`char`, in the same style as `parse/unicode_punct.rs`.

Verification: `cargo fmt --check`, `cargo check --all-targets`,
`cargo clippy --all-targets -- -D warnings`, and the full test suite green,
including 15 new cases in `tests/cjk_line_break_test.rs` that assert on laid-out
line geometry read back out of the PDF content stream (a splitter unit test can
pass while the real layout path still overflows) plus recorded pre-change
baselines for Latin wrapping.

## 0.3.4 - 2026-07-10

Issue-driven PDF fidelity patch closing the repository's first two user-filed
issues while preserving the clean-room, network-free core contract.

Hotlinked images now render in PDF output
([#2](https://github.com/Dicklesworthstone/franken_markdown/issues/2)). The
CLI downloads remote http(s) image destinations for PDF renders before
invoking the renderer, via the system `curl` (preferred) or `wget`, with an
HTTP(S)-only protocol allowlist across redirects, a per-image timeout
(`--remote-image-timeout-secs`, default 20 s), the existing
`--max-pdf-image-bytes` cap enforced on the received body, and an opt-out
(`--no-remote-images`). Every fetch failure is non-fatal: a structured warning
is reported and the destination falls back to alt text, so offline renders
keep working. Fetched bytes enter the render core as ordinary caller-supplied
assets — the engine itself still performs no I/O. The PDF writer gains JPEG
support: baseline/extended/progressive Huffman JPEGs embed losslessly as
`/DCTDecode` XObjects, while lossless/arithmetic/hierarchical flavors and
4-component Adobe CMYK fail closed to alt text. Local `.jpg`/`.jpeg` files
auto-load next to the Markdown file exactly like PNG/SVG, and the HTML
renderer embeds supplied JPEG assets as data URIs
([`5b1e6cc`](https://github.com/Dicklesworthstone/franken_markdown/commit/5b1e6cc)).

Common math and arrow glyphs no longer render as .notdef boxes
([#3](https://github.com/Dicklesworthstone/franken_markdown/issues/3)). A
curated ~56 KiB subset of Noto Sans Math (SIL OFL 1.1) is bundled as a sixth
font slot covering arrows, mathematical operators, letterlike symbols, misc
technical, geometric markers, and long arrows, regenerated reproducibly with
the project's own clean-room subsetter. Text runs split by glyph coverage
before width measurement so line breaking, justification, table allocation,
and code fitting agree on the fallback face's real advances; the face is
embedded only when a run actually uses it, so ASCII-only documents keep
byte-identical output, and fallback glyphs stay selectable through ToUnicode
CMap entries
([`e63e463`](https://github.com/Dicklesworthstone/franken_markdown/commit/e63e463)).

The patch also carries an SVG fidelity wave — structural SVG CSS parsing
(declaration splitting, quoted value delimiters, `!important` markers,
top-level separators, trailing `var()` tokens), the SVG opacity
`initial`/`unset`/`inherit` cascade, inherited and `initial` paint keywords,
paint alpha composed with opacity properties, alpha preserved on missing paint
fallbacks, gradient stop/mask/currentColor/filter-shadow alpha, `hsl()` and
`hwb()` colors, absolute length units, fail-closed empty clip paths, and
protocol-relative SVG stylesheet import stripping in HTML — plus measured
parser/HTML/PDF/compression optimization passes with rejected trials recorded
in the performance artifacts, and a Windows-only CLI contract-test assertion
fix that compares JSON-escaped path separators.

Release verification for the source tree included `cargo fmt --check`,
`cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`, the
clean-room policy and WASM core gates, and the full test suite (1706 tests),
with the issue repros rasterized and inspected. New end-to-end coverage
exercises the real binary against a loopback HTTP server
(`tests/remote_image_test.rs`) and the symbol fallback chain
(`tests/symbol_fallback_test.rs`).

## 0.3.3 - 2026-07-09

All-platform DSR patch release for the renderer work that landed after
`0.3.2`. The release preserves the clean-room, std-only core contract while
expanding SVG/PDF fidelity: local SVG assets are embedded in self-contained
HTML, PDF SVG drawing now covers native pattern strokes, text strokes,
`textPath`, coordinate-list text placement, vector-effect non-scaling stroke on
text, CSS-variable URL resources, chained drop shadows, object-bounding-box
patterns, currentColor gradient stops, pattern viewBox transforms, nested SVG
data URIs, mixed-case SVG roots, and `color-mix(..., transparent)` alpha
preservation instead of falling back to opaque black.

The patch also carries a measured optimization wave across the parser, HTML
emitter, PDF writer/layout path, font cache, compression path, and SVG
operators. Rejected micro-trials are recorded in the performance artifacts
rather than represented as wins, and the current optimization ledger ranks
recommendations by total stage cost.

Testing and release confidence improved with broad PDF/SVG branch coverage,
font/subsetter edge tests, compression and staged-write tests, CLI and batch
error-contract tests, source-shape artifact-safety tests, and a repository
corpus soak that renders real Markdown to deterministic HTML and PDF.
The generated WASM module remains byte-identical to native output over the
package corpus; the raw `.wasm` budget is consciously raised from 3,300,000 to
3,400,000 bytes for the expanded vector-SVG/PDF surface, while the gzip budget
stays at 1,600,000 bytes.

Representative commits include local SVG HTML embedding
[`b863967`](https://github.com/Dicklesworthstone/franken_markdown/commit/b863967),
expanded SVG rendering and page stream reuse
[`18520f9`](https://github.com/Dicklesworthstone/franken_markdown/commit/18520f9),
native stroked SVG text
[`288c796`](https://github.com/Dicklesworthstone/franken_markdown/commit/288c796),
non-scaling SVG text stroke
[`2465bf0`](https://github.com/Dicklesworthstone/franken_markdown/commit/2465bf0),
straight SVG pattern strokes
[`9403319`](https://github.com/Dicklesworthstone/franken_markdown/commit/9403319),
drop-shadow panic prevention
[`f21485a`](https://github.com/Dicklesworthstone/franken_markdown/commit/f21485a),
configured batch image-byte limits
[`d9d2546`](https://github.com/Dicklesworthstone/franken_markdown/commit/d9d2546),
parser/HTML hot-path tightening
[`b284095`](https://github.com/Dicklesworthstone/franken_markdown/commit/b284095),
and the latest HTML/parser performance passes
[`05359d6`](https://github.com/Dicklesworthstone/franken_markdown/commit/05359d6)
and [`dfe840f`](https://github.com/Dicklesworthstone/franken_markdown/commit/dfe840f),
plus SVG `color-mix()` alpha preservation
[`7aca35e`](https://github.com/Dicklesworthstone/franken_markdown/commit/7aca35e).

Release verification for the source tree included `cargo fmt --check`, `cargo
check --all-targets`, `cargo clippy --all-targets -- -D warnings`, the
`pdf_svg_color_mix_with_transparent_preserves_paint_alpha` regression test,
WASM package generation with native/WASM byte parity, the full test suite with
the long corpus soak isolated, and DSR build/release verification for the four
configured platform targets.

## 0.3.2 - 2026-07-08

PDF reading-quality release. Task-list markers now draw as vector checkboxes
(rounded accent-filled box with a white check when done, neutral rounded
outline when open) while the `[x]`/`[ ]` text stays selectable via invisible
render mode; URLs, underscored identifiers, and other non-hyphenatable tokens
gain separator and emergency break points so they wrap (with per-line link
annotations) instead of running off the page, in body text and table cells
alike; and `line_badness` now treats shrinking past the shrink budget as
infeasible per TeX semantics, ending crushed-interword-space justification.
Also carries the HTML embedded-font `OS/2` fix (Chromium's sanitizer accepts
the subsets instead of silently falling back to system fonts), the published
`@franken-suite/franken-markdown` npm package with an idempotent release
workflow, SVG text fidelity work (`baseline-shift`, word spacing, explicit
whitespace, aria-label alt text, separated code panels), and thirty-plus
behavior-preserving performance passes across the PDF writer, compressor,
parser, and HTML emitter.

## 0.3.1 - 2026-07-07

Patch release for the DSR-built publication path. It preserves the `0.3.0`
renderer feature set, includes the late HTML base64 encoder and PDF
empty-segment drawing passes, records but does not ship the rejected PDF
decimal-string trial, and keeps the release artifacts aligned to the tag and
manifest generated by DSR rather than the canceled GitHub Actions binary
workflow.

## 0.3.0 - 2026-07-07

This release turns the post-`0.2.0` renderer work into a tagged CLI/library
ship. The headline is broader PDF fidelity without taking a browser dependency:
frankenmermaid-generated SVG diagrams are drawn as vector PDF operators, with
coverage for paths, shapes, text, transforms, gradients, spread modes, patterns,
masks, clips, marker view boxes/orientation/units, marker-child `paint-order`,
object-bounding-box clip/mask units, opacity, drop shadows, CSS
variables/selectors, `use`/symbol reuse, embedded PNG data URIs, modern color
tokens, text decorations, and current showcase output.

Markdown authoring got more useful for technical docs. HTML and PDF now share
Mermaid/MMD source highlighting, PDF tables use measured column allocation, code
blocks can include muted line numbers, ASCII diagram fences keep their geometry,
and file-input PDF renders can auto-load relative local PNG/SVG destinations
while hosts can still pass explicit image bytes through the CLI or library API.
Native writes are staged where applicable, `--to both` rolls back sibling output
on later failure, and the CLI refuses to overwrite the input file.

The release also carries a large behavior-preserving speed pass. Parser,
highlighter, HTML emitter, PDF layout/writing, font subsetting, SVG drawing,
zlib/DEFLATE compression, and optional Asupersync batch orchestration were
profiled and tightened in small commits, with rejected trials recorded where
they did not produce a real win. Representative commits include local PDF
assets and safer writes
[`91afecc`](https://github.com/Dicklesworthstone/franken_markdown/commit/91afecc),
expanded SVG/table/typography rendering
[`5423d18`](https://github.com/Dicklesworthstone/franken_markdown/commit/5423d18),
Mermaid fence highlighting
[`791a3c8`](https://github.com/Dicklesworthstone/franken_markdown/commit/791a3c8),
SVG text decorations
[`83d6663`](https://github.com/Dicklesworthstone/franken_markdown/commit/83d6663),
symbol/use viewport scaling
[`be813af`](https://github.com/Dicklesworthstone/franken_markdown/commit/be813af),
SVG color-token parsing
[`d469f67`](https://github.com/Dicklesworthstone/franken_markdown/commit/d469f67),
checked-in frankenmermaid SVG rendering
[`af97a82`](https://github.com/Dicklesworthstone/franken_markdown/commit/af97a82),
and direct compression table indexing
[`b6ddca1`](https://github.com/Dicklesworthstone/franken_markdown/commit/b6ddca1).

WASM remains a real gate, not a source-shape claim. The `0.3.0` WASM package
check builds the release `wasm-bindgen` artifact, loads it in headless Node, and
asserts native/WASM byte parity over the HTML and PDF corpus. The raw `.wasm`
budget is consciously raised from 3,200,000 to 3,300,000 bytes to account for
the vector-SVG/PDF surface; the gzip budget stays at 1,600,000 bytes.

Release verification included `cargo fmt --check`, `cargo check --all-targets`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`,
`cargo check --no-default-features --lib`, `scripts/check-policy.sh`,
`scripts/check-wasm-core.sh`, `scripts/check-determinism.sh`,
`scripts/parser-diff.sh`, `scripts/check-claim-discipline.sh --self-test`,
`scripts/commonmark-conformance.sh ci`, `scripts/batch-throughput.sh
--self-test`, `cargo test --features batch --lib batch::`, `cargo clippy
--features batch --lib -- -D warnings`, `scripts/e2e/run-all.sh ci`, and
`scripts/release-smoke.sh` against a local release build. The crates.io package
also passed `cargo publish --dry-run --locked --all-features`.

## 0.2.0 - 2026-07-03

Crate publishing is enabled for `franken_markdown`. The package metadata now
uses `license-file = "LICENSE"` for the custom MIT plus OpenAI/Anthropic rider,
removes the first-release `publish = false` guard, and excludes local Beads
state, performance artifacts, and the untracked source PNG from crates.io
packages.

Native output paths are safer. CLI renders, config saves, and batch renders now
stage filesystem writes in same-directory temporary files, preflight duplicate
and directory destinations, roll back already-committed siblings on later
failures, and refuse batch output aliases that would overwrite the explicit
input file.

Binary asset validation is stricter. The clean-room zlib inflater now validates
headers, Adler-32 trailers, stored-block length complements, final-block trailing
data, and oversubscribed Huffman tables; the PDF PNG pipeline validates fast-path
predictor payloads and rejects extra inflated scanline bytes.

Output correctness and machine contracts were tightened: empty image
destinations render alt text without an empty `src`, PDF warning collection
includes raw HTML text preserved in layout, theme page-size names are escaped in
public JSON, and WASM diagnostic severities are escaped like the rest of the
diagnostic envelope.

Release verification before the `0.2.0` metadata bump included
`cargo fmt --check`, `cargo check --all-targets`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`,
`cargo build --no-default-features`,
`cargo check --target wasm32-unknown-unknown --no-default-features --features wasm-bindgen --lib`,
`cargo check --all-targets --features batch`,
`cargo clippy --all-targets --features batch -- -D warnings`, and
`cargo test --features batch`.

## 0.1.0 - 2026-06-30

Initial binary release of the `fmd` CLI and library source.

### Cross-platform release and installer hardening (08f)

The release path is ready ahead of the first tag. A hand-rolled (no cargo-dist
dependency), tag-gated `.github/workflows/release.yml` builds the `fmd` CLI for
Linux (`x86_64-unknown-linux-gnu`), macOS Intel and Apple Silicon
(`x86_64`/`aarch64-apple-darwin`), and Windows (`x86_64-pc-windows-msvc`),
smoke-tests each freshly built binary (the Linux/macOS binaries via
`scripts/release-smoke.sh`; Windows via an equivalent inline PowerShell smoke),
packages it with a per-archive SHA-256, and attaches the archives plus a combined
`SHA256SUMS` to the GitHub release. The browser/WASM npm package ships separately
via the tag-gated `release-wasm.yml`. CI runs the macOS/Windows `platform-check`
matrix, the Linux quality gate, and the WASM package gate;
`scripts/release-smoke.sh` (version, help, `capabilities`/`doctor` JSON, HTML+PDF
render, stdin, `--text`, and the error path) runs in the quality gate and on
every Unix release binary. The working tree is kept free of untracked generated
artifacts (`.gitignore` covers the regenerable check/smoke outputs), and the
README's Installation section is updated to reflect the wired release workflows
(prebuilt binaries with checksum verification, plus the npm package).

### Zero-dependency core and the `fmd` CLI

The project began as a working clean-room Markdown-to-HTML engine with no
third-party dependencies and a single shared CLI entrypoint feeding both the
`fmd` and `franken_markdown` binaries. The CLI was built agent-first from the
start: render aliasing so `fmd README.md`, `fmd -`, and `fmd --text '# Hi'` all
work; stdout as data and stderr as diagnostics; stable exit codes; a global
`--json`; and discovery surfaces (`capabilities`, `doctor`, `robot-docs guide`,
`--robot-triage`). Native config persistence followed, using a dependency-free
`key=value` file with `FMD_CONFIG`/XDG/platform resolution and `--no-config` for
reproducible runs.

- Scaffold, engine core, and both binaries: [`8b66477`](https://github.com/Dicklesworthstone/franken_markdown/commit/8b664778844fd8c7f5aac95c9bae386bd74ae55a)
- Agent-ergonomic CLI surface (`capabilities`, `doctor`, `robot-docs`, `--text`, JSON): [`98c7f0b`](https://github.com/Dicklesworthstone/franken_markdown/commit/98c7f0bf3379f385df58d533bc8317697eddcf3e)
- Hardened agent contract (`--robot-triage`, exit codes, typo normalization): [`0ab6879`](https://github.com/Dicklesworthstone/franken_markdown/commit/0ab6879385b27d989b3f7e5edaddb711300d76f4)
- Native config persistence: [`95773aa`](https://github.com/Dicklesworthstone/franken_markdown/commit/95773aa9adf729cd21f9cd484f61a64332e25026)
- Reconcile `--to pdf` default-output behavior with the docs: [`7e9b805`](https://github.com/Dicklesworthstone/franken_markdown/commit/7e9b8052e856f5525668fcce1e76a013d6ec310d), stdout-aware `--out -`: [`8c6b3e5`](https://github.com/Dicklesworthstone/franken_markdown/commit/8c6b3e52d568650a115717ec79b27311f2d417d1)

### Clean-room parser: CommonMark/GFM subset

The parser grew a useful CommonMark/GFM subset and then a long correctness wave.
Block features include setext headings, indented code blocks, reference-style
links and images (full, collapsed, shortcut), lazy and nested lists, and
blockquote lazy continuation. Inline features include character-reference
decoding, robust link destinations, GFM bare-URL autolinks, and correct nested
emphasis (including `***` as bold-italic and four-times emphasis). A focused fix
wave tightened opener indentation, table-width validation, intraword
underscores, ordered-list interruption, code-span pipes, list looseness, and not
extracting reference definitions from inside fenced code. Source spans and
recoverable diagnostics were added for editor/WASM tooling.

- Setext headings: [`13ecaaa`](https://github.com/Dicklesworthstone/franken_markdown/commit/13ecaaa262281bad802ade3431b59cc4314e7824); reference links: [`25ae472`](https://github.com/Dicklesworthstone/franken_markdown/commit/25ae4725da11cdbc83a9761ee20de913564c4c9b); lazy/nested lists: [`2ef00e8`](https://github.com/Dicklesworthstone/franken_markdown/commit/2ef00e80b74ca79e9b25c6e609dc6ba135e2a0d5); indented code: [`141303f`](https://github.com/Dicklesworthstone/franken_markdown/commit/141303fe6a9bb6105e483fa74da9ee3fe3674110)
- Raw-HTML policy (escape by default, pass-through only with `--allow-html`): [`04b0ea8`](https://github.com/Dicklesworthstone/franken_markdown/commit/04b0ea8f8c0941bbc36929784e3ab5387b7feb47); source spans and diagnostics: [`c7587a2`](https://github.com/Dicklesworthstone/franken_markdown/commit/c7587a271f647af7260cff4e1bdddb55d0463fdd)
- Character references: [`61439fc`](https://github.com/Dicklesworthstone/franken_markdown/commit/61439fc634cd09e2e605a8448843857e1fe08bbc); robust link destinations: [`31241a1`](https://github.com/Dicklesworthstone/franken_markdown/commit/31241a16872a1e4c846283177bfb5149bcf0a74e); bare-URL autolinks: [`39eab0e`](https://github.com/Dicklesworthstone/franken_markdown/commit/39eab0ebb1933db5fee48b1e74b3b5ec85d5efca)
- Correctness fix wave (opener indentation, table widths, intraword underscores, list interruption, code-span pipes, escaped backticks): [`ff624e9`](https://github.com/Dicklesworthstone/franken_markdown/commit/ff624e93dba9b1ae9aaf96ec2238b1a342bf7cf6), [`a84451a`](https://github.com/Dicklesworthstone/franken_markdown/commit/a84451abd8881e970533743a9621d8373f41d4fa), [`8ee5973`](https://github.com/Dicklesworthstone/franken_markdown/commit/8ee59731e4334357d2acc40803b9f1f0ddd7cce0), [`69795df`](https://github.com/Dicklesworthstone/franken_markdown/commit/69795dfa32378b2d5527d2ea27cd61c3eace5aa5), [`796d53c`](https://github.com/Dicklesworthstone/franken_markdown/commit/796d53cc07e714f0df9a8dba9c1e00515ba8681c)
- List looseness/tightness correctness and bold-italic triple runs: [`193f762`](https://github.com/Dicklesworthstone/franken_markdown/commit/193f762d885cc1e60420f79eb87c03d4a56ddbd0), [`2973915`](https://github.com/Dicklesworthstone/franken_markdown/commit/297391551de40808445b57c97b9045a832824b00), [`3239366`](https://github.com/Dicklesworthstone/franken_markdown/commit/3239366b1dde463d4e97064d39e62d2b2eca425a), [`37c3b40`](https://github.com/Dicklesworthstone/franken_markdown/commit/37c3b40ffda3f52f1d36430eaff3a4019d7b0d34); reference defs not pulled from code fences: [`73190fc`](https://github.com/Dicklesworthstone/franken_markdown/commit/73190fcb8798d78d28709a11f2f384f6803a342d)

### HTML rendering and clean-room syntax highlighting

The HTML emitter produces a single self-contained file with inlined CSS, a
Cursor/GitHub-like light palette plus a dark-mode counterpart, table striping,
blockquotes, task lists, and custom-stylesheet replacement. A clean-room syntax
highlighter (no `syntect`, no regex engine) covers the languages common in
technical writing and is wired into the emitter with token CSS and regression
tests. Markdown URL schemes are sanitized to keep unsafe links out of output.

- Clean-room syntax highlighting for code blocks: [`252c1a8`](https://github.com/Dicklesworthstone/franken_markdown/commit/252c1a88430326c81529326f4eb6b1ee2662ec53)
- Sanitize unsafe Markdown URL schemes: [`d144c80`](https://github.com/Dicklesworthstone/franken_markdown/commit/d144c80f148367aab6996be0a321ccc71c582d73)
- Tight nested lists render without spurious `<p>` wrappers: [`57149d7`](https://github.com/Dicklesworthstone/franken_markdown/commit/57149d756ae4aaad4e7cf864ad8c253f12d87843)

### Shared theme model

A structured, dependency-free theme replaced the original flat fields: body and
mono font families, light and dark color tokens, spacing/measure/leading,
table density, code theme, dark-mode policy, and a page contract (size and
margins). It serializes to stable hand-rolled JSON for CLI/config/WASM callers.
The doctrine is one theme model for both surfaces, and a later wave routed every
PDF color through the same tokens the HTML stylesheet uses, so a theme change
now moves HTML and PDF together.

- Structured shared style model: [`064e4ab`](https://github.com/Dicklesworthstone/franken_markdown/commit/064e4ab943380a13bd357aaab5d7ccb73511d3a2)
- Unify PDF colors onto the shared theme tokens (`mwm.6`): [`5e1eaf4`](https://github.com/Dicklesworthstone/franken_markdown/commit/5e1eaf46048389c97d21a113800242f8838dc3f5)

### Font and text subsystem (clean-room TrueType)

The text subsystem is entirely the project's own code: a TrueType reader
(metrics, cmap), `glyf`/`loca` outline parsing, a glyf subsetter, GPOS pair-
kerning, and a GSUB standard-ligature parser. IBM Plex Sans and Computer Modern
(both OFL) are vendored and bundled via `include_bytes!` so the PDF path can
embed document-specific subsets with no system fonts.

- TTF/OTF reader (metrics + cmap): [`102bc05`](https://github.com/Dicklesworthstone/franken_markdown/commit/102bc05e77aa9e0b9ad15d8d7f61ea68bf0c22c1); `glyf`/`loca` outlines: [`de6712d`](https://github.com/Dicklesworthstone/franken_markdown/commit/de6712d5a88e24a51eb3d24f98347021ba1215e6); glyf subsetter: [`38621ae`](https://github.com/Dicklesworthstone/franken_markdown/commit/38621ae480939c8e9fb80c480277ba55b0c8134a)
- GPOS pair-kerning parser (`vxi.4`): [`d38bc62`](https://github.com/Dicklesworthstone/franken_markdown/commit/d38bc62bf504f2fd94ebd1fdbeaa5f5ee62ceb97); GSUB standard-ligature parser (`vxi.3`): [`60e7664`](https://github.com/Dicklesworthstone/franken_markdown/commit/60e7664a3b42095c1f2099009efca76e212c0c4d)
- Vendored fonts and bundled registry: [`127e5c0`](https://github.com/Dicklesworthstone/franken_markdown/commit/127e5c023309cf7835de2682609aedfbcb15e17d), [`6b58281`](https://github.com/Dicklesworthstone/franken_markdown/commit/6b582814b47427aa5f7acc793543588c2db4d282)

### Layout: Knuth-Plass line breaking and hyphenation

The layout engine provides fixed metrics, a Knuth-Plass optimal paragraph
breaker, a deterministic hyphenation core that uses the full TeX English Liang
patterns, microtype hooks, and preservation of styled inline runs through
breaking. A later re-profile gate deferred deeper layout/hyphenation work behind
evidence rather than speculation.

- Fixed metrics and paragraph breaker: [`789e6e1`](https://github.com/Dicklesworthstone/franken_markdown/commit/789e6e134ac7c45dcfd0c88a7731c2a7ff09fd10); hyphenation core: [`22ad648`](https://github.com/Dicklesworthstone/franken_markdown/commit/22ad648f986df5a2bd36eb9736fc87b211501bc1); full TeX patterns: [`cef6d16`](https://github.com/Dicklesworthstone/franken_markdown/commit/cef6d161cb55d125772320aec47b49b0b28c26b9)
- Preserve styled inline runs: [`e65aa68`](https://github.com/Dicklesworthstone/franken_markdown/commit/e65aa68a05121521b2f9113f9f58a28125db7838); microtype hooks: [`159ff5a`](https://github.com/Dicklesworthstone/franken_markdown/commit/159ff5a4e02ca133079e318d057dac39b8fd43aa)

### PDF writer: deterministic, embedded fonts, real typography

The PDF writer began as a deterministic PDF 1.7 MVP and grew into the project's
differentiator. It embeds document-subset fonts as CIDFontType2/Identity-H,
applies GPOS kerning through `TJ` positioning, shapes and embeds GSUB ligatures
while keeping text selectable, FlateDecode-compresses font programs and large
page streams, renders styled inline runs, does Knuth-Plass breaking with
blockquote bars/link styling/code panels, lays tables out as a measured-column
booktabs-style grid, and renders discretionary hyphen breaks with justified
lines. A consolidation commit unified the best PDF and parser paths. Generalized
keep-with-next pagination keeps headings, captions, and list intros with the
block they introduce. The tagged-PDF structure tree was upgraded from a flat v0
to a real accessible hierarchy rooted at one `/Document`, with decoration marked
`/Artifact`.

- Deterministic PDF MVP writer contract: [`e0f07ac`](https://github.com/Dicklesworthstone/franken_markdown/commit/e0f07ace7bb2647aeb39fe702d63d5d118c31d9a); embed document-subset fonts: [`91d4707`](https://github.com/Dicklesworthstone/franken_markdown/commit/91d4707c128ac1297a9b6ba8d6b13100f84de937)
- GPOS kerning via `TJ`: [`2adbe44`](https://github.com/Dicklesworthstone/franken_markdown/commit/2adbe44b7b46bc4095b7044cd211149fd589b01e); shape + embed GSUB ligatures (selectable): [`20d41b4`](https://github.com/Dicklesworthstone/franken_markdown/commit/20d41b4ead4bb80b33e0a3d45a50e83d2c72375b); FlateDecode-compress font programs (`fep.5`): [`debbe82`](https://github.com/Dicklesworthstone/franken_markdown/commit/debbe82effd59c67e36474324b46352a5a6470bd)
- Styled inline runs (`dy5.1`): [`247a074`](https://github.com/Dicklesworthstone/franken_markdown/commit/247a0746da7b601052feef3f639f0726580d1834); Knuth-Plass + blockquote bars + link styling + code panels: [`dd79635`](https://github.com/Dicklesworthstone/franken_markdown/commit/dd796357c9f57e6c1af238516af466a8a4824d8e); measured-column tables: [`4636265`](https://github.com/Dicklesworthstone/franken_markdown/commit/46362658235a4a46819f3d416714a780dd9c752d)
- Consolidate the best-of PDF + parser pipeline: [`b1343da`](https://github.com/Dicklesworthstone/franken_markdown/commit/b1343da3dba2be5fc86bfa4caef4ddea8e4d06e7); discretionary hyphen breaks + justified lines: [`95d31bf`](https://github.com/Dicklesworthstone/franken_markdown/commit/95d31bf6856ff762d7df4690e236252acc99f20b)
- Generalized keep-with-next pagination (`mwm.7`): [`b4560f6`](https://github.com/Dicklesworthstone/franken_markdown/commit/b4560f6a19a3004bd077784f063aecf51a13db0b); list-intro keep-with-next (`mwm.10`): [`d36b41e`](https://github.com/Dicklesworthstone/franken_markdown/commit/d36b41e18c36a08b2d7fd4d2dfe46f67da560b27)
- Hierarchical accessible tagged-PDF structure tree (`qw1.9`): [`955dd50`](https://github.com/Dicklesworthstone/franken_markdown/commit/955dd505211ad730576d4290fb47cb44881fd926); see [`docs/PDF_ACCESSIBILITY.md`](docs/PDF_ACCESSIBILITY.md)

### WASM package and native parity

The browser path is first-class. Browser package assets and a wasm-bindgen
adapter landed, then a real "first-class WASM" proof gate that builds the release
module, loads the generated module in headless node, renders HTML and PDF, and
asserts byte-identical native parity over a corpus with a committed `.wasm` size
budget. Determinism, negative-path, and size/checksum evidence followed, and the
package was made publish-ready with a hardened manifest and a tag-gated npm
release workflow. Capabilities now reports the package as
`publishable_unpublished`: one tag push from publication, with a claim-discipline
gate that blocks any `npm install` claim until it actually ships.

- Browser package assets and PDF render hardening: [`54dc00a`](https://github.com/Dicklesworthstone/franken_markdown/commit/54dc00a84866704062fc2b122914b861e7d8c1d0)
- Real WASM proof gate, headless render + native parity (`3i5.6`): [`e999d23`](https://github.com/Dicklesworthstone/franken_markdown/commit/e999d2355f03ea88d99934f96eab8511c188f61b); determinism/negative-path/size evidence (`3i5.5`): [`3bbf90b`](https://github.com/Dicklesworthstone/franken_markdown/commit/3bbf90b6c2668c62df8394079caefdf528aa9213)
- Publish-ready package + list-intro keep (`mwm.10`, `3i5.7`): [`d36b41e`](https://github.com/Dicklesworthstone/franken_markdown/commit/d36b41e18c36a08b2d7fd4d2dfe46f67da560b27)

### CommonMark conformance harness

An official CommonMark 0.31.2 conformance harness runs all 652 official
examples, normalizes fmd's styled HTML, and reports a per-example gap ledger
(pass / known-gap / intentional non-goal) plus a section summary. The current
result is a committed, ratcheted floor of **379/652 matched** (with raw-HTML
examples treated as intentional non-goals). The number is
surfaced in `capabilities --json` with a drift guard tying the two together. The
spec is vendored as dev-only test data.

- CommonMark 0.31.2 spec harness, measured + ratcheted (`mwm.3`): [`2ce6f8c`](https://github.com/Dicklesworthstone/franken_markdown/commit/2ce6f8cb051272d3bc158648c5c22df2c65a53b4); harness hardening: [`0719ca0`](https://github.com/Dicklesworthstone/franken_markdown/commit/0719ca0923153a0e636e6a8d0ed7d04b667b4037)

### Asupersync batch and streaming orchestration

The native batch path is scoped to keep the render core synchronous and
dependency-free. A deterministic worker-budget policy and a native-only batch
API/CLI contract landed as documents (`zmd.1.1`, `zmd.1.2`), defining the
`fmd batch <inputs...>` subcommand, the budget math, and a deterministic
`fmd-batch-receipt-v1`. The implementing module (`src/batch.rs`, bead `zmd.1.3`)
follows that contract: round-robin sharding across exactly `workers` Asupersync
tasks, per-file cancellation checkpoints, and receipts assembled in deterministic
input order. The `batch` cargo feature is the only thing that pulls Asupersync;
`scripts/check-wasm-core.sh` proves the core never sees it.

- Worker-budget policy (`zmd.1.1`): [`4e36b9f`](https://github.com/Dicklesworthstone/franken_markdown/commit/4e36b9f271d310dc3f96c3461666777dd7401c01); native-only batch API/CLI contract (`zmd.1.2`): [`60f09e3`](https://github.com/Dicklesworthstone/franken_markdown/commit/60f09e3aeccb20a19244560d2131ea1f041367df)
- See [`docs/BATCH_ORCHESTRATION.md`](docs/BATCH_ORCHESTRATION.md) and [`docs/BATCH_WORKER_BUDGET.md`](docs/BATCH_WORKER_BUDGET.md)

### Performance track (measurement-first)

Performance work is gated on evidence. A measurement-first roadmap, a measured
rendering gauntlet, and safe performance counters with run comparison make up
the proof track. Hot-path layout scans were de-duplicated. The gauntlet's
gates explicitly deferred a deeper layout/hyphenation rewrite and rejected a SIMD
subtree because the evidence did not justify them.

- Measurement-first optimization roadmap: [`470fa00`](https://github.com/Dicklesworthstone/franken_markdown/commit/470fa00fb832a78157fd68ffdd031d46aa1e9d9f); measured rendering gauntlet: [`d6c986c`](https://github.com/Dicklesworthstone/franken_markdown/commit/d6c986c16878461c00f9d1d23e4838ba0fe7b7fc); de-duplicate line/hyphen scans: [`b76fb3e`](https://github.com/Dicklesworthstone/franken_markdown/commit/b76fb3ef8ce03141b8ba4959228d187d8ea43b85)
- Safe counters + run comparison (`qw1.8`): [`9bf7007`](https://github.com/Dicklesworthstone/franken_markdown/commit/9bf7007c45da66b1b30b964dd2c76dee2baf55e5); re-profile gate defers the layout subtree (`qw1.7`): [`642c68c`](https://github.com/Dicklesworthstone/franken_markdown/commit/642c68c23995b09319d787e3f3008d6002eea0aa); reject SIMD subtree per the evidence gate (`qw1.6`): [`e983993`](https://github.com/Dicklesworthstone/franken_markdown/commit/e983993dcb56e90c75fb04382e1137c41ddf478b)

### Testing, CI, and quality gates

Quality is enforced by standing gates rather than convention. CI runs
formatting, all-target checks, a std-only core check, and four custom scripts: a
clean-room policy gate (no third-party normal deps, no banned renderer/browser
forests, no build scripts, unsafe-code forbidden), a WASM-core boundary gate, a
deterministic-output gate (byte-for-byte across repeated JSON/HTML/PDF renders),
and a README-to-`capabilities` claim-discipline gate. Test suites cover parser
conformance/metamorphic/differential/spans, fonts, layout, kerning, ligatures,
PDF structure and embedding, security, the WASM API and package, and a
deterministic render-tree visual golden.

- WASM core boundary gate: [`c460f00`](https://github.com/Dicklesworthstone/franken_markdown/commit/c460f00b717e1bb3e61ad9f74f6450ce6a2df0e2); clean-room policy gate: [`7d0b1c0`](https://github.com/Dicklesworthstone/franken_markdown/commit/7d0b1c053a6c75d6b2cf69f5c8b143266a989551); deterministic output gate: [`d2b9da3`](https://github.com/Dicklesworthstone/franken_markdown/commit/d2b9da35b24c9b2c5789710e94ceb7127bad05c7)
- README/capabilities claim-discipline gate (`mwm.9`): [`96f091b`](https://github.com/Dicklesworthstone/franken_markdown/commit/96f091ba89408712e8e86a7fc5f96bb2f6fbf021); metamorphic + fixture harness: [`951533e`](https://github.com/Dicklesworthstone/franken_markdown/commit/951533ee7456a8c0960d40d724fa2f2336ed8cd0); deterministic render-tree golden (`qw1.1.2`): [`2a12ebb`](https://github.com/Dicklesworthstone/franken_markdown/commit/2a12ebb37494208b4e89af0f816697fc809bfffa)

### Documentation, governance, and identity

Governance and project intent are checked in: the MIT License with the
OpenAI/Anthropic rider, project-local agent guidance (`AGENTS.md`), the
comprehensive plan, and reality-check bridge plans that keep the README honest
against the code. A hero illustration and a GitHub social-preview image were
added.

- Project docs, governance, and license rider: [`e3cd358`](https://github.com/Dicklesworthstone/franken_markdown/commit/e3cd3587b7f14a58bb2826ad75419d1e82064105)
- 2026-06-28 reality-check bridge plan + gap-closing beads: [`5c6af41`](https://github.com/Dicklesworthstone/franken_markdown/commit/5c6af418a1574f4f726d1e45c736f5d6f3fbcc9d); README reality-check after PDF typography landed: [`5917b30`](https://github.com/Dicklesworthstone/franken_markdown/commit/5917b30c435c48232a0ecf4e54d9d482fd912dcd)
- Hero illustration + social preview image: [`b8a3904`](https://github.com/Dicklesworthstone/franken_markdown/commit/b8a3904605d9625ce980fe8df627b459ff67155f)

## Notes for agents

- **Rust crate publishing is enabled.** `franken_markdown` is published to
  crates.io starting with `0.2.0`; the custom license rider is represented via
  `license-file = "LICENSE"`. The npm package
  (`@franken-suite/franken-markdown`) is handled by the separate tag-gated WASM
  workflow.
- **Status numbers are ratcheted floors, not goals.** CommonMark is 379/652
  in-scope normalized matches and CI fails if it regresses;
  `capabilities --json` reports the same number via a drift guard.
- **The `batch` feature is the only Asupersync entry point.** The render core,
  `--no-default-features`, and wasm builds never compile it.
  `scripts/check-wasm-core.sh` is the standing proof.
- **Determinism is enforced.** `scripts/check-determinism.sh` compares repeated
  JSON/HTML/PDF output byte-for-byte; `SOURCE_DATE_EPOCH` controls PDF dates.
- **The roadmap lives in beads.** `.beads/issues.jsonl` is the checked-in tracker;
  bead ids referenced above (for example `mwm.*`, `qw1.*`, `zmd.1.*`, `vxi.*`,
  `3i5.*`) map capability waves to their tracker entries.
- **Where to look first:** `src/cli.rs` for the command contract, `src/pdf.rs`
  and `src/layout.rs`/`src/text.rs` for typography, `src/parse/` for the parser,
  and `docs/` for the PDF accessibility and batch contracts.
