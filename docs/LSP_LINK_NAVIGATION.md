# Same-document fragment navigation

The native `fmd-lsp` exposes `textDocument/completion` and
`textDocument/definition` for explicit same-document fragment destinations.
It never opens a URI, reads a workspace, loads fonts, renders PDF or invokes
an external Markdown parser.

## Completion

Place the caret in `[label](#inst)` and request completion, or type `#` to
trigger it. The unfinished `[label](#inst` and `[label](<#inst` are supported
when the destination ends the enclosing block. A candidate completion replaces
the complete fragment, not just the prefix: accepting `#installation` midway
through `#inst-old` does not leave an unwanted `-old` suffix.

Queries and titles are preserved: `[label](?view=read#inst "Guide")` changes only
`#inst`. Angle delimiters are likewise left intact. Positions and edits use
UTF-16, including text after astral characters and CRLF line endings.

The shared document analyzer supplies collision-correct heading IDs, including
headings inside containers and referenced notes. Ambiguous IDs are never
suggested. Prefix filtering is case-sensitive against actual emitted IDs;
percent escapes are decoded once for filtering. An incomplete percent escape
returns an empty incomplete list so the next character can retry. Results are
sorted by canonical ID and include a bounded plain-text heading title.

Completion does not automatically insert closing delimiters in the user's
buffer. A temporary closure may be used solely to validate unfinished syntax;
the actual TextEdit only changes the fragment. The editor/user retains control
of the remaining Markdown.

## Definitions

Place the caret inside an existing fragment destination to jump to its source.
Forward references, duplicate headings, encoded fragments, query fragments and
emitted footnote IDs use the same resolution as book/HTML validation. Missing,
invalid and ambiguous targets return null, never an arbitrary first match.
An empty `#` fragment returns the document's zero-width start location.

A top-level heading has its exact heading block range. A heading nested inside
a quote/list or a referenced footnote has its authoritative enclosing top-level
source block range. This is deliberately not a claim of precise nested spans.
The returned URI is always the exact synchronized buffer URI from the request.

## Parser-verified source locations

A bounded lexical scan finds a possible single-line URL token after `](`,
optionally with an angle delimiter. That scan grants no authority by itself.
The server substitutes a unique harmless fragment into a temporary copy and
runs the actual first-party parser. The resulting AST must contain that probe
as a Link destination in the selected top-level block, with every other block
unchanged. Code, images, HTML attributes and titles therefore cannot pass merely
because they contain text that resembles a link.

Definition requests apply a stronger check: align the original/probed inline
structure, restore only the original destination, then require equality of the
entire AST. Only an existing parsed link at that token can acquire a destination.
Completion permits a newly formed Link because the user may still be typing it.
Probe text never enters stored buffers, diagnostics, response edits or files.

This currently supports origin tokens in top-level paragraphs and headings.
It does not guess origin locations inside lists, quotes, tables or note bodies;
it does not implement reference-style link navigation, direct footnote-marker
navigation, escaped URL delimiters, multiline URLs, or links to another file.
Those remain separate source-provenance/workspace integration work. Broad
semantic diagnostics still cover parsed links in nested containers.

## Limits and verification

The server's 2-MiB document cap remains. Navigation additionally admits at most
64 KiB in the selected source block and 8,192 bytes in a candidate destination.
Probe selection is bounded to 32 attempts. Completion returns at most 256 items,
with IDs of at most 1,024 bytes and titles of at most 128 Unicode scalars;
excess matches set `isIncomplete`. The core analyzer's existing admission limits
remain in force. An oversized block or refused analysis returns RequestFailed;
malformed coordinates return InvalidParams, and unsynchronized buffers return
ContentModified. Out-of-scope syntax returns an empty result.

At most two temporary probe parses follow the initial spanned parse. This is
bounded synchronous work, not incremental parsing or a hard frame-time promise.

```sh
cargo test --features lsp --bin fmd-lsp links::tests
cargo test --features lsp --bin fmd-lsp --test lsp_protocol_test
cargo test --test document_links_test
```

The fourteen Rust regressions cover parser exclusions, canonical collisions,
forward/encoded targets, partial destinations, precise fragment edits, Unicode,
container target ownership, note collisions, probe uniqueness, result/work
limits, protocol dispatch and current-version fencing. They were authored but
not executed in the toolchain-less authoring environment. An independent
CommonMark probe-context experiment is not a Rust build or a substitute for
these repository-parser tests.
