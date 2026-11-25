# Book verification and strict publishing over MCP

`fmd.verify_book` accepts the same ordered `chapters` and optional
`includeSources` as `fmd.render_book`, with exactly the same source admission,
logical path rules and bounded include expansion. The default
`target: "navigation"` returns the existing source-navigation report without
loading fonts or images or rendering pages. Explicit `target: "pdf"` verifies
the assembled PDF through the same configured publication plan, fonts,
resources, fitting and final page emission as `fmd.render_book`. Neither mode
opens files or fetches URLs.

Both tools accept the same optional authoring `profile`: `"commonmark-gfm"`
or `"gfm-plus"`. Supply the same value when checking and publishing an edition.
The book engine selects this grammar before include discovery, expansion and
chapter parsing, so an include directive inside a code span remains literal
under the selected dialect. Strict publication checks that same parsed book.
Omission preserves the existing extended grammar; invalid values are refused
before source processing. This source option is independent of PDF/A's `pdfA`.

## Check and repair

Use this object as the `params` of `tools/call`:

```json
{
  "name": "fmd.verify_book",
  "arguments": {
    "chapters": [
      {"path": "intro.md", "markdown": "# Intro\n\n[Installation](setup.md#installation)"},
      {"path": "setup.md", "markdown": "# Getting started"}
    ]
  }
}
```

The first MCP text-content item contains the unchanged `fmd-book-link-report-v1`
JSON report. `scope` is `expanded-html-navigation`. Its chapter-ordered entries
include `path`, `checked`, `external`, `unchecked`, and `findings`; the summary
aggregates those counts. Each finding has `code`, `destination`, and `message`.
The example reports a `missing_anchor` for the first chapter's link.

When the parser reports source findings, a second text-content item contains
the complete `fmd-book-source-diagnostics-v1` report. This preserves warnings
such as `inline_work_limit` even when literal fallback leaves no link to check.
Its byte ranges explicitly address the expanded and URL-rebased chapter, not an
original include file. Clean calls retain their original one-item response.
See [BOOK_SOURCE_DIAGNOSTICS.md](BOOK_SOURCE_DIAGNOSTICS.md) for the shared
coordinate, admission, and retained-revision contract.

Finding broken links is a successful verification operation, so the report is
returned with `isError: false` even when `summary.findings` is nonzero. This is
not a clean bill of health: callers must inspect the counts and findings.
Invalid sources or an incomplete analysis instead return a JSON-RPC error,
never an empty or truncated report that resembles success.

Default navigation mode accepts source fields, `target` and `maxReportBytes`.
PDF options and resource payloads require explicit `target: "pdf"`; supplying
them to navigation mode is an argument error even when their values are empty
or false. Output paths, `to` and `strictLinks` are not verification arguments.
The source limit remains the smaller of the MCP server limit and 64 MiB,
including logical path bytes. The shared validator independently limits AST size/depth, findings
and report bytes. Includes are checked after expansion, in the containing
chapter's navigation context, and include-only sources do not become chapters.

## Verify the assembled PDF

Use the same sources and PDF options that will be published:

```json
{
  "name": "fmd.verify_book",
  "arguments": {
    "target": "pdf",
    "chapters": [
      {"path": "one.md", "markdown": "# Shared\n\n[Next](two.md#shared). Note[^n].\n\n[^n]: First note."},
      {"path": "two.md", "markdown": "# Shared\n\n[Back](one.md#shared)"}
    ],
    "page": {"size": "a5", "margins": 36},
    "toc": true,
    "pageNumbers": true,
    "footnoteBacklinks": true,
    "running": {
      "header": {"left": "{chapter}", "right": "{page}/{pages}"}
    },
    "maxReportBytes": 16777216
  }
}
```

The first text-content item has schema `fmd-book-pdf-verification-v1`:

| Field | Meaning |
|---|---|
| `layout` | Final physical pages and positioned text, plus render, accessibility and overflow findings |
| `navigation` | The existing source-navigation report, including original chapter/include locations for broken links |
| `destinations` | Authored references and their actual PDF destination status, physical page and vertical position |
| `pagination` | The final writer's exact or fallback pagination provenance |

Text capture occurs inside final page emission. It includes generated contents,
repeated table headers, continued footnotes and fitted running text. Chapter
language overrides, duplicate heading names, note identities and target
bindings therefore follow the actual book pipeline. Page numbers are one-based;
vertical coordinates use PDF points measured upward from the page bottom.
An unresolved authored destination retains its original identity instead of
exposing a temporary internal link token.

Within each `destinations` entry, `destination` is the canonical book URL used
for binding. `parsedDestination` optionally retains the parser-decoded target
before chapter URL canonicalization. Includes can already have rebased URLs
before parsing, so consumers use `source` ranges to select the original token.
The optional parsed-target sidecar admits 8192 bytes per target and 1 MiB across
the book; exhausted or ambiguous ownership produces `null` while preserving
canonical binding and the independently bounded source locations.

All PDF fields advertised by `fmd.render_book` are accepted, including
paper/margins, typography, baseline grids, fitting, running content, explicit
`images`/`fonts` bytes, pagination and archival writer settings. The same
option parser and resource budgets apply. `strictPagination` preserves typed
errors such as `pagination_infeasible`; it never produces a successful report
for weaker fallback pagination. `pdfA` and `pdfAStrict` select the publication
writer and its admission policy; this report is not independent PDF/A or
PDF/UA certification.

The core PDF report is bounded to 8 MiB. `maxReportBytes` applies to the combined
UTF-8 text of all returned report items, including parser diagnostics. It accepts
1 through 16,777,216 bytes and defaults to 16 MiB. Exceeding it returns
`report_too_large`, without a partial report. The larger combined ceiling leaves
room for the existing separately bounded source-diagnostic sidecar. Navigation
and diagnostic reports retain their own existing limits.

### Rust API

```rust
use franken_markdown::{PdfASettings, PdfOptions};
use franken_markdown::book::{BookInput, BookRenderer};

let renderer = BookRenderer::new(&[
    BookInput { path: "one.md".into(), source: "# One".into() },
    BookInput { path: "two.md".into(), source: "# Two".into() },
])?;
let report = renderer.verify_pdf_with_options(
    &PdfOptions::default(),
    PdfASettings::OFF,
)?;
let json = report.to_json()?;
```

`BookRenderer::verify_pdf()` uses the renderer's retained options.
`verify_pdf_with_options` applies options to that call. Both return a
`BookPdfVerificationReport`; neither publishes an artifact or performs host I/O.

## Refuse broken editions before rendering

Add `"strictLinks": true` to a `fmd.render_book` request. This uses the same
validator on the exact already-parsed book that the renderer will consume.
A finding returns reason `book_links_failed` before any asset decoding or
PDF/EPUB/site rendering. The error includes a bounded first-finding preview
and directs the caller to `fmd.verify_book` for the full machine-readable report.
An admission or analysis failure returns `book_link_analysis_failed`.

A failed strict request returns no artifact payload and writes nothing. With
`to: "all"`, no subset of the requested formats escapes on failure. A clean
strict check does not rewrite the document, options, artifact bytes or receipt.
Omitting `strictLinks` or setting it to false retains the ordinary non-strict
publication behavior. Successful requests still obey the aggregate artifact
limit described in `MCP_BOOK_PUBLISHING.md`.

The default navigation check and `strictLinks` guard are source-navigation
verification, **not** PDF/A, PDF/UA, EPUB validation, asset-availability
validation, or a network link checker. External URLs and
non-chapter downloads remain explicitly counted as unverified. Headings and
referenced footnote anchors use their actual publication IDs; code examples,
images, raw HTML IDs and unpublished note bodies are outside this check.
No source spans are invented for text that may have moved through includes.

## Regression coverage

The source-navigation and strict-publication suites preserve default report
bytes, include provenance, finding taxonomy, external/unchecked distinctions,
resource admission ordering and successful publication bytes. The PDF
verification suite covers configured source/resource parity, source-only
compatibility, option admission, strict pagination, report limits, included
source locations, archival writer settings and recovery through framed MCP
requests.

Run the complete affected suites and required repository gates:

```sh
cargo test --locked --features mcp --test book_pdf_verification --test book_pdf_verification_edges --test mcp_book_pdf_verify_test --test mcp_book_verify_test --test mcp_book_pdfa_test --test mcp_book_test --test mcp_resources_test --test mcp_pdf_configuration_test --test mcp_pagination_test --test mcp_font_collections
cargo check --locked --all-targets --features mcp
cargo clippy --locked --all-targets --features mcp -- -D warnings
cargo build --locked --no-default-features
cargo fmt --check
```
