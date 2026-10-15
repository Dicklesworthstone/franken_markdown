# PDF/A emission for retained books

`BookRenderer::render_pdf_pdfa(settings)` uses the retained book and its current
options to emit ordinary PDF or PDF/A-2b identification through the existing
writer. `BookRenderer::render_pdf_with_options(&options, settings)` instead
borrows complete `PdfOptions`, without copying their assets into the retained
configuration. Neither method reparses chapters or mutates the book.

```rust
use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{PdfASettings, PdfOptions};

let book = BookRenderer::new(&[
    BookInput { path: "intro.md".into(), source: "# Intro\n\n[Next](end.md#end)".into() },
    BookInput { path: "end.md".into(), source: "# End".into() },
])?;
let options = PdfOptions {
    title: Some("Manual".into()),
    metadata_epoch_seconds: Some(1_700_000_000),
    ..Default::default()
};
let pdf = book.render_pdf_with_options(&options, PdfASettings::a2b_strict())?;
# Ok::<(), franken_markdown::RenderError>(())
```

The ordinary and archival paths share source assembly, chapter-relative image
resolution, chapter-local citations, synthesized landing headings, heading
provenance and post-layout destination binding. The PDF/A writer—not a byte
patch pretending to convert an ordinary PDF—emits XMP and the output intent.
Binding then modifies only the annotation space already reserved by the book
pipeline and refreshes the deterministic document ID, preserving xref offsets.

`PdfASettings::OFF` takes the unchanged ordinary emitter. Strict mode requires
an enabled profile and applies the existing URI policy to the assembled source
before navigation/footnote preparation, as well as the document writer's checks.
Thus a forbidden action in an unpublished note cannot evade strict admission
merely because another internal link activated the navigation path. Non-strict
mode retains the existing document renderer's omission policy. A rejected call
does not change subsequent exports or stored options.

This enables the project's **PDF/A-2b emission path**, not independent PDF/A or
PDF/UA certification. Strict URI checks do not prove every ISO requirement or
validate links, image availability, font coverage, or visual fidelity. Use the
book link validator separately and independently validate generated artifacts.
No filesystem, network, clock, dependency, or native-only assumption is added.

## MCP publishing

`fmd.render_book` now accepts the existing single-document `pdfA` and
`pdfAStrict` options when `to` is `pdf` or `all`:

```json
{
  "name": "fmd.render_book",
  "arguments": {
    "chapters": [
      {"path": "intro.md", "markdown": "# Intro\n\n[Next](end.md#end)"},
      {"path": "end.md", "markdown": "# End"}
    ],
    "to": "all",
    "pdfA": "2b",
    "pdfAStrict": true,
    "strictLinks": true,
    "metadataEpochSeconds": 1700000000
  }
}
```

The existing option parser validates the profile and strictness before source
processing or asset decoding. The parsed profile is retained and passed to the
same book renderer; it is never discarded or replaced by a flat-document path.
Both fields require a PDF consumer; even `pdfA: "off"` on EPUB/site-only requests
is refused. `fmd.verify_book` remains source-only and accepts neither flag.

`strictLinks` and `pdfAStrict` are independent. The former checks the expanded
book's navigation before assets or rendering; the latter applies the PDF/A
writer's strict policy. A failure returns no partial multi-format result. With
`to: "all"`, only PDF changes: EPUB/site still consume the same transferred
font/image payloads and retain their ordinary byte output.

An enabled PDF/A output entry retains `format: "pdf"`, `application/pdf` and
base64 encoding and adds `pdfA: "2b"` plus the `pdfAStrict` boolean. These report
requested emission settings, **not validation or certification**. OFF/omitted
keeps the original receipt. `maxOutputBytes` includes archival objects and still
limits the aggregate raw PDF/EPUB/site bytes, not each artifact separately.

## Verification status

`tests/book_pdfa_test.rs` contains nine real-renderer integration tests covering
ordinary-byte parity, archival metadata, classic-xref validity, actual bound
chapter destinations, preservation of existing page/font streams, TOC and note
handling, includes, image routing, deterministic explicit metadata and geometry,
strict refusal, and reuse after failure. Existing book navigation tests remain.
Nine MCP regressions in `tests/mcp_book_pdfa_test.rs` cover direct-core parity,
option rejection, deterministic receipts, multi-format asset reuse, aggregate
limits and a real framed-session refusal/repair sequence. The existing thirteen
publishing tests remain; the former PDF/A exclusion checks now assert the new
schema and retain the rejection of fields on outputs without a PDF consumer.

Rust/Cargo/rustfmt and an admitted DSR/RCH host were not available in the
authoring environment. These Rust tests have **not run**. Source/diff checks and
independent validation of the PNG fixture do not establish compilation or
conformance. No Actions workflow or substitute renderer was used.

Run through the project's admitted build environment:

```sh
cargo test --locked --test book_pdfa_test
cargo test --locked --features mcp --test mcp_book_pdfa_test --test mcp_book_test --test mcp_book_verify_test
cargo check --locked --no-default-features --lib
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```
