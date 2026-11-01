# PDF list layout

PDF lists measure their bullets, ordered numbers and task labels using the
resolved body font and size. A fitting list uses a shared marker gutter and a
hanging text column. Continuation paragraphs and nested blocks retain that
column, and ordinary fitting documents keep their existing placement.

When a marker and its gutter cannot leave one body em for the item, the complete
marker moves above the content. Excessive nesting stops consuming the remaining
body width. Ordered labels wider than the available body wrap across measured
source clusters without losing digits, inserting hyphens or reducing type size.
Host-constructed ordered lists continue beyond a `u64::MAX` starting value.
Ordinary item prose retains its normal wrapping and hyphenation behavior.

Task labels remain intact so each item produces one vector checkbox and one
accessible `[x]` or `[ ]` state. Empty items retain a marker row. A marker above
leading prose uses the same prefixed paragraph path as hanging lists, including
the marker in each optional paragraph shape. Nested list and item identities
remain attached to every line for tagged PDF and pagination.

List-owned quotes, headings and definition-list text also reclaim indentation
when it would exhaust their physical measure. They retain their requested type
size and the same source content. Figures and image galleries keep their block
layout, links and alternative text; a marker above them does not convert those
figures into inline icons. Code, tables, mathematics and other child blocks use
their normal rendering paths.

This behavior needs no new option. Deeply nested or unusually wide-label lists
can use more vertical space because their source now remains on the page.
The geometry change applies to lists and their descendants; ordinary non-list
paragraphs keep their existing layout policy.

`tests/pdf_list_geometry_test.rs` reads emitted PDF font widths, Unicode maps
and text matrices to check physical bounds, original type size, complete source
order and marker counts. It covers default, optimal and baseline-grid layouts,
wide ordered labels, empty/task items, rich nested content, linked figures and
galleries. Existing list scale and joint-pagination suites cover the ordinary
hanging path.
