# Parser diagnostics in book publishing

Books retain the shared parser's findings alongside the chapter AST that will
be published. An inline work/depth limit, a block nesting limit, a malformed
reference definition, or an unclosed fence no longer disappears when a document
is rendered as a book. These are source findings, separate from the existing
navigation report and from PDF/EPUB conformance.

## Capture once and inspect the current revision

`BookRenderer::new` and `BookRenderer::from_sources` collect diagnostics during
their ordinary parse. `source_diagnostics()` returns an immutable
`BookSourceDiagnostics`; `chapters()` exposes every chapter in reading order,
including chapters with no findings. `diagnostic_count()` and `is_empty()` give
the complete count, and `to_json()` copies a prevalidated, cached JSON report.
Inspection does not reparse text, load assets, or render any output.

`build_book_with_diagnostics(inputs)` returns `(Book, BookSourceDiagnostics)` for
hosts using the public book model directly. The older `build_book` function
keeps its existing AST-only API. Both paths produce the same chapter AST and
publication bytes. The diagnosed path transfers parsed blocks into that AST;
it does not clone the complete tree to extract findings.

`BookWorkspace` publishes diagnostics in the same transaction as its source and
AST. A repaired chapter loses its old warnings; unchanged chapters retain theirs.
Include edits update the chapters whose expanded text changes. Source-set
replacement rebuilds the report in the new reading order. Failed, stale, or
over-budget updates preserve the previous AST, source revision, report,
presentation settings, and authorized resources.

The browser binding exposes `FmdBook.sourceDiagnostics()` as the same JSON
string. The native facade method only reads the current capture; it does not
start an export or grant access to any resource.

## Coordinate contract

The report schema is `fmd-book-source-diagnostics-v1`:

```json
{
  "schema": "fmd-book-source-diagnostics-v1",
  "coordinateSpace": "expanded_chapter",
  "chapters": [{
    "path": "guide/start.md",
    "sourceBytes": 374,
    "diagnostics": [{
      "code": "inline_work_limit",
      "severity": "warning",
      "start": 17,
      "end": 373,
      "message": "inline parsing work limit reached; this inline run is rendered as literal text"
    }]
  }],
  "summary": { "chapters": 1, "diagnostics": 1 }
}
```

`path` identifies the normalized published chapter. `start` and `end` are
half-open UTF-8 byte offsets in the exact expanded and URL-rebased chapter text
immediately before parsing, including frontmatter. `sourceBytes` is that text's
length. Without expansion/rebasing, these are offsets in the caller's original
chapter string, preserving Unicode, BOM and original newline bytes.

An included file can appear in several chapters, or only a selected portion of
it can appear. URL rebasing can also change text length. The report therefore
does **not** claim that a range addresses the original include file or an editor's
unexpanded buffer. It never invents an original source filename, line number, or
caret position. Unused include-only resources have no report entry and are not
parsed merely to produce diagnostics.

The parser's stable classifications currently include `inline_work_limit`,
`inline_depth_limit`, `block_depth_limit`, `malformed_reference`, and
`unclosed_fence`. Findings retain the parser's message, severity, and source
ordering. Consumers should display unknown future codes rather than hide them.

## Native and MCP behavior

Ordinary `fmd book` publication prints parser warnings on stderr in human mode.
With `--json`, the existing `warnings` array includes their code, chapter, and
expanded byte range; a separate `source_diagnostics` object contains the complete
structured report. Clean books retain the previous receipt shape.

`fmd book ... --check-links --json` keeps its navigation report on stdout and
emits a separate `book_source_diagnostics` warning event containing the parser
report on stderr. Its exit status still describes the requested navigation
check. Likewise, `--deny-broken-links` remains a navigation guard; recoverable
parser warnings are observable without changing that flag's scope.

`fmd.verify_book` keeps the original navigation report as its first MCP text
content item. If parser findings exist, a second text content item contains the
source diagnostic report. A source warning is report data, not a transport
failure. `fmd.render_book` adds a separate `sourceDiagnostics` object to its
publication receipt when findings exist, for every output format. These reports
describe the same parsed revision used for verification and publication.

## Admission and regression coverage

The diagnosed builder admits at most 4096 chapters and 64 MiB of chapter text and
paths. A report contains at most 4096 diagnostics and at most 4 MiB of serialized
JSON. Diagnostic codes are 1–64 ASCII letters, digits, or underscores; messages
contain 1–2048 UTF-8 bytes, and ranges must be inside their chapter.
Exceeding a limit returns `RenderError::InvalidInput` with a
`book_source_diagnostics` context. No truncated or falsely clean report escapes,
and native publication never replaces existing output after that refusal.

`tests/book_parser_diagnostics.rs` covers work/depth fallback codes, Unicode and
CRLF ranges, include/rebasing coordinates, unchanged export bytes, repaired and
rejected workspace revisions, report budgets, native warnings, and MCP reports.
The `wasm-book` feature adds a binding test against the same retained capture.
