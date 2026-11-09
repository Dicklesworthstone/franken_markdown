# Syntax-highlighted SVG code

Fenced code in SVG posters now uses the same first-party language classifiers
and syntax palette as HTML and PDF. Specify the ordinary fence language:

````markdown
```rust
fn main() {
    let count: u32 = 42;
    println!("count = {count}");
}
```
````

This applies to all SVG entry points, including native render/watch/batch,
MCP file export, browser `renderSvg`, document-worker publication and Flow SVG
export. There is no additional option or generated binding. The language roster
and aliases come from `highlight::is_supported`; SVG does not keep a second
language table. Unknown or absent languages retain ordinary plain code.

The output remains a self-contained outlined SVG. Syntax colors do not add
browser fonts, scripts, CSS dependencies, text elements or executable source
markup. Successful diagram fences retain their existing priority and render as
figures. A diagram that the shared compiler does not support retains its complete
code source, with syntax coloring when that language is recognized. Raw-HTML
source panels and unsupported-math source panels keep their plain styling.

## Whole-fence classification and unchanged layout

The lexer classifies the original complete code block once. Multiline strings,
comments and language-specific state therefore continue across physical lines;
the renderer never restarts the lexer at a visual wrap.

Original UTF-8 byte spans are projected through the existing code whitespace
rules: tabs expand to four-column source stops, LF/CRLF physical line endings
separate rows, and blank lines are retained. The existing wrapper and font
shaper still prepare every line. Syntax classification does not split their
runs or create additional break opportunities.

Color is applied when those already-prepared glyphs are painted. All glyphs in
one source cluster use its first source byte's token class, keeping combining
marks and any shaped cluster together. Glyph identities, advances, positioning,
font fallback, emergency wrapping and panel geometry match plain rendering of
the same source. The existing conservative SVG code-ligature policy is unchanged.
This feature adds color rather than italic or bold token fonts.

Light, dark and explicit high-contrast appearances use the effective shared
theme. High-contrast code retains its actual foreground color. Operators use
the numeric token color; plain text and punctuation retain the foreground.
Custom renderers can obtain exactly those choices with the additive method:

```rust
use franken_markdown::{Theme, highlight::Tok};

let theme = Theme::default();
let colors = theme.effective_colors(false, false);
let keyword = theme.code_color(colors, Tok::Keyword);
# let _ = keyword;
```

## Bounded work and complete fallback

Language tags are limited to 256 UTF-8 bytes before normalization or lexer
lookup, including host-constructed ASTs. A recognized fence admits at most
256 KiB of original code, 65,536 classified spans and 1 MiB of expanded text.
The input bound applies before the whole-block lexer allocates its span vector.
The span bound checks that bounded result before retaining a paint plan.

All fences in one poster additionally share 4 MiB of classification input
(original code plus its language tag) and 262,144 spans. Attempted classification
consumes input budget even if its resulting span inventory is refused. Unknown
normal-sized language tags do not invoke the code classifier or consume that
budget. Each new render starts with fresh limits.

Exceeding a limit preserves the complete ordinary code panel and emits one
`svg_code_highlight_limit` warning for that fence. The renderer admits an entire
color plan before painting; it does not leave a partially colored prefix.
Invalid or incomplete UTF-8 span coverage instead produces
`svg_code_highlight_invalid` with the same complete plain fallback. Existing
font, missing-glyph and wrapping diagnostics are retained independently.

These limits bound the additional syntax-classification work. They do not impose
a new whole-document output limit or truncate the existing code content. Use
`render_svg_with_diagnostics` or `render_svg_with_resources` to receive the
warnings; the existing simpler SVG APIs continue to return their ordinary
outputs without diagnostic details.

## Regression coverage

The focused painter tests cover multiline comments and strings, Unicode and
combining clusters, four-column tabs, CRLF and blank lines, narrow emergency
wrapping, supplied fonts, every palette class and complete budget fallbacks.
The emitted-output tests compare highlighted output with plain source while
masking only glyph fill values. Glyph definitions, transforms, dimensions,
panels, navigation and every remaining byte must match. They also cover
nested/list/endnote fences, deterministic public API parity, explicit appearance,
hostile language metadata and retained diagram priority.

The feature introduces no new dependency, file access or network access in the
rendering core. The Rust tests exercise native output; generated-WASM/browser
execution remains a separate validation scope.
