# Images inside PDF text

Images mixed with prose, badges, and images in table cells render as inline
objects. They participate in line breaking and reserve their own width and
height. A paragraph containing a single image retains the standalone figure
layout; a paragraph containing several large pictures can use the figure
gallery layout.

## Size and fitting

An inline image starts at its natural size, using 0.75 PDF points per image
pixel. Its preferred box is reduced, preserving its aspect ratio, to fit both
of these limits at the surrounding font size:

- Height: 1.75 em.
- Width: 36 em.

Layout then reduces an image further when that preferred box exceeds the width
of its containing paragraph or table cell. The paragraph width accounts for
list and blockquote indentation. A table cell uses its allocated text width,
after the table's ordinary font and column-width choices. Images that already
fit keep their previous dimensions; fitting never enlarges them.

The reduction applies to the entire box, including its baseline offset, so the
image keeps its aspect ratio and position relative to surrounding text. The
same fitted box drives line measurement, line height, painting, linked-image
annotations, and the tagged PDF `/Figure` bounding box. It also applies to
continuation lines and both PDF emission modes.

For example, both banners below fit their own cells even when their source
images are much wider than the table:

```markdown
| First result | Second result |
|---|---|
| [![First banner](first.svg)](https://example.com/first) | [![Second banner](second.svg)](https://example.com/second) |
```

The fit budget is the full width of a line, rather than the space left after
preceding words. A badge that fits a complete line can move to the next line
without shrinking merely because it follows some text. Adjacent images, or an
image touching text without a space, gain invisible break opportunities at
their boundaries only when the combined source word is too wide. Those breaks
do not add spaces, hyphens, or other visible content.

## Image data and accessibility

Fitting changes placement, not the source asset. Raster image data keeps the
existing embedding path, and SVG images retain vector drawing. JPEG display
dimensions include EXIF orientation before sizing. The alt text, link target,
and tagged structure stay attached to the same image after fitting.

Fitted SVG placement retains enough matrix precision for narrow cells and
very wide source view boxes. Rounding a small SVG scale to two decimal places
could otherwise paint beyond the measured box or make the image disappear.

Missing or unsupported image assets continue to use the existing alt-text
fallback. This fitting policy concerns inline image boxes; the existing rules
for standalone figures, indivisible formulas, and oversized text clusters are
separate.
