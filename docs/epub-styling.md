# EPUB typography and appearance

Document and book exports use the shared `HtmlOptions.theme` for their chapter
and navigation styles. The native CLI applies `--font` and `--font-scale` to
EPUB output, just as it does to the other render formats:

```bash
fmd guide.md --to epub --font serif --font-scale 125% --out guide.epub
```

The same options work through the library:

```rust
use franken_markdown::{FontScale, HtmlOptions, Theme, parse_markdown, render_epub};

let options = HtmlOptions {
    theme: Theme::serif().with_font_scale(FontScale::from_factor(1.25)),
    ..HtmlOptions::default()
};
let document = parse_markdown("# Reading edition\n\nLarger serif text.\n");
let bytes = render_epub(&document, &options)?;
```

Pass the same options to `epub::render_book_epub` to style all chapters in a
book. A book has one shared stylesheet, also linked from its navigation page.

## Text and layout

The default body family is sans serif. A serif theme selects the reader's serif
family. EPUB does not bundle fonts unless `font_assets` contains a supplied
face; explicit fonts use the existing publication-wide subsets and fallback
faces.

The default 16-pixel theme baseline becomes `font-size: 100%`. A 20-pixel theme
becomes `125%`, and a 32-pixel theme becomes `200%`. Headings, code and tables
use the shared `TypeScale` hierarchy as ratios of that body size. Nested code
inside a code block inherits its block's size, so it does not shrink twice.
These relative units allow a reading system's text-size controls to continue
working. Reading systems can still override publisher styles according to the
reader's preferences.

Line height, table-cell padding and corner radii follow the theme. Long code
lines wrap, and images stay within the available page width. The reader owns
the page dimensions and text column; the HTML `max_width_px` and PDF page-size
settings do not impose a fixed EPUB viewport.

## Colors and reading modes

Text, backgrounds, links, tables, code, quotations and selection highlights use
the theme's color tokens. Generated CSS uses concrete colors and relative
sizes, so basic styling does not depend on CSS custom-property support.

With `SystemAppearance::Auto` and `DarkModePolicy::Auto`, EPUB emits the base
palette plus a `prefers-color-scheme: dark` override using `dark_colors`.
Disabling automatic dark mode keeps the base palette. An explicit appearance
such as `Light`, `Dark`, `Charcoal`, or either high-contrast mode stays fixed
instead of following a media query. High-contrast modes keep syntax tokens in
the surrounding text color and retain bold keywords and italic comments.

Color tokens accept hexadecimal and named colors, plus numeric CSS color
functions such as `rgb()` and `hsl()`. Values that can introduce CSS rules,
escapes, or URLs fall back to the corresponding default palette token.
Nonfinite line-height or padding values use the standard spacing defaults.

## Custom styles and fonts

`HtmlOptions.custom_css` replaces the generated stylesheet verbatim, including
when the value is an empty string. The CLI's `--css` option uses the same path.
Chapter content retains a `.fmd` wrapper for scoped author rules.

When host fonts are embedded, their font-family rules follow the generated
theme stylesheet. They replace the generic family while preserving the text
scale and spacing. With custom CSS, the font stylesheet comes first and the
author's stylesheet comes last, allowing explicit author rules to win.

Output remains byte-deterministic for the same content and effective options.
The effective stylesheet and embedded font resources contribute to the EPUB
identifier, so differently styled editions do not reuse an identifier solely
because their Markdown text is identical. Default EPUB bytes and identifiers
therefore differ from older exports that always used a fixed serif stylesheet.
