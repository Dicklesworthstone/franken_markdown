# Expanded book navigation checks

The book publisher has a **Check expanded book links** panel. It runs the Rust
book engine on the selected chapters and include-only sources, then checks local
Markdown navigation against the expanded, parsed book. It is separate from the
original-source structure/accessibility inspection and does not create a PDF,
EPUB, or site just to examine links.

## Workbench workflow

Add and arrange chapters and shared sources, then run **Check expanded book
links** before publishing. Findings identify missing chapters, missing or
ambiguous anchors, invalid fragments, malformed/escaping local destinations, and
undefined footnote references that exist as such in a supplied AST. Missing
include files, cycles, or resource-limit failures stop the check with an error;
a partial set of results is not presented as clean.

The panel displays 50 findings at a time. **Open publishing chapter** opens the
chapter containing the reference in its expanded document. An included snippet
may have supplied the actual text: the checker does not invent an original-file
line number or highlight by searching for matching destination text. Destinations
are displayed as inert text, never turned into arbitrary clickable URLs.

**Prepare link report** creates a separate `book-links.json` download. Only click
Download to save it. The report includes chapter paths and destination strings;
these may contain sensitive information and should be reviewed before sharing.
Source bodies, images, fonts, and unknown worker-response fields are not retained
in the report. Summary totals are recomputed from the admitted chapter results.

The check owns its own cancellable worker. Cancelling it does not cancel an
independent publication, preview, or source-inspection job. Edits, source-role
changes, imports, recovery, composition and page suspension retire stale checks
and report downloads. Programmatic editor changes are checked again before
accepting a response, navigating source, or activating a download. Checks are
manual and do not rewrite source, auto-repair links, or gate publication.

## Browser API

Rebuild the matching Rust/WASM package using the repository's DSR build pipeline.
Older binaries without `FmdBook.validateLinks` produce an explicit rebuild error.
This source change is not a new npm release or an already regenerated binary.

```js
import { createBookWorker } from "@franken-suite/franken-markdown/book-worker";
import { parseBookLinkReport } from "@franken-suite/franken-markdown/book";

// Use canonical, normalized paths in reading order when admitting the report.
const files = [
  { path: "guide/start.md", source: "# Start\n\n{{#include ../parts/shared.md}}\n" },
  { path: "end.md", source: "# End\n" }
];
const includeSources = [
  { path: "parts/shared.md", source: "[Next](/end.md#missing)\n" }
];
const checker = createBookWorker({ maxOutputBytes: 4 * 1024 * 1024 });
try {
  const output = await checker.render(files, "links", { includeSources });
  const report = parseBookLinkReport(output.bytes, files.map(file => file.path));
  console.log(report.summary, report.chapters);
} finally {
  checker.dispose();
}
```

`checkBookLinks(files, options)` is the one-shot, main-thread equivalent; it
always disposes its owned book. A retained `createBook(...)` session offers
`session.validateLinks()` without reparsing or rendering pages. `BookLinksOutput`
contains JSON bytes, MIME type, source length, a Blob helper, and (on the direct
facade) a filename helper. Worker results omit the filename helper.

The one-shot and worker check paths use only `includeSources` and
`expandIncludes`. They do not read image, font, CSS or presentation-option getters
or transfer those bytes. Expansion follows the existing Rust rules. Setting
`expandIncludes: false` intentionally checks literal/already-expanded chapters;
nonempty include sources then remain invalid, as on other book routes.

## Rust API

```rust
use franken_markdown::book::{BookInput, BookRenderer};

let chapters = vec![BookInput {
    path: "start.md".into(),
    source: "# Start\n\n[Broken](#missing)\n".into(),
}];
let renderer = BookRenderer::from_sources(&chapters, &[])?;
let report = renderer.validate_links()?;
let json = report.to_json()?;
```

Existing parsed `Book` values can call
`franken_markdown::book::validation::check_book_links(&book)`. The checker is
pure, synchronous and dependency-free. It does not mutate the AST. Expand the
source first when using a parse-only constructor.

All chapters' anchors are collected before checking references. Known Markdown
paths use the existing chapter-relative/root-relative URL resolver, queries are
preserved, and fragments are percent-decoded once as UTF-8. Heading IDs reuse the
renderer-aligned search-index algorithm, including duplicate-heading suffixes.
Referenced notes follow first-reference emission order; nested citations and
cycles emit each note once, the first duplicate definition wins, and unused note
bodies are excluded. Heading/note ID collisions are reported when targeted.

## Exact scope and budgets

The report schema is `fmd-book-link-report-v1`, with scope
`expanded-html-navigation`. Each chapter has `checked`, `external`, `unchecked`,
and `findings`. `checked` includes local link and parsed footnote-reference
checks, including failures. Scheme-bearing and protocol-relative URLs are counted
as `external` but never fetched. Other local resources, such as downloads and
unknown non-Markdown pages, are `unchecked`, not presumed present.

This is **safe HTML navigation checking**, not remote URL liveness, URL security
certification, image availability, raw-HTML ID interpretation, typography, tagged
PDF compliance, or final PDF/EPUB destination validation. Do not treat zero
findings as a verdict that every publication property passed. Raw undefined
footnote syntax that the parser converts to literal text is not a parsed
footnote-reference check.

Validation rejects invalid books and output names, more than 4096 chapters,
250000 AST nodes, depth above 128, or 64 MiB of AST text. Reports are bounded to
4096 findings, 8192 bytes per destination, 256 KiB of retained destination text,
and 4 MiB of JSON. Include expansion has its existing separate whole-book work
and byte limits. These are logical admission budgets, not a bound on every
transient allocation. Exceeding any enforced check budget is an error, never
silent truncation.

## Verification

```sh
node --test wasm/tests/book_links.test.mjs wasm/tests/book_link_controls.test.mjs
tsc --noEmit --strict --target ES2022 --module ES2022 \
  --moduleResolution bundler --lib ES2022,DOM wasm/tests/book_links_types.ts
cargo test book::validation
```

The 29 Node tests exercise the production facade, worker protocol, report reader,
workbench controls and panel, with explicit Rust-class, editor, DOM and endpoint
doubles. One test checks production-entry wiring statically. File-free Blob and
object-URL behavior uses Node APIs. Strict TypeScript checks cover retained,
one-shot and worker call signatures and readonly report types.

Twelve Rust regressions were added for path/anchor parity, notes, cycles,
containers, include expansion, serialization and budgets. They were not executed
in the implementation environment because no Rust toolchain was available.
Generated WASM, native browser module workers and rendered HTML/PDF/EPUB output
remain separate DSR-managed build/parity and acceptance checks.
