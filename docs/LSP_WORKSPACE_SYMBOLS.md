# Workspace heading search

`fmd-lsp` advertises `workspaceSymbolProvider` and implements `workspace/symbol`.
It searches the current synchronized buffers opened by the editor, including
opaque `untitled:` documents. It does not crawl workspace directories, read
files, use saved copies, or retain an index from an older document revision.
Cross-file link navigation is documented in `LSP_WORKSPACE_NAVIGATION.md`.

## Search and results

The request requires a string `query`. Matching uses Unicode lowercase and
literal substring search, not regular expressions or full linguistic case
folding. Every whitespace-separated term must occur in the heading title,
canonical anchor, or original buffer URI. Thus `guide install` can find an
Installation heading in an open guide file. Empty queries return a bounded
heading list.

Exact title matches rank first, followed by title prefixes, title substrings,
and terms distributed across title/anchor/URI. Ties use lowercase title, exact
URI, source position, and anchor. Ranking covers all admitted buffers: an exact
match in the last buffer can displace a weaker result from the first buffer.
At most 256 suggestions are retained and returned, in deterministic rank order.
This is a bounded search result, not an exhaustive workspace inventory.

Results are standard `SymbolInformation` objects with `name`, `kind` (String),
`location`, and `containerName`. The location retains the exact client URI and
UTF-16 source range. The container identifies that URI plus the canonical anchor,
so repeated headings remain distinguishable within and across documents.
No resolve request is required.

## Source authority and lifecycle

Headings and collision suffixes come from the renderer's document-link analyzer.
Code examples, raw HTML IDs, unreferenced notes, and ambiguous emitted anchors
do not become symbols. Headings inside containers and referenced notes retain
their authoritative enclosing top-level source range; finer nested spans are
not invented. Every request uses the current accepted version. Desynchronized
buffers are excluded while other synchronized buffers remain searchable;
full-text resynchronization restores them and close removes them.

The existing session limits (64 buffers, 16 MiB total, 2 MiB each) are enforced
before parsing. Queries allow 256 UTF-8 bytes and 16 terms. Per-document analysis
allows at most 4096 emitted anchors, and heading titles/IDs at most 4096 bytes.
The core analyzer's own AST limits also apply. Limit failures return RequestFailed
without a partial response; invalid query parameters return InvalidParams.
Worst-case JSON escaping is budgeted before response construction, with a
4-MiB ceiling. A narrower query can resolve an excessive response-size failure.

## Verification

Nine Rust regressions in `workspace_symbols_tests.rs` cover ranking across files,
Unicode and URI filtering, duplicate identities, nested/note ownership, bounded
best-result selection, query/input/output limits, buffer lifecycle, initialization,
request dispatch, and shutdown. They were authored but not executed because the
authoring environment has no Rust toolchain.

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
cargo clippy --features lsp --all-targets -- -D warnings
```
