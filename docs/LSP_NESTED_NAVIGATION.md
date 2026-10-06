# Navigation inside Markdown containers

The LSP now recognizes explicit link destinations inside quotes, lists (including
task items), table headers and cells, definition-list terms and definitions, and
referenced footnotes. The same definition and fragment-completion routes apply
to same-document and already-open cross-file targets.

A byte token is still only a candidate. The server inserts a unique destination
in a temporary source copy, parses it with the real Markdown parser, and requires
exactly one corresponding Link in the selected top-level block. The structural
walker now descends through every inline-bearing container. For an existing link,
restoring its original destination must reproduce the entire original AST,
including sibling items, cell boundaries, list metadata, labels and titles.

The renderer's reference walk must also see the probe. This excludes dormant
footnote bodies before any cross-file resolver is consulted, while allowing
transitively referenced footnotes. Probe uniqueness checks the whole AST, including
parser-decoded entity spellings, not just raw source text. Code, images, math and
HTML attributes do not gain authority from link-shaped text.

Completion edits remain exact UTF-16 ranges in the original buffer: no nested
source spans are fabricated and no container prefix is removed. An unfinished
explicit destination can be completed when it ends its enclosing top-level
container; closing delimiters are used only for the temporary parser probe.
Target locations still use their authoritative enclosing top-level source block.

The 2-MiB document, 64-KiB source-block, 8-KiB destination and 256-item completion
limits remain. No filesystem or network access, dependency, feature flag or LSP
capability negotiation changes are introduced.

`nested_links_tests.rs` adds ten regressions and the previous container-origin
exclusion test now checks the positive path. Run:

```sh
cargo test --features lsp --bin fmd-lsp
cargo test --features lsp --test lsp_protocol_test
```

Rust execution is not verified in the authoring environment (no Rust toolchain).
Source review, lexical delimiter validation and `git diff --check` are not a
substitute for compilation or protocol execution.
