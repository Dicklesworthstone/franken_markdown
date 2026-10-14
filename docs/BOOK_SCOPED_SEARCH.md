# Scoped queries in offline book search

Exported HTML books now support chapter filters and section exclusions through
their existing **Search this book** page. The script is still embedded directly
by `src/book/site_search.rs`, so native HTML sites and HTML-site ZIPs use the same
implementation. No server, network connection, dependency, index-schema change,
or additional runtime module is required.

## Queries

```text
rust "memory safety" path:guide/ -obsolete
needle title:"User Guide" -path:archive/
path:reference/ -"old API"
"-obsolete" "path:guide/"
```

Ordinary words and quoted phrases are ANDed over a section and its chapter title,
as before. Individual phrases must occur inside one actual indexed entry or the
chapter title: separate paragraphs are not concatenated into invented phrases.

`path:` matches a substring of the original book-relative Markdown source path,
not the flattened HTML filename. `title:` matches the chapter title, not headings
or body text. Repeated filters are ANDed. Prefixing either filter with `-` excludes
matching chapters. Filter values can be quoted when they contain whitespace.

Unqualified `-word` and `-"quoted phrase"` exclude an entire section when they
occur in any of its indexed entries or its inherited chapter title. A matching
heading does not override an exclusion in a later paragraph, code block, table,
or footnote entry belonging to the same anchor. Separate anchors and chapters
remain independent. An exclusion in the chapter title rejects all its sections.

A positive `path:` or `title:` filter may be used without content words to browse
matching sections, including empty and heading-less chapters. Exclusions alone
are rejected with an actionable error instead of silently searching the whole
book. Empty ordinary phrases still return no results. Empty filter/exclusion
values and unclosed or improperly separated phrases are errors.

Quote the entire token to search for literal operator text: `"-obsolete"` or
`"path:guide/"`. Unknown prefixes such as `protocol:tcp` remain ordinary literal
text. All values use the existing case-insensitive, canonically equivalent
Unicode substring matching. There is no regular-expression, glob, URL-decoding,
accent-stripping or compatibility-folding interpretation. `path:.*` means the
literal characters `.*`, not every chapter.

The reader's accessible help describes these operators, and each result displays
its source path. Paths and excerpts remain text nodes; untrusted source text is
never interpreted as markup. Destination links still use the validated generated
page name and the original emitted anchor.

## Compatibility and bounds

The public JavaScript `parseQuery` helper retains its original term-only grammar
and array return value. The additive `parseSearchQuery` helper returns
`{ terms, excluded, filters }`; `search` and the mounted reader use that plan.
Existing unqualified positive queries retain membership, ranking, source offsets,
and deterministic tie order. There is no change to either search-index schema.

The 512-UTF-16-code-unit query ceiling and eight-unique-clause ceiling apply across
words, phrases, exclusions and filters together. Existing index, result-count and
snippet bounds remain. Metadata is admitted once per chapter using the same
bounded normalization scanner as body text. Rejected chapters do not decode their
body entries. Rejected sections do not decode later entries, but skipped rows
still count toward cooperative yields and cancellation checkpoints.

A positive match cannot end a scan while an unseen exclusion could still occur.
Scanning continues in bounded chunks to EOF or the first excluded term. No partial
answer is published on cancellation. Excluded terms never become snippet offsets
or affect scores for admitted results. Filter-only results provide an excerpt
without treating the filter's value as body text.

## Verification

```sh
node --check src/book/site_search.js
node --test wasm/tests/book_site_search.test.mjs \
  wasm/tests/book_site_search_filters.test.mjs
```

The new tests cover query admission and literal quoting, real source-path identity,
combined title/path predicates, filter-only browsing, late and cross-chunk
exclusions, exact section boundaries, canonical Unicode, retained scores and source
offsets, cancellation during text and skipped-row traversal, bounded normalization,
metadata caching, inert reader output and full match totals. An independent
whole-section oracle checks 120 generated books against five query plans each.
The unchanged baseline tests cover the original parser, ranking, pagination and
reader lifecycle. Node executes the actual production script; DOM interactions
use explicit minimal fixtures, not a native/WASM renderer or a real browser.
Native publication generation and the Rust build are not validated by these tests.
