# franken_markdown showcase

A quick tour of what the renderer handles today. The default theme is tuned to
look like a high-quality preview: comfortable measure, generous **leading**, and
careful typography.

## Inline formatting

You get **bold**, *italic*, ***both***, `inline code`, ~~strikethrough~~, and
[links](https://github.com/Dicklesworthstone/franken_markdown) with an autolink
fallback like <https://example.com>. Hard breaks work too —
this line ends with two spaces.

## Lists

- Clean, readable bullets
- With nested-friendly spacing
- And task items:

- [x] parse the document
- [ ] shape the glyphs
- [ ] break the lines (Knuth–Plass)

Ordered lists too:

1. First
2. Second
3. Third

## A table

| Feature        | Status   | Notes                          |
|:---------------|:--------:|-------------------------------:|
| HTML output    | working  | all-in-one, themeable          |
| PDF output     | building | LaTeX-grade typesetting        |
| Zero deps      | yes      | engine has no external crates  |

> Blockquotes look elegant, with a soft accent bar and tinted background.
> They wrap nicely across multiple lines.

## Code blocks

```rust
fn main() {
    println!("franken_markdown: pure-Rust, dependency-lean.");
}
```

---

That's the showcase. Run `fmd render examples/showcase.md --out showcase.html`.
