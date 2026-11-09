# Chapter languages in PDF books

PDF book exports retain each chapter's frontmatter language for paragraph
hyphenation and tagged reading-language ownership. This uses the existing book
input model; there is no new export flag or browser setting.

```markdown
---
title: Deutsche Anleitung
lang: de-DE
---

# Anleitung

Dieser Abschnitt verwendet die vorhandenen deutschen Trennmuster.
```

A second chapter can declare `lang: nl-NL`, `fr-CA`, or another language without
changing the language of the first chapter. A missing or blank chapter language
falls back to the book's `lang` option, then to the existing `en-US` default.
The PDF catalog continues to describe the book's language.

## Typography and reading order

Paragraphs and list paragraphs select language through the same shared
hyphenation policy as standalone PDF exports. The existing English, German,
French, Dutch, and Spanish dictionaries and per-word script heuristics are
unchanged. Regional tags select their supported base-language dictionary;
unknown tags retain the existing dictionary fallback while preserving the
declared tag for readers. This feature does not add fonts, script shaping, or
bidirectional paragraph layout.

Each chapter has its own tagged `/Sect` with `/Lang`. Paragraphs, headings,
lists and labels, quotes, code, figures, tables, and repeated table headers keep
their existing structure inside that chapter. Two German chapters separated
by an English chapter remain distinct sections in source order. Language
strings are shared internally, but section identities are never merged merely
because their languages match.

The canonical preparation steps retain chapter ownership when safe HTML is
lowered, authoring profiles expand a definition list, or navigation inserts a
chapter landing heading. Page-bottom notes and full-size endnote fallbacks
retain the language of their defining chapter, including notes reached by a
link from another chapter. Generated global TOC rows and the generated `Notes`
heading inherit the document language. A TOC inside a source list or quote keeps
that container in its chapter; only the generated TOC paragraph receives a
document-language override. One source list remains one logical list.

Language participates in paragraph-cache identity, while cached paragraph and
table geometry receives the current chapter's ownership on reuse. Page-fitting
trials and the final layout use the same language context. Alternative paragraph
shapes selected by joint pagination retain the finalized source scope.

## Export surfaces and compatibility

The shared book PDF pipeline applies this behavior to `render_book_pdf`,
`render_book_pdf_reported`, retained `BookRenderer` exports, exports using
borrowed `PdfOptions`, native book export, browser/WASM book export, and PDF/A
book export. Existing signatures and the public Markdown AST are unchanged.

Standalone document exports retain their document-wide language behavior.
The low-level `book_pdf_document` assembly API returns only a Markdown AST;
use the book rendering APIs to retain chapter metadata and navigation.

Books with no nonblank chapter language overrides keep the existing PDF bytes
and structure. An explicit override can change line and page breaks, and adds
language ownership to the structure tree. It does not establish PDF/UA
certification or certify any particular reader's language support.

`tests/book_pdf_languages.rs` inspects emitted objects, structure ancestry,
marked-content ownership, and ToUnicode-decoded line text. It compares chapter
line breaks with standalone exports using the same language, including fitting,
and covers interleaved languages, repeated cached content, rich notes, profile
normalization, navigation headings, fallback language, reported output, PDF/A,
and the no-override compatibility path.
