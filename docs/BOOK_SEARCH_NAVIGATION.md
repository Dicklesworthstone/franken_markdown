# Bookmarkable offline book search

The existing exported-book search page now supports result bookmarks, submitted
query history, result-page restoration, and composition-aware live search. This
is part of the same embedded `src/book/site_search.js`; no new runtime module,
index format, dependency, storage, or network access is introduced.

## Reader workflow

Type to preview results without replacing the last submitted address. Press
**Search** or **Enter** to commit the successful query to the address bar. Each
successful search also exposes **Link to these results**, including its selected
result page. The URL carries the exact query (including scoped operators and
quoted phrases) in `q` and a one-based page in `page` when beyond the first page:

```text
~fmd-search.html?q=needle+path%3Aguide%2F+-obsolete&page=2
```

Previous/next result buttons save the displayed query and page. Back and Forward
restore both without creating another history entry. Opening a bookmarked result
page likewise restores it without adding history. The query and page are read
from the URL, not arbitrary `event.state`. History entries retain no book source,
index, results, or resource payloads.

Invalid queries never replace the last committed URL. Pending queries hide the
permalink so a previous answer cannot be mistaken for a new one. Escape clears
the query, results and search URL state; a blank explicit submission does the
same. A previous submitted search can then be recovered with Back. Live typing
remains a draft: its permalink follows its displayed result, while the address
bar stays at the last submitted query until submission or paging.

Bookmarks clamp valid out-of-range pages to the last retained result page. Zero,
negative, fractional, malformed or excessively long page values start at page
one. Navigation never invents pages for results beyond the existing 200-result
retention limit. Unrelated query parameters and the URL fragment are preserved.

## Embedded and offline boundaries

History and permalink URLs derive only from the actual document's file, HTTP or
HTTPS address. They remain on its original protocol, host and pathname. The
optional `mount(document, address)` prefill argument supplies a query/page only;
it cannot redirect the permalink or history destination. Book metadata and
untrusted source text cannot supply navigation destinations.

An opaque origin remains searchable but has no usable permalink. Contexts that
reject History API writes retain search and their explicit permalink without
throwing a rendering error. No browser policy is disabled or bypassed. Hosts that
change the document to an unrelated pathname are not adopted as the search page.

## Input and lifecycle

Composition events suspend live searching until the text is committed, coalescing
`compositionend` and its following input into one debounce. Enter/submission and
Escape do not interrupt an active composition; legacy IME Escape events carrying
key code 229 are also left alone. This is controller event handling, not a claim
of platform-wide native IME qualification.

Every pending result and error is fenced against newer input, generation,
composition, disposal, and even a silent input-value change without an input
event. Back/Forward cancels pending debounce and stale scans. Pager actions cannot
record a query different from the displayed answer. Disposal cancels timers and
removes the window history listener as well as element listeners; remounting
reuses the single permalink control.

## Verification

```sh
node --check src/book/site_search.js
node --test wasm/tests/book_site_search*.test.mjs
python wasm/tests/book_site_search_browser.py --browser /path/to/chromium
```

The 22 navigation regressions execute the production script with explicit DOM,
history and timer fixtures. They cover URL encoding, preserved parameters,
filter/Unicode queries, initial bookmarks, page admission, Back/Forward, draft
isolation, deduplication, denied history, opaque contexts, untrusted prefill
origins, stale scans/errors, silent edits, composition, disposal and remounting.
Together with the 12 untouched baseline tests and 20 scoped-query regressions,
all 54 Node tests pass. The scoped tests include 600 independent whole-section
membership comparisons across 120 generated books.

The offline Chromium runner uses the complete production script with real DOM
controls in an `about:blank` fixture, matching the search page's restrictive CSP.
It passed 25 checks, with zero page errors and zero observed requests. It exercises
scoped queries, late exclusions, pagination, real Escape, inert text, synthetic
composition events, errors, disposal/remounting and opaque-origin fallback. It
requires an already installed Chromium and Playwright and never downloads one.

Managed Chromium in the available environment refused file-URL navigation. The
runner does not bypass that policy and does not verify real file/HTTP history,
normal package startup, native operating-system IMEs, or native/WASM publication
generation. URL history logic is fixture-tested in Node, not represented as a
real-browser history pass. Rust/Cargo and the prescribed remote build tools were
unavailable; no native build, PDF, typography or font-rendering result is claimed.
