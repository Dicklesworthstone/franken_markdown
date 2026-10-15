# Heading reference search in opened editor buffers

`fmd-lsp` advertises `referencesProvider` and `documentHighlightProvider`.
Editor Find References requests now find uses of a heading in the explicitly
opened, synchronized Markdown buffers. Document highlights use the same
resolution, but return ranges only in the requesting document.

Start a request on an ATX/setext heading, an explicit link destination, or a
parser-verified reference-style link. Link selection reuses the definition
provider's real-parser probe, publication check and exact AST restoration.
The search follows the shared renderer's emitted heading IDs, including
Unicode/percent-encoded fragments, duplicate-heading suffixes and headings
inside containers. Equal heading titles or equal IDs in different documents
are not the same target.

Relative paths, extensionless Markdown paths, directory index paths and explicit
`file:` URLs use the existing opened-buffer resolver. An ambiguous URI alias or
unopened target is not resolved by guessing. This feature performs no file reads,
network requests, include expansion or writes. Its workspace is the client's
opened text, not a complete directory index.

## Ranges and supported symbols

Results are the **enclosing top-level source blocks** supplied by the parser.
Repeated uses inside one paragraph/list/quote are deduplicated to one location.
The provider does not invent exact inline offsets or expose these ranges as
safe rename edits. A link can select a particular nested heading by its exact
ID, even when several headings share an enclosing source block. A caret directly
inside that container cannot select an independently mapped heading, so it
returns no symbol. Document-root links and footnote symbols are outside this
heading-search contract.

`textDocument/references` requires the standard boolean
`context.includeDeclaration`; when true, the heading's source block is included.
Results have deterministic URI/source order. Document highlights include the
local heading declaration with kind `Write` (3), and usage blocks with kind
`Read` (2). A link to another opened document highlights its uses in the current
buffer, not the other document's heading.

Images, code examples, HTML attributes, network destinations and unreferenced
footnote bodies do not become uses. Links in published notes and nested
containers follow the existing publication walk. Existing definition and
completion requests retain their previous behavior.

## Synchronization and admission

Workspace references refuse with `ContentModified` (-32801) when a selected
search would require an unsynchronized open buffer. No stale revision is
silently omitted from an apparently complete result. Local highlights do not
require unrelated buffers to be synchronized, but resolving an unsynchronized
remote target still fails. Accepted changes and closes affect the next request;
there is no independently stale reference cache.

The existing 64-document, 2 MiB/document and 16 MiB/session limits remain in
force. The opt-in core inventory additionally caps each document at 16384
references, 8192 bytes per destination and 1 MiB total destination text. A
request examines at most 32768 references and returns at most 4096 distinct
locations with at most 1 MiB of aggregate URI text. Admission, analysis and
result-budget failures return `RequestFailed` (-32803), never a truncated success.
UTF-16 position validation and CR/LF handling use the existing line index.

## Shared core API

`DocumentLinkAnalysis::with_file_references(&document)` makes the same
source-owned inventory available to other authorized hosts. The ordinary
`analyze_document_links` API is unchanged. Additional file-addressed entries
have no `target_block_index` and no `finding`: they are unchecked, **not**
validated document-root links. Hosts must resolve them against their own source
set. Existing counters, internal findings and anchor IDs retain their meaning.

## Verification

```sh
cargo test --lib inventory_tests
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
cargo clippy --features lsp --bin fmd-lsp -- -D warnings
cargo fmt --check
```

Six core inventory regressions and fifteen provider regressions were added,
including a framed JSON-RPC session through the real server, Unicode/CRLF ranges,
URI ambiguity, nested and reference-style links, stale revisions and limits.
The authoring container had no Rust/Cargo toolchain: these Rust tests, Clippy and
rustfmt were not executed there. JSON fixture decoding, lexical checks and
whitespace checks are not a substitute for that pending Rust validation.
