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

## Authoring profiles

Set the editor client's `initializationOptions.profile` to the same authoring
profile used for rendering or publication:

```json
{
  "initializationOptions": {
    "profile": "commonmark-gfm"
  }
}
```

The accepted values are exactly `commonmark-gfm` and `gfm-plus`. CommonMark/GFM
keeps definition-list-shaped text in its original paragraph, allowing inline
links, emphasis and code spans to cross `: definition` lines. GFM-Plus keeps
the extended definition-list grammar. Omitting the profile preserves the
server's existing extended grammar; omitted or `null` initialization options
also preserve that default. Unrelated fields in an options object are ignored.
An explicit invalid profile, including `null`, an alias or a non-string value,
returns `InvalidParams` without initializing the server, so the client can retry.

The profile is immutable for the session and applies to every opened buffer,
including cross-document targets. Diagnostics, outlines, folds, selections,
workspace symbols, completion, document links, hover, definition/reference navigation and all rename
providers use that grammar. The same profile also controls temporary parser
probes and source-token inventories before an edit is offered. For example,
link or footnote text inside a code span crossing a colon line stays literal in
CommonMark/GFM, while a real link whose label crosses that line remains
navigable and editable. Reconnect the server to change profiles; configuration
notifications do not change an existing session's grammar or cached diagnostics.

## Outlines, folding, and selection expansion

`textDocument/documentSymbol` returns section outlines from the shared parser
and source map, including setext headings and the source map's canonical IDs.
Clients advertising hierarchical symbols receive nested sections with separate
section and heading-selection ranges. Other clients receive flat
`SymbolInformation` locations. Empty headings have a nonempty display label.

Heading IDs use the publication allocator, including literal suffix collisions
and IDs already consumed by headings inside lists or quotes. Footnote headings
allocate after the body in first-reference order; an unreferenced definition
does not consume a body ID. This identity parity does not invent precise source
ranges for nested headings.

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
headings. This server does not discover files on disk or advertise formatting. Same-document fragment completion and definition navigation
are available with the explicit scope below. Navigation requests reparse the current
synchronized buffer on demand; requests against an unsynchronized buffer return
`ContentModified` rather than locations from stale text.

## Heading-link completion and go-to-definition

Filename completion is available inside explicit link destinations, including
empty `[next]()` and unfinished `[next](cha` links. It offers synchronized
documents already opened by the editor and folders containing those documents;
typing `/` continues through those folders. Relative, parent-relative, rooted
and explicit `file:` paths use the navigation provider's existing URI rules.
Edits preserve queries, fragments, angle brackets and titles, and percent-encode
filenames so spaces, Unicode and Markdown punctuation remain valid URLs. See
[file path completion](LSP_PATH_COMPLETION.md) for ownership and scope.

`textDocument/completion` offers publication-correct heading IDs while the caret
is inside an explicit same-document fragment, such as `[label](#par)` or the
unfinished `[label](#par`. Its fragment trigger is `#`. Text edits replace the
whole fragment without disturbing its query, angle delimiters or title. Results
are capped at 256 and set `isIncomplete` when more matches remain.

The same completion route now offers document-local footnote labels in
`[^prefix]` and reference definitions in `[visible text][prefix]`, including
unfinished markers and image references. The additional triggers are `[` and
`^`; parser ownership prevents edits in code or other literal contexts. See
[label completion](LSP_LABEL_COMPLETION.md) for exact edit ranges and limits.

`textDocument/definition` follows an existing fragment destination to its actual
source block. Missing or ambiguous targets return no location. Forward and
nested headings, percent-encoded fragments, and emitted footnote anchors use
the same shared analyzer as diagnostics and book publication. Empty fragments
address the document start; nested targets retain their enclosing block range.

Origin tokens are supported in top-level paragraphs and headings, not inside
list/quote/table/note containers or reference-style links. Code, image URLs,
HTML attributes, link titles, external/file destinations and bare hashes do not
become navigation requests. A real-parser probe validates every candidate;
definition additionally requires exact AST equality after restoring its original
destination. No guessed inline source map or independent Markdown grammar is
used. See `LSP_LINK_NAVIGATION.md` for the algorithm, budgets and exclusions.

## Link and footnote previews

`textDocument/hover` previews the actual source behind a supported inline or
reference-style link, or a footnote marker. A heading target includes its
section content through the next heading of the same or higher level; child
sections remain visible. Footnotes include their definition and continuation
paragraphs. A link to an opened file without a fragment previews its leading
content. This lets an author inspect the destination without leaving the
current paragraph.

Hover reuses go-to-definition's parser ownership, profile, path resolution and
ambiguity rules. It reads the current synchronized target buffer, including
unsaved edits; unopened or closed files and external URLs have no preview.
Unknown targets and literal lookalikes in code also return no hover.
Unsynchronized source or target buffers return `ContentModified` until a
full-text replacement restores synchronization. No file or URL is opened.

Hover honors the client's first supported content-format preference. Plaintext
clients receive authored source directly; Markdown clients receive a fenced
code block whose delimiter is longer than every backtick run in the source.
Clients without a content-format capability receive the legacy structured
MarkedString code-block form. Empty or unsupported-only format lists disable
the provider. Every supported representation keeps source links, images, code
and HTML inert.

The shared parser supplies primary block spans: nested targets retain their
enclosing source container. Previews contain at most 48 source lines and 8 KiB
of UTF-8 text including an explicit truncation marker, with no split Unicode
scalars. Markdown fence overhead has a separate 24 KiB total-text ceiling.
The optional hover range is omitted because a target document's coordinates
cannot identify the origin link in a different document.

## Clickable file, image and web destinations

`textDocument/documentLink` makes explicit Markdown link/image destinations and
reference-definition URLs clickable, including targets that the editor has not
opened. The server advertises `documentLinkProvider` with
`resolveProvider: false`; every returned link already carries its absolute
target URI.

Relative and rooted file destinations resolve against the source document's
`file:` URI. Explicit `file:` URLs also work from untitled buffers. Windows
drive roots, UNC authorities, Unicode, spaces, escaped Markdown punctuation,
queries and fragments retain their identities. Percent escapes are decoded
once when resolving file components and encoded when constructing the final
URI, so `literal%252F.md` continues to name a file containing the literal text
`%2F`. The provider does not append `.md`, choose a directory index, inspect a
file or test its existence. The client opens a target only when the user
activates the link. HTTP(S) and `mailto:` links are returned directly with
URI-safe encoding; command, executable, data and unknown schemes are omitted.

Ranges come from the selected grammar's destination inventory, including links
inside lists, quotes, tables and notes. They address the original UTF-16 source
and exclude an angle destination's surrounding `<` and `>`. Markdown entities
and escapes remain in the clickable source while the URI uses their decoded
meaning. Accepted unused or shadowed reference definitions retain their own
clickable URL. Reference-use labels, bare/angle autolinks and raw HTML attributes
are outside this provider's scope; code, titles and abandoned link parses never
acquire clickable ranges. Same-document fragments continue to use the existing
heading/footnote go-to-definition provider.

Requests use the current synchronized buffer. A rejected edit returns
`ContentModified` until the client supplies a full-text replacement; closing a
document removes its link inventory. Results are limited to 4,096 links and
8,192 bytes per decoded or encoded target, and 1 MiB for the serialized result.
Exceeding a budget returns `RequestFailed`, rather than silently publishing a
partial inventory. Malformed URI escapes, encoded file separators, control characters and ambiguous path
spellings do not produce a target. No filesystem, network, renderer or font
dependency is introduced. Ordered destination ranges are converted to UTF-16 in
one pass, so many links on one long source line do not repeatedly rescan it.

## File and folder rename edits

Clients supporting versioned workspace edits and file-operation requests can
ask `workspace/willRenameFiles` to maintain Markdown links and image paths in
opened buffers. The server plans simultaneous moves, updates incoming links,
rebases outgoing relative destinations and returns edits against the old URIs
before the client moves files. See [file rename](LSP_FILE_RENAME.md) for parser
ownership, alias handling, unopened-file scope and explicit include refusals.

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
