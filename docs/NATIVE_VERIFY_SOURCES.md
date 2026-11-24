# Verify the document that includes produce

`fmd verify` prepares file inputs with the same bounded include expander used
by native rendering. A document containing `{{#include parts/body.md}}` is
checked with that fragment present: included headings resolve links, included
parser and layout problems appear in the report, and the text layer contains
the actual composed prose.

```sh
fmd verify report.md --json
fmd verify manual.md --profile commonmark-gfm --json
fmd verify glossary.md --profile gfm-plus --a11y --json
fmd verify manual.md --max-input-bytes 1048576 --json
fmd verify report.md --load-local-images --page-size a5 --font serif --json
fmd verify report.md --pdf-font body-regular=Body.ttf --pdf-strict-pagination --json
fmd verify manual.md --links --links-cache link-results.jsonl --json
```

## Source semantics and limits

Nested paths resolve relative to the including file. Line and named-snippet
selectors, source-relative image/link destinations, code-literal protection,
cycle detection and sandbox checks use the existing rendering implementation.
The top input's directory is the allowed source root. Symlinks cannot authorize
an include outside that root, and only regular UTF-8 files are accepted.

`--max-input-bytes` bounds each source read and the expanded document; its
default is 64 MiB. The complete expansion also retains the existing 64 MiB
ceiling, 16-level depth limit and 4096-resolution limit. Missing sources,
invalid UTF-8, invalid selectors, cycles and exceeded budgets fail with input
exit code 66 before a report or link cache is written. The verifier never
reports a partial source graph as clean.

Standard input has no source directory. An active include in `fmd verify -`
fails with usage exit code 64, matching ordinary rendering. Directive examples
inside fenced, indented or inline code remain literal and need no filesystem
access. `--profile` is applied before include-code classification and parsing;
omission retains the existing default grammar. Invalid profiles fail before
the input is read.

## Report and diagnostic coordinates

The existing version-1 JSON schema, verdict codes and digest algorithm are
unchanged. Plain documents with no selected profile retain their existing
reports. A composed document's report describes its expanded text and selected
PDF layout. Parser findings explicitly say `expanded source bytes`, and
human caret locations label the virtual source as `file.md (expanded includes)`.
These are expanded-buffer positions, not invented line positions inside the
original primary file or an included file.

## Explicit PDF layout and resources

Verification ignores machine-specific rendering config, `SOURCE_DATE_EPOCH`
and frontmatter presentation defaults. With no new flags, it keeps the original
default PDF layout and empty host-resource set. Explicit options make it
possible to check the paper, typography and resource bytes used by a particular
publication:

| Input | Verification options |
|---|---|
| Paper and independent margins | `--page-size`, `--margin-top-pt`, `--margin-right-pt`, `--margin-bottom-pt`, `--margin-left-pt` |
| Typography | `--font`, `--font-scale` / `--type-size`, `--lang`, `--pdf-base-font-size`, `--pdf-heading-scale`, `--pdf-table-font-size` |
| Paragraph and page layout | `--microtype`, `--typography-homogeneous`, `--typography-antiriver`, `--typography-pareto`, `--pdf-optimal-pagination`, `--pdf-strict-pagination`, `--pdf-baseline-grid-pt`, `--fit-to-pages` / `--target-pages` |
| Contents, code and notes | `--toc`, `--toc-depth`, `--pdf-line-numbers`, `--pdf-footnote-backlinks`, `--pdf-split-footnotes`, `--pdf-theme-spacing` |
| Running headers and footers | The same six text slots, rules, page numbers, skip-first setting and logo destination, position and height options as PDF publication. Supply template values with `--title`, `--author` and `--metadata-epoch-seconds` |
| Explicit images | Repeat `--pdf-image DEST=PATH`; `DEST` is the exact parsed/rebased destination, including extensionless keys and URLs |
| Source-relative images | `--load-local-images` enables the renderer's sandboxed local PNG/JPEG/GIF/SVG discovery |
| Host fonts | Repeat `--pdf-font SLOT=PATH`, with the renderer's `--pdf-font-face` collection selectors and `--pdf-font-weight` pins |

The shared native parsers enforce the same geometry, font selection, scale and
asset limits as rendering. Explicit images override automatic images, including
an invalid explicit override: the report describes the resulting unsupported
image instead of silently choosing different bytes. `--max-pdf-image-bytes`
bounds each image (32 MiB by default). Explicit image inputs must be regular
files, so a pipe cannot turn a verification request into an unbounded wait.
Fonts retain the existing font-asset byte limit and validation.

Local loading resolves rebased include destinations against the top input's
directory. Images in supported HTML and complete note bodies participate.
Out-of-root symlinks remain unavailable. Remote image URLs are never fetched;
use an explicit `--pdf-image URL=PATH` to check a captured remote resource.
Stdin accepts explicit images/fonts, but rejects `--load-local-images` because
it has no source directory. Missing automatic images remain report findings;
missing explicit files and oversized assets fail with input exit code 66.

Strict pagination and incompatible layout combinations retain the renderer's
typed failures, with exit code 70 and no partial report. An unmet explicit page
budget also fails with `page_budget_unmet` after fitting, before external link
checking or cache writes. A failed layout cannot be reported as clean merely
because `--a11y` was selected.

Running bands are checked against the final page geometry, including logo
decoding and the space available for positioned text and rules. A decorative
logo needs an explicit `--pdf-image DEST=PATH` binding unless the document body
already supplies that resource through local loading.

```sh
fmd verify report.md --pdf-header-left '{title}' --pdf-footer-right '{page}/{pages}' \
  --pdf-header-image brand --pdf-image brand=branding/logo.svg --json
```

The report lists body text runs. Running content lives in the margins and can
cause a render error even when the body fits. Set `--metadata-epoch-seconds`
to the publication's `SOURCE_DATE_EPOCH` when verifying a `{date}` template.
PDF serialization and PDF/A or PDF/UA certification are separate from this audit. The version-1 report and digest describe measured
content and findings; they are not a cryptographic manifest of option or asset
bytes. Different image pixels with identical dimensions can yield the same
report.

## Optional external links

`--links` remains opt-in. Its existing timeout, cache and HTTP checking policy
are unchanged. Collection now follows the PDF preparation and complete AST,
including table headers/cells, definition terms/descriptions, note bodies and
the supported HTML-anchor subset. Repeated URLs are checked once. Code
examples, image resources, unsupported HTML attributes and unused reference
definitions do not authorize network requests. Unreferenced note bodies are
included because the PDF baseline publishes them as trailing notes.

The cache is an output. Before rendering or making any external request, the
verifier checks its path against the main input, every consulted include and
every selected image/font input, including symlink and hard-link aliases.
Missing and explicitly overridden automatic image paths remain protected when
local loading is enabled. Explicit resources keep this protection with stdin.
A collision fails with usage exit code 64. No cache is read or written when
`--links` is omitted.

## Regression coverage

The native CLI tests compare reports with independently written expanded
Markdown, exercise nested and selected content, source-relative resources,
profile-specific code protection, graph failures, byte budgets and cache/source
aliases. Layout/resource tests compare complete reports with the core API and
actual emitted PDFs, exercise strict refusals and page budgets, and preserve
source/resource aliases. External-link coverage uses prepopulated cache records;
resource tests run without fetchers, so the suites do not need network access.

```sh
cargo test --locked --features lsp --test cli_verify_sources \
  --test cli_verify_layout --test cli_verify_cache_safety_test \
  --test cli_verify_parser_limits
```
