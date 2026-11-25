# Verify an assembled book PDF

`fmd book DIR --check-pdf` checks the configured book through the same chapter-aware
publication plan used by `fmd book DIR --to pdf`. It renders and inspects the
final positioned text and navigation in memory and does not create or replace
publication files or directories.

```bash
fmd book chapters --check-pdf
fmd book chapters --check-pdf --json
fmd book chapters --check-pdf --json --page-size a4 \
  --pdf-header-left '{chapter}' --pdf-footer-center '{page} / {pages}'
fmd book chapters --check-pdf --json --pdf-strict-pagination \
  --pdf-split-footnotes --pdf-baseline-grid-pt 14
```

The check uses `book.toml` chapter order, include-only sources and metadata,
chapter frontmatter, native configuration, selected authoring profile, and
the usual command-line overrides. Local images, supplied fonts and explicit
`--pdf-image DEST=PATH` mappings follow the publication admission rules. Books
remain offline unless `--embed-remote-images` is explicitly requested. Use
`--no-config` for a configuration-independent invocation.

With `--json`, stdout contains one complete `fmd-book-pdf-verification-v1`
report. Input warnings and parser diagnostics go to stderr. Without `--json`,
stderr contains a human-readable summary and findings, and stdout remains empty.

The report contains:

- `layout`: actual physical pages and positioned text runs, final font
  sizes, resolved and unresolved targets, rendering and accessibility findings,
  and a deterministic content digest. Page fitting and running page bands are
  reflected in the reported positions.
- `navigation`: source navigation checks with chapter ownership and original
  include-file ranges where the parser can prove them. This retains its existing
  HTML-anchor scope separately from the emitted PDF destination inventory.
- `pagination`: the final page planner's exactness, fallback and paragraph
  alternative diagnostics.
- `destinations`: authored links, footnote references and generated return
  actions with their chapter, original source location and final physical PDF
  page and vertical destination; unbound targets have `null` coordinates.
  Each entry distinguishes resolved, unresolved and unchecked resources.

Each destination retains two distinct spellings. `destination` is the canonical
book URL used for binding, such as `/guide/next.md#next`. Optional
`parsedDestination` is the parser-decoded target captured before chapter URL
canonicalization, such as `next.md#next`. Include expansion may already have
rebased that value before parsing. The `source` ranges identify the original
chapter or include-file token, including its quoting or entity spelling; the
parsed value is not a replacement for those ranges.

Parsed targets have an independent 1 MiB allowance across the book and an
8192-byte limit per target. Exhaustion or ambiguous ownership after HTML
lowering sets `parsedDestination` to `null`. Canonical destination binding,
source navigation and independently available source ranges remain intact.

The native check exits with `0` when it has no PDF findings, source-navigation
findings or parser diagnostics, and `65` when a complete check reports any of
those. Invalid arguments exit `64`; unreadable source/configuration/resources
normally exit `66`; rendering, strict-pagination and report-budget failures
exit `70`; output-stream failures exit `74`. An admitted `--fit-to-pages N`
remains a publication requirement: failure to meet it reports
`page_budget_unmet`, without returning a successful partial report.

`--max-report-bytes N` caps the complete UTF-8 JSON report on stdout. It accepts
`1..8388608` and defaults to 8 MiB. It is measured before the trailing output
newline; parser diagnostics on stderr retain their separate source-report
budget. Reports are never truncated. The core also bounds captured pages,
text runs and navigation metadata.

The check accepts PDF layout and resource flags, including `--pdf-a 2b` and
`--pdf-a-strict` to verify through the selected publication writer. Strict
archival refusals propagate as failed checks. It rejects output paths,
`--to`, `--check-links`, `--deny-broken-links` and HTML/EPUB-only settings.
Custom CSS has no PDF layout consumer and is not loaded by
the check. Use `fmd book DIR --check-links` for the existing source-only
navigation check, which does not load fonts or images or render PDF pages.

This is a layout, navigation and authoring audit. It does not certify PDF/A or
PDF/UA conformance, fetch external hyperlinks, or replace visual inspection.
Actual emitted page geometry and link destinations are checked using the book
pipeline; verifying a flattened merged document is not equivalent.

## Library and MCP

The Rust API offers `BookRenderer::verify_pdf()` and
`BookRenderer::verify_pdf_with_options(&PdfOptions, PdfASettings::OFF)`, returning a
`BookPdfVerificationReport`. The second method borrows export-local options and leaves
the retained renderer configuration unchanged. `to_json()` returns the same
bounded report used by the native command.

The MCP tool `fmd.verify_book` accepts `target: "pdf"` with the existing PDF
options and explicit resource bytes. Omitted `target` or `target: "navigation"`
retains the original source-only verification. Its `maxReportBytes` limit counts
the combined report and parser-diagnostic content items; the MCP maximum and
default are 16 MiB, while the core PDF report keeps its own 8 MiB limit.
