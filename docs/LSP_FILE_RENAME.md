# Maintain Markdown links when files move

`fmd-lsp` supports `workspace/willRenameFiles` for editor-initiated file and
folder renames. It updates Markdown destinations in the documents the editor
has explicitly opened, including incoming chapter links and relative links or
images inside a moved chapter. It returns edits for the client to apply; it
does not move files, write documents, or change its live buffers.

The client must advertise both `workspace.fileOperations.willRename: true`
and `workspace.workspaceEdit.documentChanges: true`. The server then registers
file operations with the `file` scheme and `**/*` pattern. The pattern includes
folders and image/resource files because their moves can affect Markdown too.
Requests without both negotiated capabilities return `MethodNotFound`.

The immutable `initializationOptions.profile` selected for the LSP session also
governs file renames. `commonmark-gfm` and `gfm-plus` apply consistently to
source parsing, destination ownership, include code exclusions, recursively
selected fragments, and both sides of the final semantic proof. Omitting the
profile retains the historical extended grammar. For example, an include
inside a multiline code span that is valid in CommonMark remains literal when
that profile is selected; another grammar cannot make it an active source read
during a rename.

## Before a move

For an open `docs/start.md` containing:

```markdown
[Chapter](chapter.md#installation)
![Diagram](images/diagram.png)
```

renaming `docs/chapter.md` to `manual/guide.md` changes the first destination
to `../manual/guide.md#installation`. Moving `docs/start.md` instead rebases
its relative destinations so they continue to identify the original targets.
Links within a folder moved as a unit can keep their existing relative paths.

The request contains all moves in one `files` array of `oldUri`/`newUri`
pairs. A folder event names only the folder; the server maps descendants using
the same prefix substitution. The mapping is simultaneous: swaps and rename
chains are not applied repeatedly or in request order.

The response is a `WorkspaceEdit` containing versioned `TextDocumentEdit`
entries. They address the **old opened URIs and current document versions**,
because the client applies these edits before performing the file operation.
The server returns `null` when no source edit is needed. It does not
optimistically relocate open buffers after returning a plan; the client remains
responsible for normal change, close and open notifications.

This ordering and the folder-event shape follow the
[primary LSP 3.17 file-rename specification](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.17/workspace/willRenameFiles.md).

## What gets updated

The normal Markdown parser supplies exact source tokens for explicit link and
image destinations and accepted reference definitions, including unused and
shadowed definitions. Full, collapsed and shortcut references continue to use
their definitions. Nested list, quote, table, definition-list and footnote
destinations use the same parser ownership.

A bounded bulk probe identifies whether each token supplies links, images, or
both, then restores and compares the complete original AST. Chapter links use
the same extension and directory-index aliases as editor navigation. Images
keep exact resource identities: an image named `guide` does not become
`guide.md` merely because that chapter is open. A shared or unused reference
definition receives an edit only when chapter-link and resource interpretations
agree.

Every resulting document is parsed again and compared with the original AST
with only the intended link/image destination values changed. Visible link
text, image alt text, titles, block structure and other inline content must
survive. Missing or noncontiguous source ownership cannot produce a partial
semantic rename.

Ordinary query strings and fragments retain their source spelling, and their
decoded values are always preserved. When an existing token uses character
references or Markdown escapes, the replacement may normalize that spelling
while preserving the exact decoded query and fragment. Angle delimiters,
titles, original line endings and text outside the destination token remain
unchanged. Edit ranges use the original UTF-16 positions, including BOMs,
astral characters and CRLF.

Relative, root-relative and explicit file destinations use the existing
logical URI decoder. New path components are percent-encoded once. Moves
between authorities or Windows drive roots use explicit `file:` URIs when a
relative path cannot preserve the target. Same-document fragments and
query-only links remain local to their moved document.
Root-relative paths inherit a Windows source drive unless they explicitly
name another drive; moving the source across drives cannot silently retarget
those resources.

## Scope and refusals

Only text supplied through `didOpen` participates. The server does not scan
the workspace, read unopened files, check filesystem existence, or discover
incoming links from unopened chapters. An exact destination to an unopened
renamed file can still be updated from the explicit move map, and relative
resources in an opened moved chapter can be rebased without loading them.
An affected extension/index alias whose actual target is not open is refused
instead of guessing which file exists.
Incoming endpoints are considered even when the renamed source is closed:
an existing open target is spelled explicitly if an arriving file or folder
could capture its alias. The server does not invent a pre-move resolution or
assume that an unopened rename endpoint is a file rather than a directory.

All open buffers must be synchronized. URI aliases for multiple opened
revisions, ambiguous directory indexes, overlapping source/destination move
prefixes and collisions between known target identities refuse the whole
request. The client remains responsible for filesystem permissions, overwrite
policy and collisions not visible in the supplied documents or move map.

Code examples, escaped markers, mathematics, HTML attribute text, external URLs
and protocol-relative URLs are not rewritten. Raw HTML resource attributes are
outside this Markdown-destination contract. A file autolink without an editable
parser-owned destination token refuses the request if its semantic target
would change.

## Transcluded documents

File and folder moves also maintain active `{{#include ...}}` paths when every
source consulted by expansion is explicitly open and synchronized. For example,
moving `parts/card.md` to `notes/card.md` changes
`{{#include parts/card.md:summary}}` to
`{{#include "notes/card.md":summary}}`. The selector, surrounding whitespace and
line ending remain byte-identical. Moving an included file also rebases its
outgoing Markdown destinations and nested include paths; simultaneous swaps and
folder moves use the same move map as ordinary links.

The shared transclusion engine supplies exact path tokens, code exclusions and
literal quoting. Include paths are host path text: `%`, `#`, `?`, spaces and
quoted colons are not decoded as URL syntax. Absolute paths, backslashes,
drive-prefixed paths, directory targets, and moves requiring a different URI
authority or drive root are refused. The editor retains host filesystem and
publication-root authorization; a logical edit grants no file permission.

Before returning any edits, the provider expands every active opened root
against the captured buffers, both before and after the proposed move. Parser
probes and exact origin maps put each Markdown destination into the same
post-move identity space; the complete expanded ASTs must agree. This matters
for selected snippets: a link inside a full-source code example can become
active when selected, and separately included text can become code in its
surrounding document. If maintaining one interpretation would change literal
text or leave another interpretation broken, the entire request is refused.

Malformed directives, missing/closed include sources, cycles, invalid selectors,
ambiguous source origins and expansion-budget failures also refuse the whole
request. An include exposed only by a selected fragment is not automatically
an editable token in its complete source. Code examples remain unchanged. This
is maintenance of the explicitly opened graph, without a workspace scan or
filesystem reads.

## Bounds and regression coverage

The existing 64-document, 2 MiB/document and 16 MiB/session limits apply.
Additional bounds are 128 simultaneous moves, 8 KiB per URI/destination,
8,192 source destination tokens, 32,768 semantic destinations with 1 MiB total
destination text, 4 MiB per probe, 4,096 returned edits and 2 MiB of replacement
text. Include tokens share the 8,192-token source allowance. Across all roots
and both comparison phases, include proofs allow at most 32 MiB expanded text,
32 MiB of complete source payloads copied by resolvers, 16,384 destination
tokens, and 65,536 semantic destinations with 2 MiB of destination text. Each
expanded root retains the 2 MiB document cap; the ordinary expander also enforces
its depth and resolution-count limits. Parser passes are fixed per source or
expanded root, independent of how many links need changes.
Resource refusals use `RequestFailed`; stale buffers use
`ContentModified`; malformed parameters use `InvalidParams`.

`src/bin/lsp/file_rename_tests.rs` covers single files, folders, simultaneous
swaps/chains, incoming/outgoing paths, images and shared references, URI aliases,
unopened targets, drive/authority changes, source ownership, Unicode edits,
literal exclusions, includes, collisions, limits and capability negotiation.
`tests/lsp_protocol_test.rs` includes a real framed-server session checking
old-URI/versioned edits, unchanged server buffers and a live revision change.
Its include session additionally checks closed-source refusal, updated selected
source content, and successful retry after the current root selector changes.

```sh
cargo test --features lsp --bin fmd-lsp file_rename
cargo test --features lsp --test lsp_protocol_test real_file_rename
```
