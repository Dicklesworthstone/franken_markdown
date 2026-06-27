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
}
