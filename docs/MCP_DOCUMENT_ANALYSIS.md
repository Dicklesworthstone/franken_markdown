# Document intelligence and revision review over MCP

`fmd.stats` and `fmd.diff` analyze Markdown supplied directly by the caller. They
use the same native parser and analysis engines as the library and CLI. Each
successful result contains one MCP text-content item holding the complete JSON
report or HTML review, with `isError: false`.

## Document intelligence

```json
{
  "name": "fmd.stats",
  "arguments": {
    "markdown": "# Overview\n\nRead the [details](#missing).",
    "profile": "commonmark-gfm"
  }
}
```

The result uses the existing `fmd-document-stats-v1` schema: original UTF-8 byte
and line counts, words and reading times, readability estimates, structural
counts, heading outline and findings such as unresolved internal anchors.
Readability scores are heuristics; findings are report data and do not turn a
successful analysis into a tool error. Accepted fields are `markdown` and
optional `profile`.

## Typed semantic changes

```json
{
  "name": "fmd.diff",
  "arguments": {
    "oldMarkdown": "# Guide\n\n[Setup](install.md)",
    "newMarkdown": "## Guide\n\n[Setup](getting-started.md)",
    "profile": "commonmark-gfm"
  }
}
```

The default result is the core `fmd-diff-changes-v1` report, containing
`identical` and an ordered `changes` array. Changes identify headings, link
targets, code, tables, lists and moved blocks, with a readable `summary` and
`old_index` / `new_index`. An index addresses a top-level block in the parsed
revision; `null` indicates an addition or removal. These are not source-byte or
editor ranges.

This report provides change details beyond the aggregate `fmd-diff-v1` counts
currently returned by the CLI's `diff --json`. It normalizes ordinary prose
whitespace while retaining code, math and raw-HTML payloads verbatim. The
existing bounded alignment and move-detection behavior of `diff_documents`
applies; large, extensively changed documents can use its deterministic
fallback alignment.

## Visual review

Add `"format": "html"` to the same request for a self-contained Before/Current
HTML review. Both full revisions retain their formatting, code highlighting,
tables, links, revision-scoped heading anchors and footnotes. The shared HTML
renderer preserves supported safe markup and sanitizes or escapes unsafe HTML
under its default policy. Review highlighting uses the existing
structural comparison, so visible whitespace changes can appear even when the
typed semantic report treats the prose as equivalent.

`format` accepts exactly `json` or `html` and defaults to `json`. The complete
diff field set is `oldMarkdown`, `newMarkdown`, optional `profile`, and optional
`format`. HTML uses the default theme and the labels Before and Current.

## Grammar, ownership and limits

Both tools accept exactly `commonmark-gfm` and `gfm-plus`. The chosen grammar is
selected before parsing; diff applies it to both revisions and its HTML review.
Explicit GFM-Plus analysis recognizes standalone page-break comments, so
equivalent markers with harmless indentation differences compare identically.
Omitting `profile` preserves the CLI analysis defaults, including ordinary
page-break-comment treatment.

The server's configured Markdown byte limit applies before parsing. A diff
charges the **combined UTF-8 length of both revision strings** against that
limit, rather than giving each revision a separate allowance. Empty documents
are valid, including under a zero-byte limit. The existing JSON frame limit
also applies.

Unknown fields, wrong types, unknown profiles and unsupported formats are
rejected. These tools do not accept file paths, output paths, image/font bytes,
raw-HTML trust overrides or include-loading permissions. Include-looking text
remains supplied text; neither tool reads files, fetches URLs, expands local
includes or writes artifacts. Save the returned HTML explicitly in the client
when a review file is wanted.

## Focused verification

`tests/mcp_analysis_test.rs` exercises the actual dispatcher and both supported
stdio framing modes. It covers typed changes and block indices, native report
parity, dialect-sensitive inline parsing, page-break semantics, complete safe
HTML reviews, strict field admission, combined UTF-8 limits and successful
requests after earlier errors.

```bash
cargo test --locked --features mcp --test mcp_analysis_test --test mcp_test --test mcp_stdio_regression
```
