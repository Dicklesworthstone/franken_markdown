//! Editor navigation from the shared AST and authoritative heading source map.
//! Only primary, top-level block spans are used; no guessed nested locations.

use std::collections::BTreeSet;

use franken_markdown::{Block, HeadingSourceAnchor, SourceSpan, SpannedDocument, parse_markdown_spanned};

use super::{Json, LineIndex, Position, number, object, position, string};

pub const MAX_NAVIGATION_ITEMS: usize = 4096;
const MAX_SELECTIONS: usize = 128;

struct Section {
    heading: HeadingSourceAnchor,
    end: usize,
    children: Vec<usize>,
}

struct Navigation<'a> {
    source: &'a str,
    document: SpannedDocument,
    index: LineIndex,
    sections: Vec<Section>,
    roots: Vec<usize>,
}

impl<'a> Navigation<'a> {
    fn new(source: &'a str) -> Result<Self, &'static str> {
        let document = parse_markdown_spanned(source);
        let map = document.source_map(source).map_err(|_| "source map could not be validated")?;
        if map.headings().len() > MAX_NAVIGATION_ITEMS {
            return Err("heading navigation budget exceeded");
        }
        let mut sections: Vec<Section> = Vec::new();
        let mut roots = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for heading in map.headings() {
            if !heading.origin.is_primary() { continue; }
            while let Some(&parent) = stack.last() {
                if sections[parent].heading.level < heading.level { break; }
                sections[parent].end = heading.source_span.start;
                stack.pop();
            }
            let current = sections.len();
            if let Some(&parent) = stack.last() {
                sections[parent].children.push(current);
            } else {
                roots.push(current);
            }
            sections.push(Section { heading: heading.clone(), end: source.len(), children: Vec::new() });
            stack.push(current);
        }
        Ok(Self { source, document, index: LineIndex::new(source), sections, roots })
    }

    fn range(&self, span: SourceSpan) -> Json {
        object([
            ("start", position(self.index.position(self.source, span.start))),
            ("end", position(self.index.position(self.source, span.end))),
        ])
    }

    fn section_span(&self, section: &Section) -> SourceSpan {
        SourceSpan::new(section.heading.source_span.start, section.end)
    }

    fn symbol(&self, section_index: usize) -> Json {
        let section = &self.sections[section_index];
        let name = if section.heading.title.is_empty() { "Untitled section" } else { &section.heading.title };
        object([
            ("name", string(name)),
            ("detail", string(&format!("#{}", section.heading.slug))),
            ("kind", number(15)), // SymbolKind::String is available to old clients.
            ("range", self.range(self.section_span(section))),
            ("selectionRange", self.range(section.heading.source_span)),
            ("children", Json::Array(section.children.iter().map(|&child| self.symbol(child)).collect())),
        ])
    }

    fn symbols(&self, uri: &str, hierarchical: bool) -> Json {
        if hierarchical {
            Json::Array(self.roots.iter().map(|&root| self.symbol(root)).collect())
        } else {
            Json::Array(self.sections.iter().map(|section| {
                let name = if section.heading.title.is_empty() { "Untitled section" } else { &section.heading.title };
                object([
                    ("name", string(name)), ("kind", number(15)),
                    ("location", object([("uri", string(uri)), ("range", self.range(section.heading.source_span))])),
                ])
            }).collect())
        }
    }

    fn fold(&self, span: SourceSpan) -> Option<(usize, usize)> {
        let start = self.index.position(self.source, span.start);
        let end = self.index.position(self.source, span.end);
        // An exclusive end at the next line's column zero must not hide that
        // next heading, code block, or the document's trailing empty line.
        let end_line = if end.character == 0 { end.line.saturating_sub(1) } else { end.line };
        (end_line > start.line).then_some((start.line, end_line))
    }

    fn folds(&self, limit: usize) -> Json {
        let limit = limit.min(MAX_NAVIGATION_ITEMS);
        let mut ranges = BTreeSet::new();
        // Section folds first, then real multiline blocks. Never interpret
        // fence contents as Markdown headings or try to locate nested text.
        for section in &self.sections {
            if ranges.len() >= limit { break; }
            if let Some(range) = self.fold(self.section_span(section)) { ranges.insert(range); }
        }
        for block in &self.document.blocks {
            if ranges.len() >= limit { break; }
            if matches!(&block.node, Block::CodeBlock { .. } | Block::BlockQuote(_)
                | Block::List(_) | Block::Table(_) | Block::MathBlock(_)
                | Block::HtmlBlock(_) | Block::FootnoteDefinition { .. } | Block::DefinitionList(_))
            {
                if let Some(range) = self.fold(block.span) { ranges.insert(range); }
            }
        }
        Json::Array(ranges.into_iter().map(|(start, end)| object([
            ("startLine", number(start)), ("endLine", number(end)), ("kind", string("region")),
        ])).collect())
    }

    fn selections(&self, positions: &Json) -> Result<Json, &'static str> {
        let Json::Array(positions) = positions else { return Err("positions must be an array"); };
        if positions.len() > MAX_SELECTIONS { return Err("selection range budget exceeded"); }
        let mut results = Vec::with_capacity(positions.len());
        for requested in positions {
            let offset = self.index.offset(self.source, Position::parse(requested)?)?;
            let mut spans = vec![SourceSpan::new(0, self.source.len()), SourceSpan::new(offset, offset)];
            spans.extend(self.sections.iter().map(|section| self.section_span(section)).filter(|span| span.contains(offset)));
            let next = self.document.blocks.partition_point(|block| block.span.start <= offset);
            if let Some(block) = next.checked_sub(1).and_then(|i| self.document.blocks.get(i)) {
                if block.span.contains(offset) { spans.push(block.span); }
            }
            spans.sort_by_key(|span| (std::cmp::Reverse(span.len()), span.start, span.end));
            spans.dedup();
            let mut parent = None;
            for span in spans {
                let range = self.range(span);
                parent = Some(match parent {
                    Some(parent) => object([("range", range), ("parent", parent)]),
                    None => object([("range", range)]),
                });
            }
            // There is always a whole-document range, including empty buffers.
            results.push(parent.unwrap_or(Json::Null));
        }
        Ok(Json::Array(results))
    }
}

pub fn request(
    method: &str,
    params: &Json,
    uri: &str,
    source: &str,
    hierarchical: bool,
    folding_limit: usize,
) -> Result<Json, &'static str> {
    let navigation = Navigation::new(source)?;
    match method {
        "textDocument/documentSymbol" => Ok(navigation.symbols(uri, hierarchical)),
        "textDocument/foldingRange" => Ok(navigation.folds(folding_limit)),
        "textDocument/selectionRange" => navigation.selections(params.get("positions").ok_or("missing positions")?),
        _ => Err("unsupported navigation request"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;
    fn array(value: &Json) -> &[Json] {
        match value { Json::Array(values) => values, _ => &[] }
    }

    #[test]
    fn outline_uses_parser_hierarchy_setext_and_not_fenced_headings() -> TestResult {
        let source = "# Root\n\n## Child\n\n```md\n# Not a heading\n```\n\n# Sibling\n\nSetext\n------\n";
        let navigation = Navigation::new(source)?;
        let outline = navigation.symbols("untitled:test", true);
        let roots = array(&outline);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].get("name").and_then(Json::as_str), Some("Root"));
        assert_eq!(array(roots[0].get("children").ok_or("no children")?).len(), 1);
        assert_eq!(array(roots[1].get("children").ok_or("no children")?).len(), 1);
        assert_eq!(array(&navigation.symbols("untitled:test", false)).len(), 4);
        Ok(())
    }

    #[test]
    fn section_range_contains_children_but_not_the_next_sibling() -> TestResult {
        let navigation = Navigation::new("# A\n\n## B\ntext\n# C\n")?;
        assert_eq!(navigation.sections[0].end, navigation.sections[2].heading.source_span.start);
        assert_eq!(navigation.sections[1].end, navigation.sections[0].end);
        assert!(navigation.sections[0].heading.source_span.end <= navigation.sections[0].end);
        Ok(())
    }

    #[test]
    fn folding_respects_exclusive_end_crlf_and_client_limit() -> TestResult {
        let navigation = Navigation::new("# A\r\nbody\r\n# B\r\n```\r\ncode\r\n```\r\n")?;
        assert_eq!(navigation.fold(navigation.section_span(&navigation.sections[0])), Some((0, 1)));
        assert!(array(&navigation.folds(0)).is_empty());
        assert_eq!(array(&navigation.folds(1)).len(), 1);
        assert!(array(&navigation.folds(4096)).iter().any(|fold| fold.get("startLine").and_then(Json::as_u64) == Some(3)));
        Ok(())
    }

    #[test]
    fn selection_chain_expands_from_unicode_caret_to_document() -> TestResult {
        let navigation = Navigation::new("# Root\n\n## Child\n\nA😀B\n")?;
        let positions = Json::Array(vec![object([("line", number(4)), ("character", number(3))])]);
        let selections = navigation.selections(&positions)?;
        let mut node = &array(&selections)[0];
        let range = node.get("range").ok_or("no selection range")?;
        assert_eq!(range.get("start"), range.get("end"));
        let mut count = 1;
        while let Some(parent) = node.get("parent") {
            node = parent;
            count += 1;
        }
        assert!(count >= 4);
        assert_eq!(node.get("range"), Some(&navigation.range(SourceSpan::new(0, navigation.source.len()))));
        let invalid = Json::Array(vec![object([("line", number(4)), ("character", number(2))])]);
        assert!(navigation.selections(&invalid).is_err());
        Ok(())
    }

    #[test]
    fn empty_document_has_no_symbols_or_folds_but_has_a_selection() -> TestResult {
        let navigation = Navigation::new("")?;
        assert!(array(&navigation.symbols("untitled:empty", true)).is_empty());
        assert!(array(&navigation.folds(4096)).is_empty());
        let positions = Json::Array(vec![object([("line", number(0)), ("character", number(0))])]);
        let selections = navigation.selections(&positions)?;
        assert_eq!(array(&selections).len(), 1);
        assert!(array(&selections)[0].get("parent").is_none());
        Ok(())
    }
}
