# Native Markdown language server

Build the opt-in `fmd-lsp` executable:

```sh
cargo build --release --features lsp --bin fmd-lsp
```

Configure an editor's Markdown LSP client to launch
`target/release/fmd-lsp --stdio`. No network listener or additional runtime is
needed. Standard output contains only Content-Length-framed JSON-RPC; process
errors go to standard error. The native `lsp` feature reuses the first-party MCP
JSON codec and transport. Default builds, the dependency-free core and WASM
feature set do not enable it, and no dependency is added.

## Implemented contract

The server supports `initialize`, `initialized`, `shutdown`, `exit`, and
`textDocument/didOpen`, `didChange`, `didSave`, `didClose`. Initialization
advertises UTF-16 positions and incremental synchronization. The server accepts
both ranged edits and full-text replacements. Changes in one notification are
applied sequentially to a temporary buffer, then committed together. It never
mixes a partially accepted change batch with the live document.

Every accepted open or change publishes the shared Rust parser's diagnostics,
including their actual source ranges and document version. Clean documents and
closed documents clear earlier findings. The same publication also reports
`missing_anchor`, `invalid_fragment`, `ambiguous_anchor`, and `missing_footnote`
warnings from shared HTML/book navigation analysis.
Resolution uses the whole parsed document, including later/nested headings and
referenced footnotes in publication order;
repeated missing targets are reported once per enclosing source block. The
warning range is explicitly that original Markdown block, not a guessed inline
match. Code-looking text, image resource paths and external links do not become
heading diagnostics. A refused analysis emits `link_analysis_incomplete`, never
a falsely clean result. This does not perform PDF layout, load fonts, test external
URLs or files, expand includes, or certify every link/export format.

URIs are opaque in-memory document keys, including `untitled:` buffers. The
server does not open files, follow links, resolve includes, fetch resources, or
write editor documents. It only examines text explicitly supplied by the client.

## Outlines, folding, and selection expansion

`textDocument/documentSymbol` returns section outlines from the shared parser
and source map, including setext headings and the source map's canonical IDs.
Clients advertising hierarchical symbols receive nested sections with separate
section and heading-selection ranges. Other clients receive flat
`SymbolInformation` locations. Empty headings have a nonempty display label.

`textDocument/foldingRange` folds heading sections and actual multiline code,
list, quote, table, math, HTML, definition-list and footnote blocks. Exclusive
end positions never consume the following heading. Results are ordered,
deduplicated, line-based, and respect the client's `rangeLimit`, including zero.

`textDocument/selectionRange` expands a UTF-16 caret through its enclosing block
and heading sections to the whole document. Results preserve the input order;
an invalid position rejects the request instead of silently moving the caret.

These are top-level, primary-source navigation features. The source map does
not yet expose independent heading anchors inside list/quote containers, so
the server does not invent nested source ranges or scan fence contents for
headings. External-file navigation, completion, rename, formatting and
workspace indexing are not advertised. Navigation requests reparse the current
synchronized buffer on demand; requests against an unsynchronized buffer return
`ContentModified` rather than locations from stale text.

## Synchronization and limits

Versions must increase; stale versions are ignored. An empty change array is a
valid version-only update on a synchronized buffer. UTF-16 ranges cannot split
an astral character's surrogate pair. LF, CRLF, lone CR and final empty lines
are supported. Columns past a line's end clamp to its end, as specified by LSP;
nonexistent lines and reversed ranges are rejected. A supplied `rangeLength`
is checked in UTF-16 units.

A rejected change batch leaves the stored text untouched, clears obsolete
diagnostics, and marks that buffer unsynchronized. The client must send a full
replacement (first change in a new batch), or close and reopen the document,
before incremental changes can resume. Rejection produces `window/logMessage`,
not an illegal response to a notification.

Limits are 8 MiB per protocol frame, 2 MiB per document, 16 MiB total stored
text, 64 open documents, 128 changes per notification, and 1,024 published
parser/anchor findings per version. Messages are bounded to 512 Unicode scalars.
When more findings exist, the last entry is `diagnostics_truncated`, rather than
a silent implication that the document was fully reported. Parser findings take
precedence over anchor warnings. The source and result bounds are not an exact
heap or CPU-time quota. Navigation accepts at most 4,096 headings/folds
and 128 requested selection positions. An oversized/truncated frame is fatal; a malformed
JSON body returns a parse error without consuming the next frame. Work is
synchronous and reparses an accepted document, so cancellation and incremental
parsing are not advertised. A successful exit requires `shutdown` followed by
`exit`; bare EOF or `exit` without shutdown is unsuccessful.

## Verification

```sh
cargo test --features lsp --bin fmd-lsp --test lsp_protocol_test
cargo clippy --features lsp --bin fmd-lsp -- -D warnings
cargo build --no-default-features
```

Tests cover real parser diagnostics, framed sessions, fragmented UTF-8 input,
UTF-16 edits, CRLF positions, atomic failure and resynchronization, version
ordering, resource limits, and shutdown behavior. The protocol tests exercise
the server's real JSON codec and parser rather than mocking those components.

The subprocess tests launch the actual `fmd-lsp` binary through pipes, with a
bounded wait. They exercise incremental Unicode edits, diagnostics, negotiated
outlines/folding, selection expansion, and clean versus abrupt process exit.

Eleven link regression tests cover global resolution, repeated references,
nested containers, original Unicode/CRLF block ranges, parser-diagnostic
preservation, encoded fragments, emitted footnote collisions, analysis/result
limits, and live heading rename/repair/desynchronization.
They were added but not executed in the authoring environment: Rust, Cargo,
rustfmt and DSR are unavailable. JSON-fixture parsing and source hash checks
are not a passing Rust build, server session, or full repository test suite.
