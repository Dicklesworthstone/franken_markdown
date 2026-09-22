# Linked Markdown figures

Each blue figure below should remain visible and retain its alt text.
The first image opens an external address; list images jump to Details.

[![External figure](linked-figure.svg)](https://example.com/figure)

**![Formatted standalone figure](linked-figure.svg)**

- [![Bullet figure](linked-figure.svg)](#details)

  This paragraph follows the figure within the same item.

- [x] [![Completed task figure](linked-figure.svg)](#details)

> A figure inside a nested list:
>
> - Parent item
>   - [![Nested figure](linked-figure.svg)](#details)

## Details

The figures retain their list labels and clickable rectangles across page breaks.
