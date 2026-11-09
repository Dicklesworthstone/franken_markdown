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

The panel displays 50 findings at a time. **Select destination source** opens
the original chapter or include-only file and selects the parser-proven URL
token. Reference-style links can select their shared definition in another file.
When only the containing syntax block is proven, **Select enclosing block
source** labels that broader range explicitly. Findings spanning disjoint
source fragments have separate controls. **Open publishing chapter** remains
available, including for older packages or unavailable source metadata.
Destinations are displayed as inert text, never turned into arbitrary clickable
URLs, and the checker never searches for equal-looking text to invent a range.

**Prepare link report** creates a separate `book-links.json` download. Only click
Download to save it. The report includes chapter paths, destination strings and
available original-source locations; these may contain sensitive information
and should be reviewed before sharing.
Source bodies, images, fonts, and unknown worker-response fields are not retained
in the report. Summary totals are recomputed from the admitted chapter results.

The check owns its own cancellable worker. Cancelling it does not cancel an
independent publication, preview, or source-inspection job. Edits, source-role
changes, imports, recovery, composition and page suspension retire stale checks
and report downloads. Programmatic editor changes are checked again before
accepting a response, navigating source, or activating a download. Checks are
manual and do not rewrite source, auto-repair links, or gate publication.
Original UTF-8 byte offsets are checked against the captured source set and
converted to UTF-16 in one scan per used file. The editor applies its existing
CRLF normalization. Focus or selection callbacks that change the source cannot
attach an old report to a new editor checkpoint.

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
// The base report remains capped at 4 MiB; source metadata has a separate
// 2 MiB allowance, validated independently at the worker and report boundaries.
const checker = createBookWorker({ maxOutputBytes: 6 * 1024 * 1024 });
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
let located = renderer.validate_links_with_sources()?;
let report = located.report();
let first_original_location = located.source_location(0, 0);
let json = located.to_json();
```

Existing parsed `Book` values can call
`franken_markdown::book::validation::check_book_links(&book)`. The checker is
pure, synchronous and dependency-free. It does not mutate the AST. Expand the
source first when using a parse-only constructor.

`BookRenderer::validate_links()` still returns the original `LinkReport` and
its original JSON. `LinkFinding`, `ChapterLinks`, and `LinkReport` keep their
public fields and support the same Rust struct literals. The additive
`LocatedLinkReport` wrapper supplies `report()`, `source_location(chapter,
finding)`, and cached `to_json()`. The existing browser `FmdBook.validateLinks`,
native `fmd book --check-links --json`, and MCP `fmd.verify_book` return the
enriched report. An AST-only `Book` has no invented source attribution.

### Original-source precision

Each enriched finding adds `source`, while retaining its original code,
destination, message, ordering, counters, schema, and scope:

```json
{
  "status": "mapped",
  "precision": "destination",
  "ranges": [
    {"path": "parts/shared.md", "start": 12, "end": 29, "kind": "rewritten"}
  ]
}
```

`precision: "destination"` identifies the URL token that the parser bound to
that specific emitted link. Inline links select their own destination. Full,
collapsed, and shortcut reference links select the winning definition's URL,
which may be in a different included file from the citation. Supported HTML
anchors select their actual attribute value. Equal URL spellings, images,
code examples, abandoned speculative links, shadowed definitions, and unused
footnotes do not supply ownership for another reference.

`precision: "block"` identifies only the enclosing top-level syntax block when
exact token ownership is unavailable. A block may contain several links or
cross multiple included files. Consumers must label it as an enclosing block,
not a precise link selection. `kind` retains the original-source mapper's
`exact`, `rewritten`, and `generated` meanings: a rewritten URL selects its
complete original token, and a generated separator selects a zero-width source
boundary. Disjoint source ranges remain separate.

Missing or exhausted provenance is explicit:
`{"status":"unavailable","reason":"provenance_limit"}`. Older generated
packages can omit `source` entirely. Neither case invalidates the base link
findings. These ranges authorize navigation only; they are not a replacement
token, a repair suggestion, or permission to edit every reference sharing a
definition.

Token ownership is captured in the same parse that produces the book AST and
parser diagnostics. Transient parser allocation identities are converted into
deterministic structural ordinals before chapter URL canonicalization. Retained
books keep bounded source metadata without retaining source bodies, reparsing
on checks, or searching for matching text. Workspace updates refresh maps even
when moving a selected snippet leaves expanded text unchanged and reparses zero
chapters. Source and attribution commit together, and a refused update preserves
the previous revision and locations.

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
and 4 MiB of base JSON. Optional source metadata has its own 2 MiB allowance,
so enriched reports can reach 6 MiB. Exhausting the optional allowance emits an
explicit unavailable location and cannot reject a previously admitted base
report. Include expansion has its existing separate whole-book work
and byte limits. These are logical admission budgets, not a bound on every
transient allocation. Exceeding any enforced check budget is an error, never
silent truncation.

The source sidecar admits at most 65,536 block/reference entries across the
whole retained book, in addition to the shared origin map's 65,536 fragments
and 256 KiB of logical paths. The transient parser capture is independently
bounded to 65,536 allocation keys per chapter. One finding maps at most 64
original ranges; one report admits at most 16,384 ranges and 256 KiB of repeated
source-path text. Optional limits degrade attribution explicitly, preserving
every finding and the original validator's admission rules.

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
