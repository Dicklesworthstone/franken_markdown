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
closed documents clear earlier findings. This is parser diagnostics, not the
more expensive PDF/font verification pipeline, a second Markdown parser, or a
promise to detect every broken link.

URIs are opaque in-memory document keys, including `untitled:` buffers. The
server does not open files, follow links, resolve includes, fetch resources, or
write editor documents. It only examines text explicitly supplied by the client.

## Synchronization and limits

Versions must increase; stale versions are ignored. UTF-16 ranges cannot split
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
parser findings per version. An oversized/truncated frame is fatal; a malformed
JSON body returns a parse error without consuming the next frame. Work is
synchronous and reparses an accepted document, so cancellation and incremental
parsing are not advertised. A successful exit requires `shutdown` followed by
`exit`; bare EOF or `exit` without shutdown is unsuccessful.

## Verification

```sh
cargo test --features lsp --bin fmd-lsp
cargo clippy --features lsp --bin fmd-lsp -- -D warnings
cargo build --no-default-features
```

Tests cover real parser diagnostics, framed sessions, fragmented UTF-8 input,
UTF-16 edits, CRLF positions, atomic failure and resynchronization, version
ordering, resource limits, and shutdown behavior. The protocol tests exercise
the server's real JSON codec and parser rather than mocking those components.
