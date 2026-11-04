# HTML output for large documents

Ordinary CLI HTML output is written incrementally. No new flag is needed:

```bash
fmd large.md --out large.html
fmd large.md > large.html
fmd large.md --to both --out large.html
```

The CLI still parses once. It emits each top-level block through a buffered
writer instead of accumulating the complete HTML string. File output uses the
existing staged publication and rollback machinery: a failed HTML producer,
PDF render, sidecar, or file write does not publish a partial set of files.
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

Working memory still includes the source, parsed AST, stylesheet, shared
rendering state and largest individual block or footnote. A giant table,
paragraph, code fence, or embedded asset can therefore still require a large
allocation. The writer removes the complete-document output buffer and drops
unusually large scratch capacity after the corresponding chunk is emitted.

## Measured memory scope

The checked-in [memory report](performance/html-stream-memory-2026-11-04.json)
compares the current full-buffer API with the current writer API in the same
unoptimized native build, alternating three fresh processes per mode. This is
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
