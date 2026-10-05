# Syntax-highlighted shaped flow

The continuous-flow core can now apply the same fenced-code lexer used by HTML
without choosing a second set of line breaks or shaping each token separately.
This is opt-in. Existing `to_styled_display_list`, `BundledFlowFonts::render`,
`FlowShapeCache::render`, editor sessions and WASM session defaults are unchanged.

```rust
use franken_markdown::{FontFamily, ResumableFlowDisplay};
use franken_markdown::flow_display::FlowLayoutOptions;
use franken_markdown::fonts::BundledFlowFonts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fonts = BundledFlowFonts::new(FontFamily::Sans)?;
    let mut engine = ResumableFlowDisplay::new("```rust\nlet answer = 42;\n```", 64);
    engine.process_all()?;
    let display = fonts.render_highlighted(&engine, FlowLayoutOptions::default())?;
    assert!(!display.items().is_empty());
    Ok(())
}
```

Hosts with their own shaper call `engine.to_highlighted_display_list(options,
shape)` using the same four-argument callback as styled reflow. Hosts retaining a
`FlowShapeCache` use `cache.render_highlighted(&engine, options)`. The cache still
retains only exact-key font runs, not lexer colors, positions or document state.

## Geometry and source guarantees

Each supported fence is lexed in its entirety, preserving multiline strings and
comments across physical lines and wrap boundaries. Missing/unknown languages
remain plain. Highlighting uses the final shaped clusters; it cannot split a
ligature or combining sequence at a lexical boundary. A cluster spanning token
classes takes the color of its first source byte. Fonts, glyphs, advances and
positioning offsets are retained; fragment-local coordinates are rebased, with
normal floating-point roundoff rather than a new measurement.

The accessible code transcript remains whole and exact to the parsed code block,
including tabs, blank lines and terminal newlines. Source spans remain enclosing
Markdown ranges, not fabricated exact token offsets. Fragment UTF-8/UTF-16 and
glyph ownership indices are local to each output run, as in styled flow.

The source engine is never mutated. Failed admission, shaping or painting returns
no partial display list. Before shaping, a supported fence is limited to
`max_items` source bytes (a conservative lexer-span allocation bound), and all
supported fences together to `max_total_shape_bytes` bytes. The latter is a
separate allowance from cumulative shaping. Final paint fragments also obey
`max_items`. These deliberately conservative checks can reject a large fence
that ordinary styled rendering accepts; choose the unhighlighted API when that
tradeoff is inappropriate.

## Canvas palettes

`FlowCanvasRenderer` accepts `tok-kw`, `tok-ty`, `tok-fn`, `tok-st`, `tok-nu`,
`tok-cm`, `tok-op` and `tok-pn` as `colorRole` values and custom palette keys.
The default colors follow the light HTML token palette. Hosts choosing a dark or
high-contrast background must supply corresponding token colors explicitly.
Unknown incoming roles retain the existing text-color fallback; unknown custom
palette keys remain errors. Glyph-cache identity does not include paint color.

This change does **not** add a WASM session option or rebuild the generated WASM
package. Canvas support is ready for snapshots carrying the new roles; the
existing browser session still uses its ordinary styled rendering path.

## Validation and scope

Eleven Rust regression tests cover the core and host entrypoints, including
multiline state, wrapped Unicode/ligatures, RTL geometry, repeated nested fences,
measurement-window parity, admission, unchanged shaper calls, real bundled-font
marks, and shape-cache reuse. They were added but could not be run in the authoring
sandbox, which lacked Rust, Cargo, rustfmt and DSR. Formatting and Rust gates
remain unverified.

Five recording-Canvas tests passed locally against the modified renderer using
an isolated legacy-session boundary and the extracted outline validator. The
unchanged renderer fails three of these five tests. The committed test imports
the normal production modules and can be run in a checkout with:

```sh
node --test wasm/flow_canvas_syntax.test.mjs
```

`node --check` passed for the Canvas module. The TypeScript palette fixture passed
an isolated `tsc --noEmit --strict --skipLibCheck` check; that does not validate
the rest of the package declarations. These are unit-level checks, not generated
WASM, browser raster, full package or native/DSR integration evidence.
