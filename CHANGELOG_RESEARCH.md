# CHANGELOG Research Notes

Scope: full repository history through the current `0.3.4` release preparation.

## Evidence Sources

- `git log --oneline --decorate --max-count=20`
- `git log --oneline --no-merges v0.3.2..HEAD`
- `git show --stat --oneline HEAD`
- `git show --name-only --format=fuller HEAD`
- repository files after the initial scaffold commit
- `.beads/issues.jsonl` now contains the initial roadmap issue graph

## Version Spine

| Version / phase | Date | Evidence | Notes |
|---|---:|---|---|
| Pre-Phase-0 scaffold | 2026-06-26 | commit `8b66477` | Initial working zero-dependency Markdown-to-HTML renderer and `fmd` CLI scaffold |
| Governance/docs wave | 2026-06-26 | commit `e3cd358` | Added project docs, AGENTS guidance, comprehensive plan, changelog, and MIT rider |
| CLI ergonomics wave | 2026-06-26 | commit `98c7f0b` | Added agent-friendly `fmd` shortcuts, JSON/status surfaces, doctor, capabilities, and robot docs |
| Formatting normalization | 2026-06-26 | commit `5fac07e` | Rustfmt-only cleanup of the scaffold source |
| Roadmap bead graph | 2026-06-26 | commit `d694c86` | Seeded the project roadmap into Beads |
| First binary release | 2026-06-30 | tag `v0.1.0` | Release archives and installer asset lookup landed |
| Crates.io hardening release | 2026-07-03 | tag `v0.2.0` | crates.io publication, package trimming, staged writes, and stricter binary validation |
| Renderer capability release | 2026-07-07 | tag `v0.3.0` | SVG/PDF fidelity, Mermaid/MMD highlighting, local PDF assets, safer writes, batch receipts, and measured optimization work |
| DSR publication patch | 2026-07-07 | tag `v0.3.1` | DSR-built artifact line aligned to the release manifest after canceling the stuck Actions binary workflow; includes the late HTML base64 and PDF empty-segment drawing passes and leaves the rejected PDF decimal-string trial out of shipped source |
| PDF reading-quality release | 2026-07-08 | tag `v0.3.2` | Vector task-list checkboxes, long-token wrapping, TeX-correct shrink semantics, SVG text fidelity, npm package publication, and performance passes |
| DSR all-platform patch | 2026-07-09 | release prep for `v0.3.3` | 90 post-`v0.3.2` commits including the color-mix transparency fix: SVG/PDF fidelity, HTML local SVG embedding, measured speedups, coverage expansion, and DSR target coverage for Linux, macOS Intel, macOS Apple Silicon, and Windows |
| Issue-driven PDF fidelity patch | 2026-07-10 | release prep for `v0.3.4` | 46 post-`v0.3.3` commits closing GitHub issues #2 (remote images + JPEG `/DCTDecode` embedding, `5b1e6cc`) and #3 (Noto Sans Math symbol fallback face, `e63e463`), plus an SVG CSS/opacity/paint structural-parsing wave, `hsl()`/`hwb()` colors, measured perf passes, and a Windows CLI contract-test assertion fix |

## 0.3.4 Research Notes

The `v0.3.3..HEAD` log contains 46 non-merge commits before the release-prep
metadata bump. The dominant tracks are:

- User-filed issue closures: hotlinked images fetched by the CLI at render
  time with JPEG `/DCTDecode` embedding in the PDF writer (`5b1e6cc`, fixes
  #2), and a bundled ~56 KiB Noto Sans Math subset as a per-character symbol
  fallback face for math/arrow glyphs (`e63e463`, fixes #3). Both fixes were
  verified against the exact issue repros, rasterized and inspected, with new
  end-to-end suites in `tests/remote_image_test.rs` and
  `tests/symbol_fallback_test.rs`.
- SVG CSS structural parsing: declaration splitting (`09ee111`), quoted value
  delimiters (`4a9b604`), `!important` markers (`0e3a6e0`), top-level
  separators (`b17c3c0`), and trailing `var()` tokens (`f1c79a4`).
- SVG opacity/paint cascade: `initial` resets (`1c58704`), `unset` (`fb9af2c`),
  `inherit` (`3c4c296`), inherited paint keywords (`c2fd4b7`), `initial` paint
  (`f3cde82`), paint alpha composed with opacity (`add6c6b`), missing-paint
  fallback alpha (`fafd4dd`), gradient stop alpha (`3b651d7`), mask and
  currentColor alpha (`c70f9ab`), filter shadow flood alpha (`febeb4b`),
  fail-closed empty clip paths (`f9d8059`), `hsl()` (`1408a5f`) and HWB colors
  (`d8e3234`), and absolute length units (`d3d9080`, `fd8dbf4`).
- Measured speed work with rejected trials recorded: parser line scanner and
  paragraph continuation fast paths, HTML highlight-fragment caching and safe
  URL memoization, PDF file-ID FNV unrolling, hyphenation exception
  streamlining, and compression trials rejected on the data.
- CI repair: the Windows-only `cli_contract` equals-mapping assertion now
  compares the JSON-escaped path form (`ad86a92`), fixing the windows-latest
  test failure that predated this release line.

The WASM package gate keeps native/WASM byte parity across the package corpus.
The raw `.wasm` budget is consciously raised from 3,400,000 to 3,500,000 bytes
for the bundled math fallback face and JPEG `/DCTDecode` surface (measured raw
3,447,897 / gzip 1,557,945 bytes; the gzip budget stays at 1,600,000), and the
full suite stands at 1706 tests at release prep. Post-tag CI on windows-latest
surfaced three more contract tests asserting raw Windows paths against
JSON-escaped envelopes plus one platform-dependent `io::ErrorKind` assumption;
all four are fixed on `main` alongside the budget bump.

## 0.3.3 Research Notes

The `v0.3.2..HEAD` log contains 90 non-merge commits before the release-prep
metadata bump, including the committed color-mix transparency fix. The dominant
tracks are:

- SVG/PDF fidelity: pattern strokes (`9403319`), stroked SVG text (`288c796`),
  non-scaling stroke on SVG text (`2465bf0`), CSS-variable URL resource
  resolution (`d5b5b6a`), object-bounding-box patterns (`728cf15`), pattern
  viewBox transforms (`e09eec5`), chained drop shadows (`d42c0bf`), nested SVG
  data URIs (`b832e4e`), textPath labels (`6459c2e`), coordinate-list text
  placement (`0edc719`), and drop-shadow panic prevention (`f21485a`).
- HTML and asset fidelity: local SVG assets become self-contained data URIs
  (`b863967`), mixed-case SVG roots are recognized (`f0e3e8c`), and remote SVG
  imports are stripped from data-URI payloads (`9f77d30`, `05319cc`).
- Measured speed work: parser reference/inline fast paths, HTML font/base64 and
  highlighter caching, PDF shaped/table/simple-paragraph caches, bundled font
  caches, direct page/structure/object writers, compression capacity and fixed
  table work, and a ranking pass that orders recommendations by total stage
  cost.
- Test and coverage expansion: broad PDF/SVG branch tests, text/font/subsetter
  edge tests, compression and staged-write tests, CLI/batch error-contract
  tests, artifact-source safety tests, and the repository Markdown corpus soak.
- WASM package gate: the generated module is 3,351,808 raw bytes and 1,510,214
  gzip bytes for this release-prep tree. Native/WASM parity holds across the
  package corpus, so the raw budget is raised from 3,300,000 to 3,400,000 bytes
  for the expanded vector-SVG/PDF surface while keeping the gzip cap at
  1,600,000 bytes.

The final release-prep fix preserves alpha when parsing
`color-mix(in srgb, <color> <weight>, transparent)` for SVG PDF paint. The
regression checks that the PDF uses a native ExtGState with the expected fill
alpha and the source hue instead of leaving inherited black paint active.

## 0.3.0 Research Notes

The `v0.2.0..HEAD` log is dominated by two related tracks:

- Renderer fidelity: local PDF asset loading and safer CLI writes (`91afecc`),
  expanded SVG/table/typography rendering (`5423d18`), Mermaid/MMD fence
  highlighting (`791a3c8`), SVG text decorations (`83d6663`), modern SVG color
  tokens (`d469f67`), symbol/use viewport scaling (`be813af`), and checked-in
  frankenmermaid SVG rendering (`af97a82`).
- Measured speed work: parser, HTML, PDF layout/writer, highlighter, table
  layout, font shaping/subsetting, SVG drawing, and compression all received
  small behavior-preserving passes, with rejected trials recorded when the data
  did not justify the change.

The release gate evidence also now includes real WASM package construction and
native/WASM byte-parity checks. Raw `.wasm` grew with the vector-SVG/PDF surface,
so the committed raw budget moves from 3,200,000 to 3,300,000 bytes while the
gzip budget remains 1,600,000 bytes.

## Initial Capability Wave

Commit `8b66477` created the initial product skeleton:

- Rust 2024 crate with nightly toolchain pin.
- Library plus two binaries: `franken_markdown` and `fmd`.
- Clean-room Markdown AST and parser.
- All-in-one HTML emitter with default theme.
- CLI render command for files and stdin.
- Typed PDF not-yet-implemented path.
- Smoke tests for HTML rendering, escaping, custom CSS, serif theme, and PDF
  refusal.
- Example document at `examples/showcase.md`.

## Follow-up Work In This Session

The first post-scaffold wave landed in four logical commits:

- license changed to MIT with OpenAI/Anthropic rider,
- Cargo license metadata aligned with `/dp/asupersync`,
- `fmd` CLI improved with first-try render shortcuts, raw `--text`, global
  `--json`, `capabilities`, `doctor`, and `robot-docs guide`,
- project-local `AGENTS.md`, `README.md`, `CHANGELOG.md`, and comprehensive plan
  added,
- `.beads/issues.jsonl` seeded with the implementation roadmap,
- roadmap tightened around first-class WASM and Asupersync as native
  orchestration rather than render-core dependency.
