# Footnote and reference-label completion

`fmd-lsp` completes document-local footnote labels and explicit reference-link
labels in opened Markdown buffers. This extends the existing heading-fragment
completion provider. Its triggers are `#`, `[` and `^`; ordinary prose containing
those characters does not acquire completion edits.

With these definitions:

```markdown
[^research]: Supporting evidence.

[manual]: guide.md "User guide"
```

completion in `Claim [^re]` offers `[^research]`, and completion in
`[Read more][ma]` offers `manual`. Image reference suffixes such as
`![Diagram][im]` use the same reference-definition inventory. Definitions may
follow their uses. Footnotes in currently unreferenced note bodies and valid
definitions inside containers remain available while authoring.

## Edits and matching

The returned `textEdit` replaces the entire label inside the brackets, including
an existing suffix after a caret in the middle of the label. It preserves the
visible link text, image alt text, brackets, destination definitions, titles and
surrounding document. Ranges use the client's original UTF-16 coordinates,
including astral characters, BOMs and CRLF line endings.

Unfinished `[^re` and `[Read more][ma` markers are supported. The edit adds the
missing closing bracket. It consumes the unfinished word under the caret and
leaves following whitespace and prose intact. A closed reference label can
contain spaces; a multiline label is outside this completion contract.

Footnote labels match their exact, case-sensitive parser identities. Reference
labels use the parser's pinned Unicode case folding and whitespace rules, so
`STRAS` can offer the accepted spelling `Straße`. Reference definitions with
the same normalized identity are offered once, using the first accepted source
spelling. Footnotes are also deduplicated by their actual identity. Results
follow definition order and include `sortText` so clients can retain that order.

This provider completes explicit reference suffixes, including the empty suffix
in `[visible text][]`. It does not turn arbitrary `[prose]` into a shortcut
reference or change reference-definition names; those positions have different
authoring meanings. Existing fragment completion remains available inside
`[text](#heading)` and links to explicitly opened documents.

## Parser ownership and limits

A bounded bracket scan only proposes a source location. Footnotes reuse the
same exact source inventory as navigation and rename. Reference suffixes are
verified through the ordinary parser's source-label metadata. An unfinished
reference is tested using a temporary definition inserted after actual
frontmatter and before the document body, so an unclosed final code fence does
not swallow that definition. Existing label characters are retained during
these probes; code, math and HTML delimiters are not removed to manufacture a
match. Existing reference uses and destination tokens must survive, preventing
completion inside an enclosing link from silently replacing that outer link.

Code spans and fences, escaped markers, URL destinations, link titles, HTML
attributes, mathematics and frontmatter do not receive label edits. The real
parser determines nesting and image/reference ownership. No separate Markdown
grammar, filesystem lookup or network access is introduced.

The number of parser passes is constant with respect to the number of offered
definitions. The provider retains the 2 MiB source limit, caps reference
inventories at 8,192 tokens and probe text at 4 MiB, and offers at most 256 items.
The existing footnote inventory also retains its 8,192 lexical-candidate and
4,096 semantic-symbol limits. Labels are single-line and bounded to 4,096 UTF-8
bytes; actual parser limits, including 128-byte footnote IDs, still apply.
Truncated or unrepresentable matching suggestions set `isIncomplete`.

Requests use only the current synchronized document revision. Unsynchronized
buffers return `ContentModified`; invalid UTF-16 positions return
`InvalidParams`; exceeded work limits return `RequestFailed`. Completion never
changes a server buffer. Clients apply an accepted edit and send normal document
change notifications.

## Regression coverage

`src/bin/lsp/label_completion_tests.rs` covers exact edits, incomplete and
mid-label requests, Unicode identity, duplicate precedence, dormant/nested
definitions, literal exclusions, enclosing-link preservation, frontmatter,
unclosed final fences, deterministic truncation and dispatch to the existing
fragment provider. `tests/lsp_protocol_test.rs` exercises a real framed server
session, advertised triggers, exact UTF-16 edits and a live revision change.

```sh
cargo test --features lsp --bin fmd-lsp label_completion
cargo test --features lsp --test lsp_protocol_test real_label_completion
```
