//! The Markdown document AST.
//!
//! Intentionally small and rendering-oriented: every variant maps to something
//! the HTML emitter and the PDF layout engine know how to typeset. The parser
//! produces this; the renderers consume it. Keeping the AST renderer-agnostic is
//! what lets the HTML and PDF outputs share one structural source of truth.

/// A parsed Markdown document: a sequence of block-level elements.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    /// Top-level blocks in source order.
    pub blocks: Vec<Block>,
}

/// A block-level element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// `# ` .. `###### ` ATX heading (level 1–6).
    Heading { level: u8, inlines: Vec<Inline> },
    /// A paragraph of inline content.
    Paragraph(Vec<Inline>),
    /// A fenced (``` or ~~~) or indented code block, with an optional info word.
    CodeBlock { lang: Option<String>, code: String },
    /// A block quote containing nested blocks.
    BlockQuote(Vec<Block>),
    /// An ordered or unordered list.
    List(List),
    /// A GFM pipe table.
    Table(Table),
    /// A thematic break (`---`, `***`, `___`).
    ThematicBreak,
    /// A raw HTML block (only emitted when raw HTML is allowed; otherwise the
    /// parser keeps it as a paragraph of escaped text).
    HtmlBlock(String),
    /// A GFM footnote definition `[^id]: content`. Kept in the block flow at
    /// source position; emitters skip it in normal flow and render a notes
    /// section from all definitions (numbered by first-reference order).
    FootnoteDefinition { id: String, blocks: Vec<Block> },
    /// A display mathematics block (`$$...$$` or ````math ... ````).
    MathBlock(String),
    /// A definition list (GFM-plus).
    DefinitionList(Vec<DefinitionItem>),
    /// A forced page/chapter break (book builder, bead j0o4). Renders as a
    /// zero-content boundary: a `break-after: page` div in HTML, a new page in
    /// PDF. Not produced by the Markdown parser — hosts insert it between
    /// merged chapters.
    PageBreak,
}

/// An item in a definition list: one or more terms, followed by one or more definitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionItem {
    /// The term(s) being defined.
    pub terms: Vec<Vec<Inline>>,
    /// The definition(s) for the term(s).
    pub definitions: Vec<Vec<Inline>>,
}

/// An ordered or unordered list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    /// True for ordered (`1.`) lists.
    pub ordered: bool,
    /// Starting number for ordered lists.
    pub start: u64,
    /// Tight lists render items without `<p>` wrappers / extra leading.
    pub tight: bool,
    /// The list items.
    pub items: Vec<ListItem>,
}

/// A single list item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListItem {
    /// `Some(checked)` for GFM task-list items (`- [ ]` / `- [x]`).
    pub task: Option<bool>,
    /// The item's block content.
    pub blocks: Vec<Block>,
}

/// Column text alignment for a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// No explicit alignment.
    None,
    /// `:---`
    Left,
    /// `:--:`
    Center,
    /// `---:`
    Right,
}

/// A GFM pipe table: a header row, a per-column alignment, and body rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// Per-column alignment (length defines the column count).
    pub align: Vec<Align>,
    /// Header cells (one inline sequence per column).
    pub head: Vec<Vec<Inline>>,
    /// Body rows; each row is a list of cells.
    pub rows: Vec<Vec<Vec<Inline>>>,
}

/// An inline-level element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Literal text.
    Text(String),
    /// `*em*` / `_em_`.
    Emphasis(Vec<Inline>),
    /// `**strong**` / `__strong__`.
    Strong(Vec<Inline>),
    /// `~~strikethrough~~` (GFM).
    Strikethrough(Vec<Inline>),
    /// `` `code span` ``.
    Code(String),
    /// `[text](dest "title")` link.
    Link {
        dest: String,
        title: Option<String>,
        content: Vec<Inline>,
    },
    /// `![alt](dest "title")` image.
    Image {
        dest: String,
        title: Option<String>,
        alt: String,
    },
    /// A soft line break (source newline within a paragraph).
    SoftBreak,
    /// A hard line break (two trailing spaces or a trailing backslash).
    HardBreak,
    /// Raw inline HTML (only when allowed).
    Html(String),
    /// A GFM footnote reference `[^id]`. The superscript number is assigned
    /// renderer-side by first-reference document order; `id` links the
    /// reference to its `[^id]:` definition. References whose id has no
    /// definition are rewritten to literal text by a post-parse pass.
    FootnoteRef { id: String },
    /// Inline mathematics (`$…$`).
    Math(String),
    /// Inline display mathematics (`$$…$$`).
    DisplayMath(String),
}

/// Try to interpret the blocks of a `BlockQuote` as a GitHub Flavored Markdown alert
/// (e.g. `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`).
/// Returns `Some((tag, label, body_blocks))` on match, or `None` if plain quote.
#[must_use]
pub fn alert_body(inner: &[Block]) -> Option<(&'static str, &'static str, Vec<Block>)> {
    const TAGS: [(&str, &str); 5] = [
        ("note", "Note"),
        ("tip", "Tip"),
        ("important", "Important"),
        ("warning", "Warning"),
        ("caution", "Caution"),
    ];
    let first = inner.first()?;
    let Block::Paragraph(inlines) = first else {
        return None;
    };
    let Some(Inline::Text(text)) = inlines.first() else {
        return None;
    };
    let trimmed = text.trim_start_matches([' ', '\t']);
    let rest = trimmed.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let tag_raw = &rest[..close];
    let (tag, label) = TAGS.iter().find(|(t, _)| t.eq_ignore_ascii_case(tag_raw))?;

    // GFM: the first line is only the marker (optional trailing space/tab).
    // Same-line prose (`> [!NOTE] urgent`) stays a normal blockquote so the
    // text is not swallowed into a false callout.
    if !rest[close + 1..].bytes().all(|b| b == b' ' || b == b'\t') {
        return None;
    }
    if inlines.len() > 1 && !matches!(inlines[1], Inline::SoftBreak | Inline::HardBreak) {
        return None;
    }
    let rest_inlines = &inlines[1..];
    let start_idx = if matches!(
        rest_inlines.first(),
        Some(Inline::SoftBreak | Inline::HardBreak)
    ) {
        1
    } else {
        0
    };
    let body_inlines = rest_inlines[start_idx..].to_vec();
    let mut body: Vec<Block> = Vec::new();
    if !body_inlines.is_empty() {
        body.push(Block::Paragraph(body_inlines));
    }
    body.extend_from_slice(&inner[1..]);
    Some((*tag, *label, body))
}
