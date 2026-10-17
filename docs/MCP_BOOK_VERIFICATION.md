# Book verification and strict publishing over MCP

`fmd.verify_book` accepts the same ordered `chapters` and optional
`includeSources` as `fmd.render_book`, with exactly the same source admission,
logical path rules and bounded include expansion. It returns the existing
`BookRenderer::validate_links` report without loading fonts or images,
rendering pages, opening files, fetching URLs, or inventing a second Markdown
link grammar.

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

The MCP text-content item contains the unchanged `fmd-book-link-report-v1`
JSON report. `scope` is `expanded-html-navigation`. Its chapter-ordered entries
include `path`, `checked`, `external`, `unchecked`, and `findings`; the summary
aggregates those counts. Each finding has `code`, `destination`, and `message`.
The example reports a `missing_anchor` for the first chapter's link.

Finding broken links is a successful verification operation, so the report is
returned with `isError: false` even when `summary.findings` is nonzero. This is
not a clean bill of health: callers must inspect the counts and findings.
Invalid sources or an incomplete analysis instead return a JSON-RPC error,
never an empty or truncated report that resembles success.

The tool accepts only source fields. Render options, asset payloads, output
paths and `strictLinks` are rejected rather than silently ignored. Its source
limit remains the smaller of the MCP server limit and 64 MiB, including logical
path bytes. The shared validator independently limits AST size/depth, findings
and report bytes. Includes are checked after expansion, in the containing
chapter's navigation context, and include-only sources do not become chapters.

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

This is source-navigation verification, **not** PDF/A, PDF/UA, EPUB validation,
asset-availability validation, or a network link checker. External URLs and
non-chapter downloads remain explicitly counted as unverified. Headings and
referenced footnote anchors use their actual publication IDs; code examples,
images, raw HTML IDs and unpublished note bodies are outside this check.
No source spans are invented for text that may have moved through includes.

## Tests and execution status

Sixteen new Rust integration tests cover shared report equality, include
context, finding taxonomy, external/unchecked distinctions, all strict output
modes, ordering before resource decoding, unchanged successful bytes, Unicode,
reference-style links, footnote collisions, narrow schemas, input/analysis
limits and fresh requests. Two cases run through the real framed MCP server
loop: refusal followed by repair/publication, and tool notifications producing
no reply. They use the repository parser, validator and renderers, not doubles.
The thirteen publication tests remain, for 29 new tests across the two files.

**These Rust tests have not been run in the authoring environment.** Rust,
Cargo, rustfmt and an admitted DSR/RCH host are unavailable. Source hash,
lexical/delimiter, whitespace and JSON-fixture checks are not compilation,
runtime execution or a full repository pass. No GitHub Actions was used.

Run through the project's admitted build environment:

```sh
cargo test --locked --features mcp --test mcp_book_test --test mcp_book_verify_test --test mcp_test
cargo check --locked --all-targets --features mcp
cargo clippy --locked --all-targets --features mcp -- -D warnings
cargo build --locked --no-default-features
cargo fmt --check
```
