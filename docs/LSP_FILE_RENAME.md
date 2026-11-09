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

Transclusion directives are owned by a separate expansion subsystem and are
not destination tokens. If any opened source contains an active or malformed
include directive, this provider refuses the request with an explicit
transclusion error. It uses the real expander with a refusing, no-I/O resolver
to make that determination; include examples inside literal code remain safe.
The provider does not claim whole-project link or include maintenance.

## Bounds and regression coverage

The existing 64-document, 2 MiB/document and 16 MiB/session limits apply.
Additional bounds are 128 simultaneous moves, 8 KiB per URI/destination,
8,192 source destination tokens, 32,768 semantic destinations with 1 MiB total
destination text, 4 MiB per probe, 4,096 returned edits and 2 MiB of replacement
text. Parser passes are fixed per source document, independent of how many
links need changes. Resource refusals use `RequestFailed`; stale buffers use
`ContentModified`; malformed parameters use `InvalidParams`.

`src/bin/lsp/file_rename_tests.rs` covers single files, folders, simultaneous
swaps/chains, incoming/outgoing paths, images and shared references, URI aliases,
unopened targets, drive/authority changes, source ownership, Unicode edits,
literal exclusions, includes, collisions, limits and capability negotiation.
`tests/lsp_protocol_test.rs` includes a real framed-server session checking
old-URI/versioned edits, unchanged server buffers and a live revision change.

```sh
cargo test --features lsp --bin fmd-lsp file_rename
cargo test --features lsp --test lsp_protocol_test real_file_rename
```
