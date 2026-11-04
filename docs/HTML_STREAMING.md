# HTML output for large documents

Ordinary CLI HTML output is written incrementally. No new flag is needed:

```bash
fmd large.md --out large.html
fmd large.md > large.html
fmd large.md --to both --out large.html
```

The CLI still parses once. It emits ordinary HTML through a 64 KiB buffer,
including within a single enormous paragraph, code fence, list, table, alert,
or footnote. File output uses the existing staged publication and rollback
machinery: a failed HTML producer, PDF render, sidecar, or file write does not
publish a partial set of files.
Stdout remains a stream and can contain a prefix if its receiver fails; broken
pipes retain the existing successful-exit behavior.

Interactive HTML and browser preview still use their complete-string paths.
This change does not introduce a streaming parser or split Markdown source at
arbitrary byte boundaries.

## Library API

`render_html_document_to_writer` accepts the existing parsed document and HTML
options, and returns the emitted byte count. It performs the same font
validation as `render_html_document` before writing. The caller owns flushing,
error handling and publication of its destination:

```rust
use franken_markdown::{HtmlOptions, parse_markdown, render_html_document_to_writer};
use std::io::{BufWriter, Write};

fn render(source: &str, file: std::fs::File) -> franken_markdown::Result<usize> {
    let doc = parse_markdown(source);
    let mut output = BufWriter::with_capacity(64 * 1024, file);
    let bytes = render_html_document_to_writer(&doc, &HtmlOptions::default(), &mut output)?;
    output.flush()?;
    Ok(bytes)
}
```

One rendering state survives all chunks, so heading IDs, raw HTML containers,
automatic and explicit tables of contents, and footnote numbering remain
document-wide. Footnotes discovered inside other notes are emitted in the
same order as the string API. Output is byte-identical for the same document
and options. The tests compare exact bytes across all 24 repository Markdown
fixtures and additional combinations of profiles, raw HTML, TOCs and nested
notes. Existing HTML/PDF goldens are unchanged.

Ordinary inline escaping, prose, nested lists and quotes, table rows/cells,
code text and footnote bodies now stream inside each block. Every writer call
is at most 64 KiB, including a single huge text node or highlighted token.
The built-in Rust, JSON, TOML, YAML, CSS, Markdown and Mermaid lexers can emit
through a fallible callback with one pending token instead of retaining a
complete span vector. Exact span boundaries and output bytes match the
existing APIs. Huge code fences bypass the highlight cache; oversized fallback
span vectors and temporary strings are released after use.

This bounds ordinary output buffering, not total renderer memory. The source,
parsed AST, document indexes and shared rendering state remain resident.
Header/CSS and TOC generation, sanitized raw-HTML runs, formulas, diagrams,
embedded images and individual financial cells retain their specialized
String-producing paths. Richer external language lexers still use a complete
token vector. Parsing can set the process peak before output starts, so a
smaller output buffer need not reduce RSS for every input.

Writer failures stop emission immediately. The caller owns destination
rollback, and no Drop path retries a failed write or emits a trailing fragment.
Regression tests cover exact fixture/profile parity, giant individual blocks,
short writes, Interrupted errors, and a failure inside a giant highlighted
token.

## Initial document-level streaming measurement

The checked-in [memory report](performance/html-stream-memory-2026-11-04.json)
compares the full-buffer API with the initial top-level-block writer in the
same unoptimized native build, alternating three fresh processes per mode. This is
an output-buffering comparison, not an original-commit speedup claim.

The generated input contains 33,554,256 UTF-8 bytes in 48,072 paragraphs. Both
paths deliver 39,563,508 HTML bytes with the same stable FNV-1a fingerprint
`8864ba30dca2e0a1`. The fingerprint checks this large run; exact fixture byte
comparisons remain the correctness oracle.

| Median measurement | Full-buffer API | Writer API |
|---|---:|---:|
| Process peak RSS before HTML rendering | 79,396 KiB | 79,476 KiB |
| Process peak RSS after HTML rendering | 116,576 KiB | 79,476 KiB |
| Render and fingerprint time, unoptimized build | 864 ms | 801 ms |

The writer's median process peak is 31.8% lower in this case, and rendering did
not increase its pre-render high-water mark. The timing includes the probe's
per-byte output fingerprint and is not a release throughput promise.

The probe is `examples/fmd_html_stream_memory.rs`. It requires Linux process
memory counters. Build it with the pinned toolchain, then run `string` and
`stream` in separate processes against the same input:

```bash
cargo build --example fmd_html_stream_memory
target/debug/examples/fmd_html_stream_memory string large.md
target/debug/examples/fmd_html_stream_memory stream large.md
```

The report records the precise corpus, source-file hashes, toolchain and build
settings used for the measurements. Source I/O and the iOS export transport
have separate bounds and evidence in [the iOS plan](IOS_APP_PLAN.md#33-binary-transfer).

## Giant-block comparison

The [giant-block observations](performance/html-block-stream-memory-2026-11-04.json)
compare the live `542341d` CLI with the intra-block writer in one alternating
30-process invocation. Both builds were unoptimized; the measurement includes
source reading, parsing, rendering, stdout transport and parent-side hashing.
All five corpora produced matching byte counts and SHA-256 across both versions.

| Corpus | Before peak RSS | After peak RSS | RSS change | Wall-time change |
|---|---:|---:|---:|---:|
| About 16 MiB, one paragraph | 106,404 KiB | 106,796 KiB | +0.4% | +4.2% |
| About 16 MiB, plain code fence | 83,544 KiB | 54,336 KiB | -35.0% | -1.0% |
| About 4 MiB, Rust code fence | 154,824 KiB | 20,324 KiB | -86.9% | +5.9% |
| About 8 MiB, one table | 149,788 KiB | 127,756 KiB | -14.7% | +5.2% |
| About 8 MiB, many paragraphs | 30,084 KiB | 30,280 KiB | +0.7% | +8.2% |

These are memory gains for code fences and the table, with essentially
unchanged paragraph process peaks and an unoptimized timing cost in four
cases. The measurement does not isolate parser allocations or establish
release-build throughput, WASM memory use, or iPhone behavior.

The execution environment became unavailable after this initial comparison.
The report preserves the exact printed summaries, four surviving raw rows,
corpus recipes, build settings and known baseline hash. The other 26 raw rows
and candidate executable/source SHA-256 values were unavailable; they are
explicitly missing, not reconstructed measurements. Rerun these corpora before
using the observations as a release performance gate.

Before that interruption, 189 focused Rust tests and `cargo check --all-targets`
passed, including unchanged HTML/PDF goldens. The six source files were then
recovered from recorded edits and independently reviewed through GitHub; three
blob prefixes match the original inspected diff hashes. Final strict Clippy,
the full suite, the lean WASM check, and post-recovery build/format checks could
not be completed. The interrupted subsequent micro-optimization is excluded.
