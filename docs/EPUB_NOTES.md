# Notes in EPUB publications

Single-document and book EPUB exports give Markdown notes explicit publishing
and accessibility semantics. The shared HTML renderer emits these attributes
while it still knows which source nodes are references, definitions and return
links. Authored links, HTML classes and footnote-looking text cannot grant note
semantics to unrelated content.

## Reading-system and accessibility roles

| Element | EPUB semantics | Accessibility role |
| --- | --- | --- |
| Linked note citation | `epub:type="noteref"` | `role="doc-noteref"` |
| Trailing notes section | `epub:type="endnotes"` | `role="doc-endnotes"`, named `Notes` |
| Individual note | `epub:type="endnote"` | Native `li` semantics |
| Return link | `epub:type="backlink"` | `role="doc-backlink"`, named `Back to reference N` |

Notes appear at the end of their document or chapter, so they use endnote
semantics. The native ordered list is retained. `doc-endnote` is deprecated in
DPUB-ARIA 1.1, and `doc-footnote` must not be used inside a `doc-endnotes`
section. The EPUB namespace is declared on each chapter that actually emits
notes; chapters without notes retain their existing namespace and body bytes.

These attributes let supporting reading systems recognize note navigation and
provide their own note presentation. Popup behavior depends on the reading
system. EPUB semantics and ARIA roles have distinct purposes; neither this
feature nor its tests establish complete EPUB accessibility certification.

The mappings follow the W3C
[EPUB Structural Semantics Vocabulary](https://www.w3.org/TR/epub-ssv-11/#notes),
[DPUB-ARIA 1.1](https://www.w3.org/TR/dpub-aria-1.1/#doc-endnotes), and
[EPUB Type to ARIA Role Authoring Guide](https://www.w3.org/TR/epub-aria-authoring-11/).

## Source identity and link ownership

Numbering follows first-reference order. Repeated citations share one note;
references discovered inside notes are emitted too, including finite cycles.
The first definition wins, unreferenced definitions remain omitted, and
undefined references remain literal text. Every return link addresses the
first citation's anchor.

Generated note and return identifiers retain their familiar `fn-ID` and
`fnref-N` spellings when available. Before choosing alternatives, the renderer
reserves all emitted body/note heading identifiers and all legacy note targets.
If a heading occupies a note's usual identifier, a deterministic numeric suffix
keeps the generated citation and return links attached to the intended note.
This includes headings introduced by safe HTML lowering and notes whose names
already contain numeric suffixes. URI fragments encode literal percent signs
and Unicode exactly once.

An ordinary authored link keeps its authored target, including an ambiguous
heading/note alias. Generated note links carry their own identity. A citation
inside another link retains the outer link's destination and never creates a
nested anchor or falsely marks that outer link as a note reference.

Each book chapter owns its notes and identifiers. The same source label in two
chapters produces two independent local targets. The core never reads files or
fetches resources to resolve these links. The public export entry points and
options are unchanged, and ordinary HTML output keeps its existing markup.

## Verification

`tests/epub/notes.rs`, run through `epub_test`, decodes the actual archive with
the independent test inflater, checks member sizes and CRCs, scans XHTML
well-formedness, and resolves emitted reference/return fragments to unique
typed targets. Fixtures cover single documents and books, duplicate and nested
notes, cycles, link ownership, authored lookalikes, Unicode and escaped IDs,
heading collisions, custom CSS, chapter isolation and deterministic archives.
