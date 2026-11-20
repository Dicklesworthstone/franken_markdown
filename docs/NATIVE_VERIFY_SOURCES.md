# Verify the document that includes produce

`fmd verify` prepares file inputs with the same bounded include expander used
by native rendering. A document containing `{{#include parts/body.md}}` is
checked with that fragment present: included headings resolve links, included
parser and layout problems appear in the report, and the text layer contains
the actual composed prose.

```sh
fmd verify report.md --json
fmd verify manual.md --profile commonmark-gfm --json
fmd verify glossary.md --profile gfm-plus --a11y --json
fmd verify manual.md --max-input-bytes 1048576 --json
fmd verify manual.md --links --links-cache link-results.jsonl --json
```

## Source semantics and limits

Nested paths resolve relative to the including file. Line and named-snippet
selectors, source-relative image/link destinations, code-literal protection,
cycle detection and sandbox checks use the existing rendering implementation.
The top input's directory is the allowed source root. Symlinks cannot authorize
an include outside that root, and only regular UTF-8 files are accepted.

`--max-input-bytes` bounds each source read and the expanded document; its
default is 64 MiB. The complete expansion also retains the existing 64 MiB
ceiling, 16-level depth limit and 4096-resolution limit. Missing sources,
invalid UTF-8, invalid selectors, cycles and exceeded budgets fail with input
exit code 66 before a report or link cache is written. The verifier never
reports a partial source graph as clean.

Standard input has no source directory. An active include in `fmd verify -`
fails with usage exit code 64, matching ordinary rendering. Directive examples
inside fenced, indented or inline code remain literal and need no filesystem
access. `--profile` is applied before include-code classification and parsing;
omission retains the existing default grammar. Invalid profiles fail before
the input is read.

## Report and diagnostic coordinates

The existing version-1 JSON schema, verdict codes and digest algorithm are
unchanged. Plain documents with no selected profile retain their existing
reports. A composed document's report describes its expanded text and fixed
PDF baseline. Parser findings explicitly say `expanded source bytes`, and
human caret locations label the virtual source as `file.md (expanded includes)`.
These are expanded-buffer positions, not invented line positions inside the
original primary file or an included file.

Verification still ignores machine-specific rendering config and uses default
PDF styling, with the explicitly selected authoring profile. It does not
automatically load image/font files, download image resources, reproduce
arbitrary render flags, or certify PDF/A or PDF/UA conformance. Those limits
are distinct from expanding the Markdown source correctly.

## Optional external links

`--links` remains opt-in. Its existing timeout, cache and HTTP checking policy
are unchanged. Collection now follows the PDF preparation and complete AST,
including table headers/cells, definition terms/descriptions, note bodies and
the supported HTML-anchor subset. Repeated URLs are checked once. Code
examples, image resources, unsupported HTML attributes and unused reference
definitions do not authorize network requests. Unreferenced note bodies are
included because the PDF baseline publishes them as trailing notes.

The cache is an output. Before rendering or making any external request, the
verifier checks its path against the main input and every consulted include,
including symlink and hard-link aliases. A collision fails with usage exit
code 64. No cache is read or written when `--links` is omitted.

## Regression coverage

The native CLI tests compare reports with independently written expanded
Markdown, exercise nested and selected content, source-relative resources,
profile-specific code protection, graph failures, byte budgets and cache/source
aliases. External-link coverage uses prepopulated cache records, so the suite
does not need network access.

```sh
cargo test --locked --features lsp --test cli_verify_sources \
  --test cli_verify_cache_safety_test --test cli_verify_parser_limits
```
