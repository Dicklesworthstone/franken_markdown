# Linked tables of contents in SVG posters

SVG exports can include a native, keyboard-accessible outline of the document
body. Each row is drawn with the poster's bundled or supplied font outlines and
links to the heading's final position through the existing SVG named views. The
outline uses the shared theme, paragraph wrapping and exact painted link
rectangles. It adds no scripts, browser fonts, `foreignObject` or page numbers.

## Placement and depth

Place a marker in its own paragraph:

```markdown
[TOC]

# Installation

## Prerequisites

# Usage
```

`[[TOC]]` and the documented underscored form `[[_TOC_]]` work too. A marker can
specify a maximum heading level, such as `[TOC:2]` or `[[TOC:2]]`. It can appear
inside a list item or blockquote. Multiple markers render the same body outline
at their respective positions, using each marker's own depth.

Alternatively, set `toc: true` in the library or browser options, or pass
`--toc` to the CLI. The automatic outline appears before the first body heading,
after any introductory paragraphs. A body marker takes ownership of placement
and suppresses the automatic copy. `toc_depth` / `tocDepth` supplies a default
depth; an explicit marker depth overrides it. A depth option by itself filters
explicit markers without enabling an automatic outline.

The outline includes body headings in source order, including headings nested
in lists and quotes and headings produced by safe HTML lowering. Appended note
headings and the generated Notes heading are excluded. A marker in an endnote
still renders the body outline, but cannot suppress its automatic copy. If the
body has no headings, an explicit marker draws no rows and automatic generation
adds no content.

Markers must occupy a whole paragraph. Code spans, fenced code, links, enclosing
emphasis such as `**[TOC]**`, and markers embedded in a sentence remain ordinary
content. The existing shared AST does not retain bracket-escape provenance:
escaping the brackets of a whole-paragraph marker may still produce the same
text node. Use a code span when the marker itself should be visible.

## Native and library use

```sh
fmd guide.md --to svg --out guide.svg --toc --toc-depth 2
fmd batch guide.md reference.md --to svg --out-dir posters --toc --toc-depth 2
```

Native single-file and batch exports also consume the existing frontmatter and
configuration defaults. For example:

```markdown
---
toc: true
toc_depth: 2
---
# Guide
```

```rust
use franken_markdown::{SvgOptions, parse_markdown, render_svg};

let document = parse_markdown("# Guide\n\n## Installation\n");
let options = SvgOptions {
    toc: true,
    toc_depth: Some(2),
    ..SvgOptions::default()
};
let svg = render_svg(&document, &options);
# let _ = svg;
```

`render_svg_with_report`, `render_svg_with_diagnostics` and
`render_svg_with_resources` use the same outline implementation. `SvgOptions`
now has the two fields above; existing callers constructing every field
explicitly should add them or use `..SvgOptions::default()`. Defaults remain
`false` and `None`. The MCP `fmd.render_file` SVG target accepts `toc` and
`tocDepth`, with the same strict depth validation as its other formats.

## Browser and worker use

```javascript
import { renderSvg } from "./franken_markdown.js";

const output = await renderSvg(markdown, { toc: true, tocDepth: 2 });
```

The same options work through the document worker, multi-format document
publication and Flow's SVG export. The worker validates and snapshots the
choices with the rest of the request. The direct browser facade inherits
frontmatter TOC defaults when available; an explicit `toc: false` overrides
automatic generation without hiding explicit source markers.

The additive generated binding is `renderSvgConfiguredNavigation`. It combines
TOC options, authoring profiles and the existing image/font resource payloads.
Older generated bindings retain their argument lists. Explicit TOC options
require a matching navigation-capable WASM package; the facade refuses an
older or mismatched package with `UNSUPPORTED_WASM_PACKAGE` before rendering
instead of silently dropping the settings. Existing requests without those
options retain the legacy binding fallback.

## Heading identity and bounded work

Generated rows target immutable prepared heading identities, not guessed title
strings or authored fragment names. Duplicate titles, empty headings, inserted
safe-HTML headings and headings named like footnotes therefore keep distinct
destinations. The destination is registered when the actual heading is painted,
after all preceding outline rows have affected layout. Wrapped rows retain their
one logical target and use a separate hit rectangle on each physical line.

Row titles retain plain visible text; source formatting and links supply their
content, images supply alt text, and math supplies its source text. Generated
footnote citations are omitted from row titles. An empty heading uses `Section`.
The original heading keeps its normal rendering and any source links.

The outline inventory allows at most 4,096 headings, 64 KiB per title and 1 MiB
of retained title text. Individual title walks also have the existing navigation
style limits of 8,192 inline nodes and 128 levels. Exceeding an inventory limit
disables the entire generated outline, keeps explicit markers visible and emits
`svg_toc_limit`; source content is still rendered. Repeated markers additionally
share a cumulative limit of 16,384 generated rows and 4 MiB of expanded row text.
An expansion is admitted in full before painting; once this budget is exhausted,
later markers remain visible and one `svg_toc_limit` warning is emitted. Earlier
outline links still bind to headings that are painted later. Existing navigation budgets
continue to control active destinations, hit regions and emitted metadata and
retain their existing diagnostic codes. No source text or resource budget is
weakened.

Navigation needs an interactive SVG viewer. A rasterized poster or an SVG used
as an inert image does not acquire interactive links. These outlines extend
existing link accessibility and do not claim complete SVG document accessibility.

## Verification

`tests/svg_toc_test.rs` checks emitted destination identity, complete native
markers, per-marker depth, nested placement, safe HTML, note collisions, empty
headings, narrow wrapping, determinism and equality across public render APIs.
The native CLI test covers flags, frontmatter and batch parity. MCP and WASM
adapter tests verify that their options reach the same core output. Browser,
document worker and Flow tests separately cover option admission, compatibility,
frontmatter precedence and snapshot ownership; their binding doubles do not
establish generated-WASM rendering parity.
