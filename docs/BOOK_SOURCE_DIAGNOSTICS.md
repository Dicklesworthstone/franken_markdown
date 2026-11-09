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
`source_location(chapter_index, diagnostic_index)` returns separate original-source
attribution. The public fields and construction of `BookChapterDiagnostics`
remain unchanged; source locations live in the report's private sidecar.

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
Moving a selected snippet within its original file refreshes its source locations
even if the expanded chapter is byte-for-byte unchanged and no chapter is reparsed.

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
it can appear. URL rebasing can also change text length. The expanded `start` and
`end` fields therefore retain their existing meaning. New builds add a separate
`source` field to each diagnostic:

```json
{
  "status": "mapped",
  "ranges": [{
    "path": "parts/example.md",
    "start": 23,
    "end": 41,
    "kind": "exact"
  }]
}
```

These are half-open UTF-8 ranges in the caller's original chapter or include-only
source. They come from the shared include expander's byte map, composed with the
actual parser-owned URL replacements. No text matching, filename inference,
second include grammar, filesystem read or reparsing is used to locate a finding.
Nested selections, stripped anchor markers, repeated equal-looking snippets,
BOM, Unicode and original newline bytes keep their own coordinates.

| Range kind | Meaning for navigation |
| --- | --- |
| `exact` | The diagnostic bytes were copied unchanged from this original range. |
| `rewritten` | URL rebasing replaced this complete original token. An interior position in its new spelling selects the complete original token. |
| `generated` | Expansion inserted a separator at this original source boundary; `start` equals `end`. |

A warning spanning multiple sources has multiple ranges. Disjoint original
fragments remain separate rather than selecting removed markers or an invented
contiguous region. If a complete location cannot be represented within its limits,
the diagnostic instead carries `{"status":"unavailable","reason":"range_limit"}`
or another stable reason. Unavailable locations never include partial ranges.
Unused include-only resources still have no report entry and are not parsed just
to produce diagnostics.

The book workbench offers a **Select source** button for each verified range,
including include-only files. It validates the named source against the captured
project and checks every requested UTF-8 boundary in one scan per used file.
Selections use UTF-16 offsets and the existing editor's CRLF normalization.
Edits, source replacement, cancellation and stale asynchronous completions retire
the result. Generated boundaries and rewritten tokens are labeled explicitly.
The **Open publishing chapter** action remains available separately.

Older generated packages omit `source`. Their diagnostics remain readable, and
the workbench explains that matching WASM is required for original-source jumps.
Expanded offsets are never substituted for unavailable original offsets. This
source navigation applies to parser findings; the separate link report continues
to identify publishing chapters.

The parser's stable classifications currently include `inline_work_limit`,
`inline_depth_limit`, `emphasis_work_limit`, `emphasis_depth_limit`,
`block_depth_limit`, `malformed_reference`, and `unclosed_fence`. Findings retain
the parser's message, severity, and source ordering. Consumers should display
unknown future codes rather than hide them.

Emphasis limits preserve already-resolved styles and accepted links, leaving
remaining or surplus delimiters as literal text. They differ from the inline
tokenizer's work/depth fallbacks, which render the entire affected inline run
literally. All of these warnings identify the affected run's original source
envelope; successful publication can still require author review.

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
paths. A report contains at most 4096 diagnostics. The original report data retains
its **4 MiB** serialized limit. Optional source metadata has a separate **2 MiB**
allowance, for at most **6 MiB** on the wire. This separate admission preserves
previously valid near-limit diagnostic reports. Diagnostic codes are 1–64 ASCII
letters, digits, or underscores; messages
contain 1–2048 UTF-8 bytes, and ranges must be inside their chapter.
Exceeding a limit returns `RenderError::InvalidInput` with a
`book_source_diagnostics` context. No truncated or falsely clean report escapes,
and native publication never replaces existing output after that refusal.

The complete book's temporary provenance map admits at most 65,536 fragments and
256 KiB of logical source paths, with a 4096-byte individual path limit. One
diagnostic can expose at most 64 source ranges; the whole report admits 16,384
ranges and 256 KiB of repeated path text. Metadata exhaustion produces explicit
unavailability and leaves the warning and publication intact. Native and browser
workers use the separate diagnostics wire allowance; link reports retain their
existing 4 MiB limit.

`tests/book_parser_diagnostics.rs` covers work/depth fallback codes, Unicode and
CRLF ranges, include/rebasing coordinates, unchanged export bytes, repaired and
rejected workspace revisions, report budgets, native warnings, and MCP reports.
The `wasm-book` feature adds a binding test against the same retained capture.
`tests/book_source_locations.rs` and the source-map unit regressions cover nested
and selected includes, repeated snippets, rewritten URL interiors, split-source
warnings, generated separators, mapping-only edits, transaction rollback and
unchanged publication bytes. The adapter/worker/DOM-double tests in
`wasm/tests/book_source_diagnostics.test.mjs` cover source identity, UTF-8-to-UTF-16
selection, stale results, old packages, metadata admission and both worker
directions. They are not a claim of real-browser or generated-WASM execution.
