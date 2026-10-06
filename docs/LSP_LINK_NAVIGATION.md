# Markdown link navigation

The native `fmd-lsp` exposes `textDocument/completion` for explicit fragment
destinations and `textDocument/definition` for explicit and reference-style
links. It never opens a URI, reads workspace files, loads fonts, renders PDF or
invokes an external Markdown parser.

Links between explicitly opened files are supported; see
[LSP_WORKSPACE_NAVIGATION.md](LSP_WORKSPACE_NAVIGATION.md) for file resolution and
[LSP_WORKSPACE_SYMBOLS.md](LSP_WORKSPACE_SYMBOLS.md) for ranked heading search.
[Nested origins](LSP_NESTED_NAVIGATION.md) and
[reference-style uses](LSP_REFERENCE_NAVIGATION.md) share the same target model.

## Completion

Place the caret in `[label](#inst)` and request completion, or type `#` to
trigger it. The unfinished `[label](#inst` and `[label](<#inst` are supported
when the destination ends the enclosing top-level block or container. Accepting
`#installation` midway through `#inst-old` replaces the complete fragment and
leaves no stale suffix.

Queries and titles are preserved: `[label](?view=read#inst "Guide")` changes only
`#inst`. Angle delimiters and container prefixes remain intact. Positions and
edits use UTF-16, including text after astral characters and CRLF line endings.
Reference labels are not fragment destinations and receive no completion edits.

The shared document analyzer supplies collision-correct heading IDs, including
headings inside containers and referenced notes. Ambiguous IDs are not suggested.
Prefix filtering is case-sensitive against emitted IDs; percent escapes are
decoded once. Incomplete percent escapes return an empty incomplete list so the
next character can retry. Results are sorted by canonical ID with bounded titles.

Completion never inserts closing delimiters into the user's buffer. A temporary
closure can validate unfinished syntax; the actual edit changes only the fragment.

## Definitions

An explicit destination, or the text/label of a single-line reference use such
as `[guide][install]`, can resolve to its published target. Forward references,
duplicate headings, encoded fragments, query fragments and emitted footnote IDs
follow book/HTML validation. Missing, invalid and ambiguous targets return null.
An empty fragment addresses the document's zero-width start location. Reference
uses resolve to that target, not the reference-definition declaration.

A top-level heading has its exact block range. A heading nested inside a quote,
list or referenced footnote retains its authoritative enclosing top-level source
range, not an invented fine-grained span. Results preserve exact client URIs.
Cross-document targets must already be open and synchronized.

## Parser-verified source locations

Lexical tokens grant no authority on their own. The server substitutes a unique
harmless destination in a temporary source copy and invokes the actual parser.
The resulting AST must contain that Link in the selected top-level block, with
all other blocks unchanged. Structural correspondence descends through quotes,
lists, tables, definition lists and note bodies. The renderer must also emit the
probed reference, excluding dormant notes before any cross-file lookup.

For an existing explicit link, restoring its destination must reproduce the
entire AST. For a reference-use probe, both the destination and the title supplied
by the reference definition are restored at the one matching Link. Siblings,
labels, container metadata and other titles are not relaxed. Code, images, math
and HTML attributes therefore cannot acquire authority from link-shaped text.
Probe text never enters stored buffers, diagnostics, response edits or files.

Direct footnote-marker navigation, escaped/multiline explicit URL tokens,
multiline reference uses and reference-definition editing are not implemented.

## Limits and verification

The server's 2-MiB document cap remains. Navigation admits at most 64 KiB in the
selected top-level source block and 8192 bytes in a candidate destination/use.
Probe selection is bounded to 32 attempts. Completion returns at most 256 items,
with IDs of at most 1024 bytes and titles of at most 128 Unicode scalars; excess
matches set `isIncomplete`. The core analyzer's admission limits remain in force.

Explicit URL navigation uses at most two temporary probe parses. Reference-use
navigation admits at most eight candidate contexts, with one parse per candidate.
These are bounded synchronous operations, not incremental parsing or frame-time
promises. An oversized block or refused analysis returns RequestFailed; malformed
coordinates return InvalidParams; unsynchronized buffers return ContentModified.
Out-of-scope syntax returns an empty result.

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
cargo test --test document_links_test
```

The suites cover parser exclusions, target collisions, forward/encoded targets,
partial destinations, exact edits, Unicode, nested origins, reference uses, notes,
work limits, dispatch and current-version fencing. Rust compilation and execution
remain unverified in the toolchain-less authoring environment. Lexical checks,
source review and whitespace checks are not substitutes for these tests.
