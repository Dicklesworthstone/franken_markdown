# Reference-label rename

The native `fmd-lsp` supports `textDocument/prepareRename` and
`textDocument/rename` for document-local Markdown reference labels. Put the
caret in a definition's label, the second label of a full reference, or the
label of a collapsed/shortcut reference. Links and images participate.

For example, renaming `guide` to `manual` changes:

```markdown
[Read the guide][guide] [guide][] [guide] ![Cover][guide]

[guide]: <book.md#start> "Original title"
```

to:

```markdown
[Read the guide][manual] [guide][manual] [guide][manual] ![Cover][manual]

[manual]: <book.md#start> "Original title"
```

The visible link text, image alt text, destination and title stay the same.
Collapsed and shortcut references acquire an explicit suffix when the lookup
key changes. A spelling-only change with the same normalized key leaves those
forms intact. Full references and definitions receive the requested spelling.

## Parser-owned identity and locations

The public `parse::reference_label_spans` API runs the ordinary native parser
with optional source tracking. It returns accepted definitions and resolved
reference uses, their parser-normalized keys, source selections, and replacement
ranges. This reuses the same source maps as destination tracking; no separate
reference grammar or source-text search assigns edit authority. Ordinary parse
calls and their AST remain unchanged.

Keys use the parser's pinned Unicode full fold and whitespace normalization.
Labels containing escapes or entity-like text retain the parser's literal
identity rules. Source positions preserve UTF-8 byte offsets internally and
become exact UTF-16 ranges in LSP responses, including BOM, CRLF, astral text,
lists, quotes, tables, and note bodies.

Unused and shadowed definitions are included. All definitions and uses with the
selected key change together, preserving first-definition precedence. References
in dormant notes and accepted references inside image alt text participate too.
Speculative references from rejected outer links or exhausted inline work are
rolled back. Code, HTML, math, escaped lookalikes, explicit URL destinations and
unresolved literal references do not acquire rename selections.

Tracking retains an occurrence even when its transformed source has no single
exact range. For example, a multiline label spanning quote prefixes cannot be
represented as one label edit. If any occurrence of the selected key lacks an
exact range, the whole request fails instead of omitting that occurrence.

## Semantic and version checks

The new label must parse as exactly one reference label. Admission uses the
native parser, including its 999-source-scalar limit before Unicode folding.
The LSP additionally requires a nonempty single line without control characters
and at most 4096 UTF-8 bytes. A requested key already owned by another reference
group is refused.

The server applies the complete proposed edits to a temporary source copy and
requires its entire AST to equal the original AST. This protects visible text,
image alt text, destinations, titles, containers, and other content. It also
rejects a rename that would activate previously unresolved literal references
to the new key. This is a bounded synchronous validation, not an incremental
parser or frame-time guarantee.

Clients must advertise `workspace.workspaceEdit.documentChanges: true`.
Successful responses contain one `TextDocumentEdit` with the current source
version and exact client URI. A no-op returns empty `documentChanges`.
The server does not write files or modify its stored buffer; the client applies
the edits and sends the resulting document change normally.

Only the selected document's reference namespace is affected. Identically named
labels in other opened documents are independent. The selected buffer must be
synchronized. Stale buffers return `ContentModified`; malformed positions or
labels return `InvalidParams`; incomplete mappings, collisions, semantic changes
and exhausted budgets return `RequestFailed`.

## Scope and limits

| Limit | Bound |
| --- | --- |
| Source document | 2 MiB |
| Open source session after edits | 16 MiB |
| Reference occurrences examined | 8192 |
| Occurrences in the selected group | 4096 |
| Aggregate replacement bytes | 2 MiB |
| New label | 999 source scalars and 4096 UTF-8 bytes |

Go to definition from reference uses keeps its existing target-navigation
semantics; it resolves the destination rather than the label declaration.
Reference-label completion and reference-label search/highlights are separate
future work. [Footnote labels](LSP_FOOTNOTES.md) and
[heading titles](LSP_HEADING_RENAME.md) retain their own rename providers.

## Verification

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
cargo test --test parser_reference_labels
cargo clippy --features lsp --all-targets -- -D warnings
```

The focused tests check complete expected source edits and AST preservation,
all link/image forms, references inside image alt text, duplicate precedence,
unused definitions, Unicode identity, exact UTF-16 ranges, original line endings,
containers and notes, multiline refusal, invalid names, newly activated literal
references, budgets, current-version dispatch, and real stdio protocol behavior.
