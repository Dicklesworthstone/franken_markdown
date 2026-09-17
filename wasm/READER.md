# Semantic reading, search, and navigation

`./flow-reader` consumes the existing flow engine's `readingOrder` pages. It adds
native selectable HTML, literal document search, a heading inventory, and fenced
locations for navigating the measured Canvas or original Markdown. It never
parses Markdown or shapes text again. Both synchronous and worker sessions work.

```js
import { readFlowDocument, FlowReaderView, sourceSpanToUtf16 }
  from "@franken-suite/franken-markdown/flow-reader";

const document = await readFlowDocument(session, { token: session.token });
const reader = new FlowReaderView(readingContainer);
reader.render(document);
const results = document.find("measured text", { asciiCaseInsensitive: true });
if (results.matches.length) {
  const match = results.matches[0];
  reader.selectMatch(match); // ordinary browser Copy now copies the selected text
  const location = document.locate(match.nodeIndex);
  viewport.scrollTop = location.bounds.y; // host owns scrolling and repainting
  // Use the ORIGINAL SOURCE matching document.token.revision, not reading text.
  const range = sourceSpanToUtf16(authoritativeSource, location.enclosingSourceSpan);
  // This selects an enclosing Markdown block, NOT the precise matched phrase.
  sourceEditor.setSelectionRange(range.start, range.end);
}
reader.dispose(); // session remains owned by the host
```

## Meaning and fidelity

Reading text is unsplit logical text from the native engine, so a search can cross
a visual line wrap or an inline style boundary without joining guessed glyph
fragments. Code whitespace is retained. Search is literal and non-overlapping,
within each semantic leaf; it does not span different paragraphs or table cells.
An empty query yields no results. Case-sensitive matching is the default. Optional
ASCII case-insensitivity folds only A-Z, preserving every UTF-16 offset; it is not
Unicode case folding, locale-sensitive search, normalization, or regular expressions.

Container transcripts summarize their children and are not rendered/searched a
second time. This prevents duplicated table content. The plain `document.text`
joins nonempty leaves with blank lines; it is a transcript, not Markdown export.
Headings retain their supplied levels. Code uses pre/code, quotes use blockquote,
and tables use real rows/cells/header cells. Standalone native table rows and list
items are grouped only when consecutive siblings share an enclosing source span.
The wire format lacks list ordering/nesting metadata, link ranges and inline
emphasis in the reading tree. The reader does not invent those details from pixel
indentation or parse them from source. It uses generic list roles and inert link
text. Image descriptions are text figures, not auto-loaded images.

The semantic surface is intentionally separate from the Canvas ink. Browser DOM
text is selectable and exposes real heading/table/list semantics; it is not a
promise of font or wrapping parity with the bundled Canvas shaper, a complete
accessibility conformance certification, or a rich-text editing surface.

## Revision and resource safety

All pages are collected under one captured source/layout token. A changed token,
malformed page, invalid structure, or exceeded budget rejects the whole snapshot.
No partial new tree is published. Root AND descendant nodes count toward the
10,000-node default; depth is at most 64 below a root and all retained transcripts
including container summaries total at most 1,048,576 UTF-16 units. Limits may only
be lowered. The additional flat transcript adds at most two separators per node;
DOM elements add bounded structural overhead, not exact browser memory accounting.
Search retains at most 1,000 matches and truthfully signals extra matches with
`truncated`; the query limit is 1,024 UTF-16 units. These are bounded full-document
snapshots, not a spatial index or constant-time large-document search.

Documents are bound to their actual session, not just numeric-looking tokens.
Search matches are genuine objects issued by that document; matches from another
session, forged matches and stale matches cannot select DOM text. After an edit,
reflow or image-dimension update, read a fresh snapshot before navigating bounds.
The last successfully published DOM may remain readable while a replacement is
pending or rejected, but stale navigation/search/selection methods refuse it.
Rendering the SAME snapshot is a no-op that preserves native browser selection.

AbortSignal cancels only the public reading wait, never the worker RPC. Late
rejections are observed. Hosts should serialize reading requests and bound their
own retry policy; this API does not kill workers or imply rollback. The native
wire/page limits and worker backpressure remain separate safeguards.

No innerHTML, href/src, remote URL, copied event attribute, or clipboard write is
used. All text becomes Text nodes. The host owns navigation, source validation,
container placement, CSS, and lifecycle. `clear`/`dispose` remove only the reader's
container children. Do not edit those children behind its back: selection checks
that the referenced Text node still belongs to the view and has unchanged text.

`sourceSpanToUtf16` converts UTF-8 byte boundaries to JavaScript indices without
splitting a scalar or silently replacing malformed Unicode. The host MUST use
source from the same revision. Spans are enclosing original Markdown blocks, not
exact inline offsets and not stable identities across edits.

## Tests

```sh
node --test wasm/flow_reading.test.mjs
python wasm/tests/run_flow_reader.py --chromium /usr/bin/chromium
```

The Node tests exercise the real admission/search/mapping implementation with
explicit native-wire/session doubles. Browser tests exercise actual semantic DOM,
inert hostile-looking text, native Range selection, revision fences and disposal
in Chromium without network requests. They do not prove generated Rust/WASM
integration or replace assistive-technology testing. Use the matching built flow
package to verify the complete native engine and worker path.

## Live preview controls

The assembled `demo/flow-canvas.html` includes a semantic reader, literal Find,
previous/next matches, a heading selector, and **Show enclosing source**. Search
matches use native browser selection for ordinary Copy. Heading/match navigation
also scrolls the measured Canvas to the corresponding block. The source button
selects the actual enclosing Markdown block after lossless UTF-8/UTF-16 conversion;
it does not replace source or claim exact inline reverse mapping.

The controller collects reading geometry and paints under the same captured
layout token. Scrolling/height-only changes reuse that snapshot, preserving the
DOM and browser selection. Reflow, source changes and image-dimension changes
require a fresh reading snapshot; late image/read races cannot mix old semantic
bounds with newer pixels. The optional `readDocument` callback is checked against
the actual session, not merely a coincident revision token.

Navigation also compares the current textarea value with the successfully applied
source. Even an edit that has not yet reached the animation-frame update makes
old source navigation fail closed. Rejected source edits retain the old readable
surface but pause navigation. Restart/disposal revoke prior documents and remove
control listeners. No page-wide keyboard shortcuts or clipboard permissions are
installed; Enter/Shift+Enter is handled only in the search field.

A reading limit/format failure disables the semantic controls for that layout,
not otherwise valid Canvas rendering. It is reported separately as `readingError`
and not retried on every scroll; a new source/layout can recover. The host remains
responsible for limiting worker I/O and for assistive-technology acceptance tests.

Additional checks:

```sh
node --test wasm/tests/flow_preview_reading.test.mjs wasm/flow_reader_package.test.mjs
tsc --noEmit --strict --target ES2022 --module NodeNext --moduleResolution NodeNext wasm/flow_reader_types_test.mts
```

The Chromium runner also exercises the actual demo HTML controls and controller,
using explicit native-session and Canvas doubles. It checks native selection,
heading navigation, unsubmitted-source fences, scroll reuse, reader-limit recovery,
and teardown. This remains separate from the complete generated-WASM demo gate.
