# Multi-document publishing over MCP

`fmd.render_book` sends an explicitly supplied Markdown collection through the
existing book engine. It produces a continuous PDF book, a multi-chapter EPUB,
a complete HTML-site ZIP, or all three from the same parsed book. No filesystem
or network access is performed. The native `mcp` feature is required; the core,
WASM feature graph, existing tools and single-document output APIs are unchanged.

## Request

Send this object as the `params` of an MCP `tools/call` request:

```json
{
  "name": "fmd.render_book",
  "arguments": {
    "chapters": [
      {"path": "guide/intro.md", "markdown": "# Introduction\n\n{{#include ../parts/body.md}}\n\n[Next](../next.md#next)"},
      {"path": "next.md", "markdown": "# Next\n\n[Back](guide/intro.md#introduction)"}
    ],
    "includeSources": [
      {"path": "parts/body.md", "markdown": "Reusable **content**."}
    ],
    "to": "all",
    "title": "Manual",
    "pageNumbers": true,
    "customCss": ".fmd { line-height: 1.5; }",
    "maxOutputBytes": 16777216
  }
}
```

Chapter array order is reading order, not lexical filename order. Include-only
sources do not become chapters, EPUB spine items or site pages. The shared
`BookRenderer::from_sources` owns relative paths, includes, selectors, cycles,
expansion limits, chapter-local notes and chapter-link canonicalization. Paths
are logical source names, not permission to read a similarly named host file.
Missing includes, duplicate normalized paths and output collisions are errors;
the adapter never falls back to the filesystem or returns a partial book.

`to` is `pdf` (default), `epub`, `site`, or `all`. For `all`, output order is
PDF, EPUB, site ZIP. Shared `images` and `fonts` use the existing MCP resource
schema: explicit base64 payloads, book-relative image keys, canonical font slots
and optional font weights. Their byte storage is transferred between format
options rather than duplicated for each output.

The existing PDF option parser supplies paper/margins, base/heading/table sizes,
font scale, language, microtypography, pagination, contents, running text and
explicit metadata timestamps. HTML/EPUB use the existing theme and stylesheet
parser. An explicitly empty `customCss` is preserved. Options must have a real
consumer in at least one requested output: PDF-only options are rejected for
EPUB/site, and HTML-only options are rejected for PDF. EPUB does not accept
raw-HTML pass-through or HTML font-container selection. PDF/A and interactive
HTML are not offered by this tool; they are not silently downgraded to ordinary
book PDF or site HTML.

## Result and limits

The MCP text-content item contains a JSON object with schema
`fmd-mcp-book-publication-v1`, chapter count, original Markdown `sourceBytes`,
aggregate `artifactBytes`, and an `outputs` array. Each output contains `format`,
`mimeType`, `encoding` (`base64`), raw `bytes`, and `data`. Decode `data` once to
obtain the artifact. The site ZIP includes the book engine's landing page,
chapter pages, navigation and offline search. No `out` path is accepted.

At most 4096 total chapter/include sources and 4096 UTF-8 bytes per path are
admitted. Source text plus logical path bytes share the lower of the server's
input limit and 64 MiB. These checks run before copying source strings. The
book core separately enforces its existing expansion depth, resolution, copy
and expanded-source limits. MCP's existing font/image admission is retained.

`maxOutputBytes` limits the combined raw artifact bytes before base64 encoding:
default 16 MiB, maximum 32 MiB, minimum one byte. Each completed native artifact
is checked before encoding. This is not an allocation or CPU-time quota inside
the synchronous renderer. Any failure returns an error instead of a partial
multi-output success. No output is written or published remotely.

## Verification status

Thirteen new Rust integration tests exercise the real public MCP dispatcher and
book renderers, including byte parity for all formats, source ordering,
transclusion, relative image resources, custom geometry and metadata, empty CSS,
invalid options/sources, deterministic receipts, discovery and aggregate limits.
Existing discovery tests now require the additional tool and preserve all five
previous tool names.

Rust/Cargo/rustfmt and an admitted DSR/RCH host were unavailable in the authoring
environment. The tests have **not run**. Source hash, lexical/delimiter,
whitespace and JSON-fixture checks do not establish a passing Rust build or a
rendered artifact. No GitHub Actions workflow was used.

Run through the project's admitted build environment:

```sh
cargo test --locked --features mcp --test mcp_book_test --test mcp_test
cargo check --locked --all-targets --features mcp
cargo clippy --locked --all-targets --features mcp -- -D warnings
cargo build --locked --no-default-features
cargo fmt --check
```
