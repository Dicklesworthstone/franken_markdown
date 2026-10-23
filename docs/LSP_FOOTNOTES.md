# Footnote navigation and rename

The native `fmd-lsp` supports direct footnote markers such as `[^note]` and
`[^note]: definition` in explicitly opened Markdown buffers. It provides
definitions, references, document highlights, and versioned rename edits.
Footnote labels are local to one document.

Ordinary links such as `[read](#fn-note)` continue to use
[Markdown link navigation](LSP_LINK_NAVIGATION.md). Their targets are emitted
anchors. Direct footnote operations also cover unresolved references and
definitions in currently unreferenced note bodies.

## Operations

Place the caret anywhere within a complete `[^label]` marker. Returned ranges
cover only `label`, preserving the `[^`, `]`, and definition colon.

| Request | Footnote result |
| --- | --- |
| `textDocument/definition` | The first matching definition in parser traversal order, with an exact label range; `null` when no definition exists. |
| `textDocument/references` | Every matching marker reference in the current document. Set `context.includeDeclaration` to `true` to include matching definitions. The boolean is required. |
| `textDocument/documentHighlight` | Every matching reference and definition in the current document. References use `Read` (2); definitions use `Write` (3). |
| `textDocument/prepareRename` | The selected label range and its original spelling, after rename safety checks. |
| `textDocument/rename` | A versioned `WorkspaceEdit` replacing every matching reference and definition in the selected document. |

Repeated references retain separate ranges. Duplicate definitions navigate to
the first definition; rename updates every definition and reference with that
label. Unresolved references can be searched, highlighted, and renamed without
inventing a definition.

For example, renaming `note` to `source` in this buffer changes all three
footnote labels and leaves the code span untouched:

```markdown
First [^note], again [^note]. Literal: `[^note]`.

[^note]: Supporting text.
```

## Labels, containers, and positions

Labels use the actual Markdown parser's exact, case-sensitive spelling.
`[^Note]` and `[^note]` are different labels. Entity-like text, backslashes,
and Unicode are not independently normalized or decoded by the LSP provider;
precomposed and combining Unicode spellings remain distinct.

A label must be nonempty, at most 128 UTF-8 bytes, and accepted by the parser.
Whitespace, brackets, and ASCII control characters are invalid. Rename validates
the proposed label through the parser and then verifies its effect in the
original document, so a label valid by itself can still be refused if it changes
surrounding Markdown syntax.

Actual footnote nodes inside headings, paragraphs, quotes, lists, table cells,
definition lists, and nested note bodies participate. This includes currently
unreferenced note bodies. Footnote-shaped text inside code, escaped markers,
image alt text, URL destinations, link titles, raw HTML tokens or attributes,
or math does not become an
editable footnote.

Positions and ranges use LSP UTF-16 code units, including text after astral
characters. CRLF line endings are preserved. Invalid positions, including a
position inside a surrogate pair, are rejected. Exact client document URIs are
preserved in results.

## Client capability and edit application

Rename is enabled only when the client advertises support for versioned
`documentChanges` in its initialize capabilities:

```json
{
  "capabilities": {
    "workspace": {
      "workspaceEdit": {
        "documentChanges": true
      }
    }
  }
}
```

The server then advertises `renameProvider: {"prepareProvider": true}`.
Otherwise it advertises `renameProvider: false` and rejects rename requests with
`MethodNotFound`. Definitions, references, and highlights remain available.

A successful rename returns one `documentChanges` entry containing the selected
document's exact URI, current version, and label-only text edits. The client
applies those edits and reports the resulting text through normal LSP document
changes. The server does not modify its stored buffer while answering rename.
An unchanged label returns an empty edit list when the request otherwise passes
the safety checks.

Only the selected document's footnote namespace is renamed. Matching labels in
other opened documents remain independent. General heading rename and
reference-link label rename are outside this provider's scope.

## Rename safety

Rename refuses a proposed label already used by any other actual footnote in
the document, including an unresolved reference. It also refuses collisions
with emitted document anchors, or any edit whose reparsed document differs from
the expected change to footnote IDs alone.

Changing a footnote label changes its emitted `fn-label` anchor. The provider
checks ordinary Markdown links in all explicitly opened buffers, including
links in unreferenced note bodies. If a link can target the selected note's
anchor, prepare-rename and rename are refused; update those links before
renaming the footnote label. Encoded fragments, relative file links, and
normalized URI aliases participate in this check.

Ambiguous opened aliases are treated conservatively: a possible target is
enough to refuse the edit. Extensionless paths can therefore cause a refusal
when another opened direct-path document would otherwise take precedence in
ordinary navigation. The provider does not guess which revision or path the
client intends.

All opened buffers must be synchronized for rename, because any of them can
contain an ordinary link that needs protection. A rejected incremental change
requires a valid full-text resynchronization before refactoring. Read-only
footnote navigation depends only on the selected buffer's synchronization.

Only opened buffers participate. The server does not discover or inspect
unopened files, follow includes, or perform network access. It cannot inspect
inbound links in unopened documents.

## Bounded source validation

Lexical markers only propose source locations. The provider parses the original
document, then uses two bulk probe parses to identify actual footnote nodes and
prove complete correspondence with the original AST. This is a bounded number
of whole-document parses, rather than one parse for every marker. If the source
mapping cannot be proved complete, the request fails instead of returning a
partial inventory.

A small UTF-8-safe window around the caret determines whether the request could
concern a footnote before the global scan. Large quantities of footnote-shaped
literal text elsewhere do not consume the footnote budget for ordinary heading
or link navigation.

| Limit | Value |
| --- | --- |
| Selected document size | 2 MiB |
| Opened document count | 64 |
| Total opened source text | 16 MiB |
| Lexical marker candidates per selected document | 8,192 |
| Actual footnote reference and definition nodes | 4,096 |
| Expanded source per bulk probe | 4 MiB |
| Attempts to reserve an unused probe prefix | 32 |
| Ordinary links inspected during rename | 32,768 |
| Aggregate destination bytes inspected during rename | 1 MiB |
| One ordinary link destination | 8,192 bytes |

The renamed document must fit both the document limit and the session capacity
remaining after accounting for every other buffer. Edits that exactly consume
the remaining budget and edits that reduce the document size are allowed.

Malformed parameters return `InvalidParams` (-32602), unsynchronized required
buffers return `ContentModified` (-32801), and refused safety or work limits
return `RequestFailed` (-32803). A cursor position with no actual footnote
falls through to the existing navigation provider or returns `null` for rename.
A complete-looking marker whose source mapping exceeds a limit can produce an
explicit error instead.

## Regression coverage

Focused cases are in `src/bin/lsp/footnotes_tests.rs`. They cover exact and
Unicode labels, CRLF/UTF-16 positions, repeated references and duplicate definitions,
unresolved references, nested containers, literal exclusions, rename collisions,
opened URI aliases, session capacity, capability negotiation, synchronization,
and preservation of ordinary heading navigation.

Run the LSP unit and protocol suites with:

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
```
