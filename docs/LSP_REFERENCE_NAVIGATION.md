# Reference-style link navigation

Go to definition now resolves full (`[text][id]`), collapsed (`[id][]`), and
shortcut (`[id]`) reference uses. Put the caret inside either the link text or
the reference label. The result addresses the resolved heading, emitted note,
or open target file, not the `[id]: ...` declaration itself.

This uses the same target resolver as explicit links: chapter-local heading
collisions, encoded fragments, empty document-root destinations, exact client
URIs, current synchronized target buffers, and ambiguous-target refusal are
unchanged. Uses inside lists, quotes, tables, definition lists, and referenced
notes work as described in `LSP_NESTED_NAVIGATION.md`.

## Parser-owned meaning

The server does not reimplement reference-definition collection or label
normalization. A bounded, escape-aware bracket scan only proposes a source use.
It temporarily replaces that use's reference suffix with a unique explicit
Link destination and invokes the real Markdown parser. The probe must occur
once in the selected top-level block and in the renderer's emitted reference
walk. Restoring the original Link's destination and reference-supplied title
must reproduce the entire original AST, with every other field unchanged.

Consequently forward definitions, duplicate-definition precedence, case and
whitespace normalization, entities, escaped labels, formatting and titles are
resolved by the existing parser. Definition titles may span lines even though
the selected reference use must be on one physical line. Images, code, math,
HTML attributes, undefined references and dormant note bodies remain inert.
If two different bracket interpretations both reproduce the original AST, the
request returns null rather than treating map order as source authority.

No buffer or file is modified. Fragment completion still applies only to
explicit URL destinations: it never overwrites a reference label with a heading
ID. Multiline reference uses, reference-definition editing/completion, direct
footnote-marker navigation and filesystem discovery remain outside this slice.

## Work limits and verification

The existing document and source-block limits remain (2 MiB and 64 KiB). A
reference use is capped at 8192 bytes. The bracket scan may propose at most
eight contexts before returning RequestFailed; each admitted candidate gets at
most one temporary parser pass. The unique marker is selected once, with at
most 32 attempts and whole-document checks. Analyzer admission failures and
excess work return explicit errors, not partial successful results.

Twelve new regressions cover full/collapsed/shortcut uses, parser precedence,
metadata restoration, containers, exclusions, source ambiguity, URI failures,
Unicode/CRLF, completion isolation, budgets, live cross-file revisions and probe
collisions. The original explicit-link and workspace suites remain enabled.

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
cargo test --test document_links_test
cargo clippy --features lsp --all-targets -- -D warnings
```

The authoring environment lacks a Rust toolchain. Lexical checks, source review
and `git diff --check` were performed; compilation and these executable tests
are not claimed to have passed.
