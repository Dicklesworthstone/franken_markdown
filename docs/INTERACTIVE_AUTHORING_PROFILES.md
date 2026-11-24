# Authoring profiles in editable HTML

Self-contained HTML exports retain the source dialect selected by
`HtmlOptions.profile`. The embedded editor uses that same selection after local
edits, when preparing a styled print preview, and after Save HTML and reopen.
No server or network connection is required.

| Selection | Definition lists | Standalone authored page breaks |
|---|---|---|
| `commonmark-gfm` | Parsed as ordinary Markdown | Keep the lightweight renderer's existing escaped HTML/comment policy |
| `gfm-plus` | Rendered as semantic `dl`, `dt` and `dd` elements | Rendered as page breaks |
| Omitted | Same extension grammar as `gfm-plus` | Same extension grammar as `gfm-plus` |

The omitted selection matches the native parser's existing default. Newly
generated editable files now retain these extensions when their source changes.
An older exported file still contains its original bundled JavaScript; opening
that file does not automatically upgrade its parser.

## Select the dialect before parsing

The initial preview is rendered from the native AST. Callers supplying an AST
must parse the source with the same profile as the HTML options:

```rust
use franken_markdown::{
    HtmlOptions, Profile, parse_markdown_with_profile,
};
use franken_markdown::interactive::render_interactive_html_checked;

let source = "Term\n: Definition\n";
let options = HtmlOptions {
    profile: Some(Profile::GfmPlus),
    ..HtmlOptions::default()
};
let document = parse_markdown_with_profile(source, options.profile);
let editable_html =
    render_interactive_html_checked(&document, source, &options)?;
```

Applying an output transform after parsing cannot recover the other dialect's
source ownership. For example, a colon definition can split a paragraph whose
emphasis or link would otherwise continue across physical lines.

The lightweight parser accepts the same canonical strings as an optional third
argument:

```javascript
parseMarkdownClient(source, imageAssets, 'commonmark-gfm');
parseMarkdownClient(source, imageAssets, 'gfm-plus');
```

An omitted or null third argument uses the extended default. An invalid explicit
value throws an error with code `FMD_INVALID_PROFILE` before reading or coercing
the source or image assets.

## Public browser API

The browser package also accepts an authoring profile when generating a new
editable file:

```javascript
import { renderInteractiveHtml } from './franken_markdown.js';

const workspace = await renderInteractiveHtml(source, {
  profile: 'commonmark-gfm',
  title: 'Editable report',
});
```

The selected native binding parses the source under that dialect before
rendering the initial preview and saving its profile metadata. The public
facade returns `workspace.profile` only after that profiled binding succeeds.
Configured styles, explicit images and fonts, and inherited source metadata use
the same path.
The document worker accepts the same option for `interactive-html` jobs and
requires the renderer to acknowledge the selected profile before publication.

An explicit profile requires a matching `wasm-workspace` or `wasm-full` package
exporting `renderInteractiveHtmlConfiguredProfiled`. Older or incompatible
packages fail with `UNSUPPORTED_WASM_PACKAGE` before source coercion or resource
copying. Omitting the profile keeps the existing legacy bindings and result
shape. The public browser API follows the shared authoring-option contract:
`undefined` means omitted; explicit `null`, aliases, wrong types, and padded or
case-varied names are invalid.

## Definition lists and page breaks

Under `gfm-plus`, one or more term lines followed by a colon definition form a
definition list. Multiple definitions, continued physical lines, and inline
formatting follow the native definition-list grammar. Terms and definitions use
the ordinary inline renderer, including links, code, emphasis, images, and
footnote references. Footnote ID planning includes both terms and definitions.

```markdown
Primary term
Alternate term
: A definition with **emphasis** and a note.[^details]
: A second definition.

[^details]: More information.

<!-- pagebreak -->

Next page.
```

An exact standalone `<!-- pagebreak -->` HTML block becomes:

```html
<div class="fmd-pagebreak" style="break-after: page;"></div>
```

Code blocks, inline code, and larger HTML blocks retain ownership of their
contents; text resembling a directive inside those constructs is not promoted
to a page break. Browser print pagination determines the final paper layout.

## Tables of contents during editing

Authored whole-paragraph markers such as `[TOC]`, `[[TOC]]`, `[[_TOC_]]` and
`[TOC:2]` retain their navigation after local edits, print preparation, and
Save HTML and reopen. A marker's depth selects the greatest source heading
level to include. Code, prose containing a marker, and resolved Markdown links
keep their ordinary meaning.

The outline uses the actual body heading IDs, including Unicode and duplicate
suffixes. Labels preserve readable link text, image descriptions, code, and
literal footnote labels; generating the outline does not create extra note
references. Headings inside quotes, callouts, and lists participate. Note
headings retain their own anchors without becoming entries in the body outline.

Configured automatic tables of contents appear once before the first body
heading. Any authored body marker selects its own placement instead. Markers
inside notes can show the body outline without suppressing its automatic
table of contents. Marker depth overrides the configured default depth.
These decisions use the same parser for the edited preview and styled print
fragment, so a second table of contents is not synthesized from rendered HTML.

## Retention and recovery

Explicit selections are stored in a direct child of the document body:

```html
<script type="application/json" id="fmd-authoring-profile">{"version":1,"profile":"gfm-plus"}</script>
```

The controller captures this data once when the document opens. Later source
edits or replacement of the live metadata element cannot switch dialects.
Save HTML writes the captured selection into the saved copy before its
executable bundle. An omitted selection stays omitted. Metadata-shaped content
inside the preview is never used as application configuration.

Malformed, duplicated, unsupported-version, or unknown-profile metadata is
reported when opening the workspace. Preview regeneration, HTML export, and the
editor's print commands refuse to proceed with an ambiguous dialect. The
Markdown remains editable and downloadable, including its original byte
representation when untouched or exactly restored.

Native Flow/WASM workspaces keep their dialect in their native runtime settings.
Their runtime or embedded payload remains authoritative; lightweight profile
metadata does not select a fallback parser or replace native export behavior.

## Scope of the lightweight renderer

Profile retention preserves the selected definition-list and page-break grammar
through the offline editor. The lightweight JavaScript renderer remains a
reduced renderer: this change does not promise full native/CommonMark
conformance, native typesetting, or native raw-HTML output. In particular, its
existing policy escapes unsupported raw HTML and comments. Untouched source and
exact undo preserve the native initial preview, which may contain features
outside the JavaScript subset.

The focused tests exercise the shipped parser, controller and presentation
adapter, including edit/save/reopen and print routing. Browser layout, real
download navigation and physical print pagination require browser qualification.
