//! Real PDF objects retain note ownership and citation relationships.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    BaselineGrid, Block, Document, Inline, PageMargins, PdfASettings, PdfEmitOptions, PdfOptions,
    PdfPageEmission, render_pdf, render_pdf_document_emitted, render_pdf_emitted, render_pdf_pdfa,
};
use std::collections::{BTreeMap, BTreeSet};

fn integer_after(text: &str, needle: &str) -> usize {
    text.split_once(needle)
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|number| number.trim_end_matches(">>").parse().ok())
        .unwrap_or_else(|| panic!("missing integer after {needle:?}: {text}"))
}

/// The writer uses direct dictionaries/arrays and generation-zero references.
/// Parse their boundaries exactly: substring matches cannot establish either
/// forward structure membership or a ParentTree number-key association.
#[derive(Debug, PartialEq, Eq)]
enum Value<'a> {
    Atom(&'a str),
    Reference(usize),
    Array(Vec<Value<'a>>),
    Dictionary(BTreeMap<&'a str, Value<'a>>),
}

impl<'a> Value<'a> {
    fn parse(source: &'a str) -> Self {
        let tokens = tokens(source);
        let mut remaining = tokens.as_slice();
        let value = Self::take(&mut remaining);
        assert!(remaining.is_empty(), "trailing PDF object tokens");
        value
    }

    fn take(tokens: &mut &[&'a str]) -> Self {
        let (first, rest) = tokens.split_first().expect("complete PDF value");
        *tokens = rest;
        match *first {
            "<<" => {
                let mut dictionary = BTreeMap::new();
                while tokens.first() != Some(&">>") {
                    let (key, rest) = tokens.split_first().expect("dictionary key");
                    let key = key.strip_prefix('/').expect("named dictionary key");
                    *tokens = rest;
                    assert!(
                        dictionary.insert(key, Self::take(tokens)).is_none(),
                        "duplicate PDF dictionary key"
                    );
                }
                *tokens = &tokens[1..];
                Self::Dictionary(dictionary)
            }
            "[" => {
                let mut values = Vec::new();
                while tokens.first() != Some(&"]") {
                    values.push(Self::take(tokens));
                }
                *tokens = &tokens[1..];
                Self::Array(values)
            }
            ">>" | "]" => panic!("unexpected PDF value terminator"),
            atom => {
                if tokens.get(1) == Some(&"R") {
                    let object = atom.parse().expect("object reference number");
                    assert_eq!(tokens[0], "0", "generation-zero writer reference");
                    *tokens = &tokens[2..];
                    Self::Reference(object)
                } else {
                    Self::Atom(atom)
                }
            }
        }
    }

    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Dictionary(values) => values.get(key),
            _ => None,
        }
    }

    fn required(&self, key: &str) -> &Self {
        self.get(key).unwrap_or_else(|| panic!("missing /{key}"))
    }

    fn atom(&self) -> &'a str {
        match self {
            Self::Atom(value) => value,
            _ => panic!("expected atomic PDF value: {self:?}"),
        }
    }

    fn named(&self, key: &str, name: &str) -> bool {
        self.get(key)
            .is_some_and(|value| matches!(value, Self::Atom(atom) if *atom == name))
    }

    fn number(&self) -> usize {
        self.atom().parse().expect("unsigned PDF integer")
    }

    fn reference(&self) -> usize {
        match self {
            Self::Reference(id) => *id,
            _ => panic!("expected indirect PDF reference: {self:?}"),
        }
    }

    fn array(&self) -> &[Self] {
        match self {
            Self::Array(values) => values,
            _ => panic!("expected PDF array: {self:?}"),
        }
    }

    fn kids(&self) -> &[Self] {
        self.get("K").map_or(&[], Self::array)
    }
}

#[derive(Debug)]
struct Node {
    id: usize,
    tag: String,
    parent: usize,
    body: String,
    children: Vec<usize>,
    mcrs: Vec<(usize, usize)>,
}

struct Pdf {
    nodes: Vec<Node>,
    marks: BTreeMap<(usize, usize), String>,
    paint: Vec<Vec<String>>,
    dictionaries: BTreeMap<usize, String>,
}

impl Pdf {
    fn new(bytes: &[u8]) -> Self {
        let objects = Objects::new(bytes);
        let mut pdf = Self {
            nodes: Vec::new(),
            marks: BTreeMap::new(),
            paint: Vec::new(),
            dictionaries: BTreeMap::new(),
        };
        for id in 1..objects.offsets.len() {
            if objects.offsets[id] == 0 {
                continue;
            }
            let body = objects.dictionary(id);
            pdf.dictionaries.insert(id, body.to_owned());
            let value = Value::parse(body);
            if value.named("Type", "/StructElem") {
                pdf.nodes.push(Node {
                    id,
                    tag: value
                        .required("S")
                        .atom()
                        .strip_prefix('/')
                        .unwrap()
                        .to_owned(),
                    parent: value.required("P").reference(),
                    body: body.to_owned(),
                    children: value
                        .kids()
                        .iter()
                        .filter_map(|kid| match kid {
                            Value::Reference(id) => Some(*id),
                            _ => None,
                        })
                        .collect(),
                    mcrs: value
                        .kids()
                        .iter()
                        .filter(|kid| kid.named("Type", "/MCR"))
                        .map(|mcr| {
                            (
                                mcr.required("Pg").reference(),
                                mcr.required("MCID").number(),
                            )
                        })
                        .collect(),
                });
            } else if value.named("Type", "/Page") {
                let content = objects.stream(value.required("Contents").reference());
                let (paint, marks) = page_content(&content);
                pdf.paint.push(paint);
                pdf.marks
                    .extend(marks.into_iter().map(|(mcid, tag)| ((id, mcid), tag)));
            }
        }
        pdf.assert_integrity();
        pdf
    }

    fn tagged(&self, tag: &str) -> Vec<&Node> {
        self.nodes.iter().filter(|node| node.tag == tag).collect()
    }

    fn children(&self, node: &Node) -> Vec<&Node> {
        node.children
            .iter()
            .map(|id| {
                self.nodes
                    .iter()
                    .find(|candidate| candidate.id == *id)
                    .unwrap()
            })
            .collect()
    }

    fn ancestor<'a>(&'a self, node: &'a Node, tag: &str) -> Option<&'a Node> {
        let mut node = node;
        for _ in 0..self.nodes.len() {
            if node.tag == tag {
                return Some(node);
            }
            node = self
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.parent)?;
        }
        panic!("cycle in structure parents");
    }

    fn note(&self, number: usize) -> &Node {
        self.tagged("Note")
            .into_iter()
            .find(|node| node.body.contains(&format!(" /ID (fmd-note-{number})")))
            .unwrap()
    }

    fn pages_under(&self, node: &Node) -> BTreeSet<usize> {
        self.nodes
            .iter()
            .filter(|candidate| {
                self.ancestor(candidate, &node.tag)
                    .is_some_and(|owner| owner.id == node.id)
            })
            .flat_map(|node| node.mcrs.iter().map(|&(page, _)| page))
            .collect()
    }

    fn assert_integrity(&self) {
        let objects = self
            .dictionaries
            .iter()
            .map(|(&id, body)| (id, Value::parse(body)))
            .collect::<BTreeMap<_, _>>();
        let roots = objects
            .iter()
            .filter(|(_, value)| value.named("Type", "/StructTreeRoot"))
            .collect::<Vec<_>>();
        assert_eq!(roots.len(), 1);
        let (&root_id, root) = roots[0];
        let catalog = objects
            .values()
            .find(|value| value.named("Type", "/Catalog"))
            .unwrap();
        assert_eq!(catalog.required("StructTreeRoot").reference(), root_id);
        let nodes = self
            .nodes
            .iter()
            .map(|node| (node.id, node))
            .collect::<BTreeMap<_, _>>();
        let mut pending = root
            .required("K")
            .array()
            .iter()
            .map(|child| (root_id, child.reference()))
            .collect::<Vec<_>>();
        let mut reached = BTreeSet::new();
        let mut owners = BTreeMap::new();
        let mut annotation_owners = BTreeMap::new();
        while let Some((parent, id)) = pending.pop() {
            assert!(reached.insert(id), "unique, acyclic forward /K membership");
            let node = nodes.get(&id).expect("/K names a structure element");
            assert_eq!(node.parent, parent, "forward /K agrees with reverse /P");
            for kid in objects[&id].kids() {
                if let Value::Reference(child) = kid {
                    pending.push((id, *child));
                } else if kid.named("Type", "/MCR") {
                    let page = kid.required("Pg").reference();
                    assert!(objects[&page].named("Type", "/Page"));
                    let mark = (page, kid.required("MCID").number());
                    assert_eq!(
                        self.marks.get(&mark),
                        Some(&node.tag),
                        "each forward MCR owns its actual marked content"
                    );
                    assert!(
                        owners.insert(mark, id).is_none(),
                        "each marked occurrence has one owner"
                    );
                } else {
                    assert!(kid.named("Type", "/OBJR"), "known structure kid type");
                    assert_eq!(node.tag, "Link");
                    let page = kid.required("Pg").reference();
                    assert!(objects[&page].named("Type", "/Page"));
                    let annotation = kid.required("Obj").reference();
                    assert!(objects[&annotation].named("Subtype", "/Link"));
                    assert!(
                        annotation_owners.insert(annotation, (page, id)).is_none(),
                        "each annotation has one forward OBJR owner"
                    );
                }
            }
        }
        assert_eq!(
            reached,
            nodes.keys().copied().collect(),
            "every structure element is reachable from the catalog root"
        );
        assert_eq!(
            owners.keys().copied().collect::<BTreeSet<_>>(),
            self.marks.keys().copied().collect()
        );

        let parent_tree = &objects[&root.required("ParentTree").reference()];
        let nums = parent_tree.required("Nums").array();
        assert_eq!(nums.len() % 2, 0, "complete ParentTree key/value pairs");
        let mut actual_parents = BTreeMap::new();
        let mut previous_key = None;
        for pair in nums.as_chunks::<2>().0 {
            let key = pair[0].number();
            assert!(previous_key.is_none_or(|previous| previous < key));
            assert!(actual_parents.insert(key, &pair[1]).is_none());
            previous_key = Some(key);
        }
        let mut expected_parents = BTreeMap::new();
        let mut page_annotations = BTreeMap::new();
        for (&page_id, page) in objects
            .iter()
            .filter(|(_, value)| value.named("Type", "/Page"))
        {
            let marks = owners
                .iter()
                .filter_map(|(&(page, mcid), &owner)| (page == page_id).then_some((mcid, owner)))
                .collect::<Vec<_>>();
            assert_eq!(
                marks.iter().map(|&(mcid, _)| mcid).collect::<Vec<_>>(),
                (0..marks.len()).collect::<Vec<_>>()
            );
            if marks.is_empty() {
                assert!(page.get("StructParents").is_none());
            } else {
                let key = page.required("StructParents").number();
                let value = Value::Array(
                    marks
                        .into_iter()
                        .map(|(_, owner)| Value::Reference(owner))
                        .collect(),
                );
                assert!(expected_parents.insert(key, value).is_none());
            }
            if let Some(annotations) = page.get("Annots") {
                for annotation in annotations.array() {
                    let id = annotation.reference();
                    assert!(objects[&id].named("Type", "/Annot"));
                    assert!(page_annotations.insert(id, page_id).is_none());
                }
            }
        }
        for (&annotation_id, annotation) in objects
            .iter()
            .filter(|(_, value)| value.named("Subtype", "/Link"))
        {
            let &(page, owner) = annotation_owners
                .get(&annotation_id)
                .expect("annotation has a logical Link owner");
            assert_eq!(
                page_annotations.get(&annotation_id),
                Some(&page),
                "OBJR /Pg names the actual containing /Annots page"
            );
            if let Some(annotation_page) = annotation.get("P") {
                assert_eq!(annotation_page.reference(), page);
            }
            let key = annotation.required("StructParent").number();
            assert!(
                expected_parents
                    .insert(key, Value::Reference(owner))
                    .is_none()
            );
        }
        assert_eq!(
            actual_parents.keys().copied().collect::<BTreeSet<_>>(),
            expected_parents.keys().copied().collect(),
            "ParentTree contains exactly the page and annotation keys"
        );
        for (key, expected) in &expected_parents {
            assert_eq!(
                actual_parents.get(key).copied(),
                Some(expected),
                "exact ParentTree owner at key {key}"
            );
        }

        let mut identifiers = BTreeMap::new();
        for node in &self.nodes {
            if let Some(id) = objects[&node.id].get("ID") {
                assert!(identifiers.insert(id.atom(), node.id).is_none());
            }
        }
        let mut resolved_identifiers = BTreeMap::new();
        if let Some(tree) = root.get("IDTree") {
            let names = tree.required("Names").array();
            assert_eq!(names.len() % 2, 0);
            for pair in names.as_chunks::<2>().0 {
                assert!(
                    resolved_identifiers
                        .insert(pair[0].atom(), pair[1].reference())
                        .is_none()
                );
            }
        }
        assert_eq!(
            identifiers, resolved_identifiers,
            "IDTree resolves exactly every unique table/note ID"
        );
        assert!(
            self.nodes
                .iter()
                .all(|node| objects[&node.id].get("Ref").is_none()),
            "PDF 1.7 must not introduce the PDF 2.0 association key"
        );
    }
}

/// Tokenize this writer's content syntax, retaining PDF strings/hex strings
/// atomically. This oracle tracks operators, not source formatting or lines:
/// legal marked-content changes may occur inside one continuing BT/ET.
fn tokens(content: &str) -> Vec<&str> {
    let bytes = content.as_bytes();
    let mut result = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at].is_ascii_whitespace() {
            at += 1;
            continue;
        }
        let start = at;
        match bytes[at] {
            b'(' => {
                at += 1;
                let mut depth = 1;
                while at < bytes.len() && depth != 0 {
                    match bytes[at] {
                        b'\\' => {
                            at += 2;
                            continue;
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    at += 1;
                }
            }
            b'<' if bytes.get(at + 1) != Some(&b'<') => {
                at += 1;
                while bytes[at] != b'>' {
                    at += 1;
                }
                at += 1;
            }
            b'<' | b'>' => at += 2,
            b'[' | b']' => at += 1,
            _ => {
                at += 1;
                while at < bytes.len()
                    && !bytes[at].is_ascii_whitespace()
                    && !b"()<>[]/".contains(&bytes[at])
                {
                    at += 1;
                }
            }
        }
        result.push(&content[start..at]);
    }
    result
}

fn page_content(content: &str) -> (Vec<String>, BTreeMap<usize, String>) {
    let mut operands = Vec::<&str>::new();
    let mut marks = BTreeMap::new();
    let mut open = Vec::new();
    let mut paint = Vec::new();
    let mut in_text = false;
    for token in tokens(content) {
        let operator = token.as_bytes()[0].is_ascii_alphabetic() || matches!(token, "'" | "\"");
        if !operator {
            operands.push(token);
            continue;
        }
        match token {
            "BDC" | "BMC" => {
                let tag = operands[0].trim_start_matches('/').to_owned();
                let mcid = operands
                    .iter()
                    .position(|&value| value == "/MCID")
                    .map(|index| operands[index + 1].parse::<usize>().unwrap());
                if let Some(mcid) = mcid {
                    assert!(marks.insert(mcid, tag.clone()).is_none());
                }
                open.push(tag);
            }
            "EMC" => {
                open.pop().expect("marked content must balance");
            }
            "TJ" | "Tj" => {
                assert!(in_text);
                assert!(
                    !open.is_empty(),
                    "painted text must be tagged or artifacted"
                );
                let values = if token == "TJ" {
                    assert_eq!(operands.first(), Some(&"["));
                    assert_eq!(operands.last(), Some(&"]"));
                    &operands[1..operands.len() - 1]
                } else {
                    assert_eq!(operands.len(), 1);
                    operands.as_slice()
                };
                for operand in values {
                    if operand.starts_with('<') && !operand.starts_with("<<") {
                        assert!(operand.ends_with('>'));
                        let hex = &operand.as_bytes()[1..operand.len() - 1];
                        assert!(hex.iter().all(u8::is_ascii_hexdigit));
                        let (glyphs, remainder) = hex.as_chunks::<4>();
                        assert!(
                            remainder.is_empty(),
                            "every admitted CID has four hex digits"
                        );
                        for glyph in glyphs {
                            paint.push(format!("glyph:{}", std::str::from_utf8(glyph).unwrap()));
                        }
                    } else {
                        assert_eq!(token, "TJ");
                        let adjustment = operand
                            .parse::<i32>()
                            .expect("this writer emits only hex CIDs and integer TJ adjustments");
                        if adjustment != 0 {
                            paint.push(format!("kern:{adjustment}"));
                        }
                    }
                }
            }
            _ => {
                if token == "BT" {
                    assert!(!in_text);
                    in_text = true;
                }
                if token == "ET" {
                    assert!(in_text);
                    in_text = false;
                }
                paint.push(format!("{token} {}", operands.join(" ")));
            }
        }
        operands.clear();
    }
    assert!(open.is_empty());
    assert!(!in_text);
    assert!(operands.is_empty());
    (paint, marks)
}

fn options() -> PdfOptions {
    let mut opts = PdfOptions {
        page_numbers: false,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = 320.0;
    opts.theme.page.size.height_pt = 260.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 24.0,
        right_pt: 24.0,
        bottom_pt: 24.0,
        left_pt: 24.0,
    };
    opts
}

#[test]
fn citations_labels_and_annotation_owners_are_source_roles_not_destination_guesses() {
    let source = "Claim[^a][^a] and [enclosing[^a]](https://example.org). [Ordinary](#fmd-fn-1). Undefined[^missing].\n\n[^a]: Complete body.\n";
    for backlinks in [false, true] {
        let opts = PdfOptions {
            footnote_backlinks: backlinks,
            ..options()
        };
        let bytes = render_pdf(source, &opts).unwrap();
        let pdf = Pdf::new(&bytes);
        assert_eq!(pdf.tagged("Note").len(), 1);
        assert_eq!(pdf.tagged("Reference").len(), 3);
        let note = pdf.note(1);
        assert_eq!(
            pdf.children(note)
                .iter()
                .map(|node| node.tag.as_str())
                .collect::<Vec<_>>(),
            ["Lbl", "P"]
        );
        let label = pdf.children(note)[0];
        assert_eq!(
            pdf.children(label)
                .iter()
                .filter(|node| node.tag == "Link")
                .count(),
            usize::from(backlinks)
        );
        let citations = pdf.tagged("Reference");
        assert_eq!(
            citations
                .iter()
                .filter(|node| pdf.children(node).iter().any(|child| child.tag == "Link"))
                .count(),
            2
        );
        assert_eq!(
            citations
                .iter()
                .filter(|node| pdf.ancestor(node, "Link").is_some())
                .count(),
            1
        );
        assert_eq!(bytes, render_pdf(source, &opts).unwrap());
    }
}

#[test]
fn rich_oversized_unreferenced_and_nested_notes_have_one_durable_container() {
    let long = "    A long complete paragraph keeps its note identity across page boundaries.\n\n"
        .repeat(18);
    let source = format!(
        "Short[^short] and long[^long].\n\n[^short]: Small body referencing another note[^nested].\n\n[^long]: Long body starts.\n\n{long}    ## Heading\n\n    > Quote\n    >\n    > - list body\n\n    ```rust\n    let value = 1;\n    ```\n\n    | Header | Other |\n    | --- | --- |\n    | Cell | Value |\n\n[^nested]: Nested target refers back[^short].\n\n[^unused]: Retained unreferenced note.\n"
    );
    for grid in [None, Some(BaselineGrid::from_points(14.0).unwrap())] {
        let mut opts = options();
        opts.footnote_backlinks = true;
        opts.theme.page.baseline_grid = grid;
        let pdf = Pdf::new(&render_pdf(&source, &opts).unwrap());
        assert_eq!(pdf.tagged("Note").len(), 4);
        assert_eq!(pdf.tagged("Reference").len(), 4);
        assert!(pdf.pages_under(pdf.note(2)).len() > 1);
        for tag in ["H2", "BlockQuote", "L", "Code", "Table"] {
            assert!(
                pdf.tagged(tag).iter().any(|node| pdf
                    .ancestor(node, "Note")
                    .is_some_and(|note| note.id == pdf.note(2).id)),
                "{tag}"
            );
        }
        for number in 1..=4 {
            let note = pdf.note(number);
            assert_eq!(
                pdf.children(note)
                    .iter()
                    .filter(|node| node.tag == "Lbl")
                    .count(),
                1
            );
        }
    }
}

#[test]
fn cached_tables_and_repeated_headers_keep_source_citation_occurrences() {
    let table = "| Header[^a] | Value |\n| --- | --- |\n| Same[^a] | one |\n\n";
    let rows = (0..35)
        .map(|index| format!("| Row {index} | value |\n"))
        .collect::<String>();
    let source = format!(
        "{table}{table}| Repeated[^a] | Value |\n| --- | --- |\n{rows}\n[^a]: Table note.\n"
    );
    let pdf = Pdf::new(&render_pdf(&source, &options()).unwrap());
    assert_eq!(pdf.tagged("Note").len(), 1);
    assert_eq!(pdf.tagged("Reference").len(), 5);
    assert_eq!(pdf.tagged("Table").len(), 3);
    assert!(
        pdf.tagged("Reference")
            .iter()
            .any(|node| pdf.pages_under(node).len() > 1)
    );
}

fn text(value: &str) -> Inline {
    Inline::Text(value.into())
}
fn reference() -> Inline {
    Inline::FootnoteRef { id: "n".into() }
}
fn link(destination: &str, content: Vec<Inline>) -> Inline {
    Inline::Link {
        dest: destination.into(),
        title: None,
        content,
    }
}

#[test]
fn semantic_boundaries_preserve_exact_shaped_paint_and_streaming_bytes() {
    let source = Document {
        blocks: vec![
            Block::Paragraph(vec![
                text("office "),
                reference(),
                reference(),
                text(" "),
                link(
                    "https://example.org",
                    vec![
                        text("Cafe\u{0301} x\u{0308} "),
                        reference(),
                        text(" affine"),
                    ],
                ),
            ]),
            Block::FootnoteDefinition {
                id: "n".into(),
                blocks: vec![Block::Paragraph(vec![text(
                    "office body Cafe\u{0301} x\u{0308} preserved.",
                )])],
            },
        ],
    };
    // Exact ordinary layout representation of that source. This calls the
    // existing low-level API, which has no typed source-note provenance.
    let prepared = Document {
        blocks: vec![
            Block::Paragraph(vec![
                text("office "),
                link("#fmd-fn-1", vec![text("¹")]),
                link("#fmd-fn-1", vec![text("¹")]),
                text(" "),
                link(
                    "https://example.org",
                    vec![text("Cafe\u{0301} x\u{0308} "), text("¹"), text(" affine")],
                ),
            ]),
            Block::FootnoteDefinition {
                id: "fmd-fn-1".into(),
                blocks: vec![Block::Paragraph(vec![
                    text("¹ "),
                    text("office body Cafe\u{0301} x\u{0308} preserved."),
                ])],
            },
        ],
    };
    let opts = options();
    let emit = PdfEmitOptions::default();
    let bytes = render_pdf_document_emitted(&source, &opts, emit).unwrap();
    let baseline =
        franken_markdown::pdf::render_with_emit(&prepared, &opts, PdfASettings::OFF, emit).unwrap();
    let tagged = Pdf::new(&bytes);
    assert_eq!(
        tagged.paint,
        Pdf::new(&baseline).paint,
        "all font, matrix, glyph, kern, color and decoration operators must match"
    );
    assert_eq!(tagged.tagged("Reference").len(), 3);
    let monolithic = render_pdf_document_emitted(
        &source,
        &opts,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..emit
        },
    )
    .unwrap();
    assert_eq!(bytes, monolithic);
}

#[test]
fn book_language_sections_and_pdfa_keep_notes_owned_by_their_definitions() {
    let inputs = ["de-DE", "en-US", "de-DE"].iter().enumerate().map(|(index, lang)| BookInput {
        path: format!("chapter-{index}.md"),
        source: format!("---\nlang: {lang}\n---\n# Chapter {index}\n\nClaim[^n].\n\n[^n]: Language-owned note {index}.\n"),
    }).collect::<Vec<_>>();
    let book = BookRenderer::new(&inputs).unwrap();
    let opts = options();
    let bytes = book
        .render_pdf_with_options(&opts, PdfASettings::OFF)
        .unwrap();
    let pdf = Pdf::new(&bytes);
    assert_eq!(pdf.tagged("Note").len(), 3);
    assert_eq!(pdf.tagged("Sect").len(), 3);
    for (number, lang) in ["de-DE", "en-US", "de-DE"].iter().enumerate() {
        let section = pdf.ancestor(pdf.note(number + 1), "Sect").unwrap();
        assert!(section.body.contains(&format!(" /Lang ({lang})")));
    }
    let report = book
        .render_pdf_with_options_reported(&opts, PdfASettings::OFF)
        .unwrap();
    assert_eq!(bytes, report.bytes);
    let archive = Pdf::new(
        &book
            .render_pdf_with_options(&opts, PdfASettings::a2b_strict())
            .unwrap(),
    );
    assert_eq!(archive.tagged("Note").len(), 3);
    assert_eq!(archive.tagged("Reference").len(), 3);
    let standalone = Pdf::new(
        &render_pdf_pdfa(
            "Claim[^n].\n\n[^n]: Note.\n",
            &opts,
            PdfASettings::a2b_strict(),
        )
        .unwrap(),
    );
    assert_eq!(standalone.tagged("Note").len(), 1);
}

#[test]
fn note_free_rendering_retains_low_level_bytes_and_ordinary_fragment_links() {
    let source = "# fmd-fn-1\n\n[Ordinary](#fmd-fn-1).\n\n- body\n\n| Head | Value |\n| --- | --- |\n| Cell | text |\n";
    let opts = options();
    let emit = PdfEmitOptions::default();
    let bytes = render_pdf_emitted(source, &opts, emit).unwrap();
    let document = franken_markdown::parse_markdown(source);
    assert_eq!(
        bytes,
        franken_markdown::pdf::render_with_emit(&document, &opts, PdfASettings::OFF, emit).unwrap()
    );
    let pdf = Pdf::new(&bytes);
    assert!(pdf.tagged("Note").is_empty());
    assert!(pdf.tagged("Reference").is_empty());
    assert_eq!(pdf.tagged("Link").len(), 1);
}

#[test]
fn fitting_and_equal_note_body_cache_hits_keep_owners_and_citations_distinct() {
    let body = "    Identical ordinary paragraph for cache reuse.\n\n    | Shared | Heading |\n    | --- | --- |\n    | Identical | cell |\n\n";
    let source = format!(
        "Claim[^a].\n\nClaim[^a].\n\nClaim[^b].\n\n{}\n\n[^a]: First label.\n\n{body}[^b]: Second label.\n\n{body}",
        "Fitting probe retains every note and source citation. ".repeat(65)
    );
    let opts = PdfOptions {
        fit_to_pages: Some(1),
        footnote_backlinks: true,
        ..options()
    };
    let bytes = render_pdf(&source, &opts).unwrap();
    let pdf = Pdf::new(&bytes);
    assert_eq!(pdf.tagged("Note").len(), 2);
    assert_eq!(pdf.tagged("Reference").len(), 3);
    let tables = pdf.tagged("Table");
    assert_eq!(tables.len(), 2);
    let owners = tables
        .iter()
        .map(|table| pdf.ancestor(table, "Note").unwrap().id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        owners,
        [pdf.note(1).id, pdf.note(2).id].into_iter().collect()
    );
    for number in [1, 2] {
        let paragraphs = pdf
            .nodes
            .iter()
            .filter(|node| {
                node.tag == "P"
                    && pdf
                        .ancestor(node, "Note")
                        .is_some_and(|note| note.id == pdf.note(number).id)
            })
            .count();
        assert_eq!(
            paragraphs, 2,
            "equal body paragraphs must remain in their defining note"
        );
    }
    assert_eq!(bytes, render_pdf(&source, &opts).unwrap());
}

#[test]
fn alert_leading_paragraph_clones_keep_actual_citation_provenance() {
    let source =
        "> [!NOTE]\n> Leading[^n] paragraph.\n>\n> Continued[^n] paragraph.\n\n[^n]: Alert note.\n";
    let pdf = Pdf::new(&render_pdf(source, &options()).unwrap());
    assert_eq!(pdf.tagged("Note").len(), 1);
    assert_eq!(pdf.tagged("Reference").len(), 2);
    assert!(
        pdf.tagged("Reference")
            .iter()
            .all(|node| pdf.ancestor(node, "BlockQuote").is_some())
    );
}

#[test]
fn authored_lookalikes_cannot_steal_first_citation_placement_or_return_targets() {
    let mut source = Document {
        blocks: vec![
            Block::Paragraph(vec![link("#fmd-fn-1", vec![text("Authored lookalike")])]),
            Block::PageBreak,
            Block::Paragraph(vec![text("Actual citation"), reference()]),
            Block::FootnoteDefinition {
                id: "n".into(),
                blocks: vec![Block::Paragraph(vec![text("Owned note evidence.")])],
            },
        ],
    };
    let opts = PdfOptions {
        footnote_backlinks: true,
        fit_to_pages: Some(2),
        ..options()
    };
    let pdf =
        Pdf::new(&render_pdf_document_emitted(&source, &opts, PdfEmitOptions::default()).unwrap());
    let citations = pdf.tagged("Reference");
    assert_eq!(citations.len(), 1);
    let citation_pages = pdf.pages_under(citations[0]);
    assert_eq!(citation_pages.len(), 1);
    let citation_page = *citation_pages.first().unwrap();
    assert_eq!(pdf.pages_under(pdf.note(1)), citation_pages);
    let label_link = pdf
        .tagged("Link")
        .into_iter()
        .find(|node| pdf.ancestor(node, "Lbl").is_some())
        .unwrap();
    let backlink = integer_after(&label_link.body, " /Obj ");
    assert_eq!(
        integer_after(&pdf.dictionaries[&backlink], " /Dest ["),
        citation_page
    );
    let page_ids = pdf
        .dictionaries
        .iter()
        .filter_map(|(&id, body)| body.contains("/Type /Page ").then_some(id))
        .collect::<Vec<_>>();
    assert_eq!(page_ids.len(), 2);
    assert_eq!(
        citation_page, page_ids[1],
        "the authored link on page one is not a citation"
    );

    let layer = franken_markdown::pdf::verification_text_layer_result(&source, &opts).unwrap();
    assert_eq!(layer.page_count, 2);
    assert!(
        !layer.pages[0]
            .runs
            .iter()
            .any(|run| run.text.contains("Owned note evidence."))
    );
    assert!(
        layer.pages[1]
            .runs
            .iter()
            .any(|run| run.text.contains("Owned note evidence."))
    );

    source.blocks[2] = Block::Paragraph(vec![text("Ordinary second page.")]);
    let unused =
        Pdf::new(&render_pdf_document_emitted(&source, &opts, PdfEmitOptions::default()).unwrap());
    assert!(unused.tagged("Reference").is_empty());
    assert!(
        unused
            .tagged("Link")
            .iter()
            .all(|node| unused.ancestor(node, "Lbl").is_none()),
        "an authored lookalike cannot manufacture a generated backlink"
    );

    // Existing low-level caller-prepared inputs intentionally retain their
    // legacy destination convention because no source provenance exists.
    source.blocks[3] = Block::FootnoteDefinition {
        id: "fmd-fn-1".into(),
        blocks: vec![Block::Paragraph(vec![text("¹ Legacy note evidence.")])],
    };
    let legacy = franken_markdown::pdf::verification_text_layer(&source, &opts).unwrap();
    assert!(
        legacy.pages[0]
            .runs
            .iter()
            .any(|run| run.text.contains("Legacy note evidence."))
    );
    let legacy_pdf =
        Pdf::new(&franken_markdown::pdf::render(&source, &opts, PdfASettings::OFF).unwrap());
    assert!(legacy_pdf.tagged("Note").is_empty());
    assert!(legacy_pdf.tagged("Reference").is_empty());
}

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let start = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap();
        let xref = std::str::from_utf8(&bytes[start + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let section: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(section[0], "0");
        let offsets = lines
            .take(section[1].parse().unwrap())
            .map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields[2] == "n" {
                    fields[0].parse().unwrap()
                } else {
                    0
                }
            })
            .collect();
        Self {
            bytes,
            offsets,
            xref,
        }
    }

    fn body(&self, id: usize) -> &'a [u8] {
        let start = self.offsets[id];
        let end = self
            .offsets
            .iter()
            .copied()
            .filter(|&offset| offset > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        let object = &self.bytes[start..end];
        assert!(object.starts_with(header.as_bytes()));
        &object[header.len()..]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let object = self.body(id);
        let end = object.iter().position(|&byte| byte == b'\n').unwrap();
        std::str::from_utf8(&object[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let object = self.body(id);
        let start = object
            .windows(8)
            .position(|part| part == b"\nstream\n")
            .unwrap()
            + 8;
        let dictionary = self.dictionary(id);
        let data = &object[start..start + integer_after(dictionary, "/Length ")];
        let data = if dictionary.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(data, 1 << 24).unwrap()
        } else {
            assert!(!dictionary.contains("/Filter"));
            data.to_vec()
        };
        String::from_utf8(data).unwrap()
    }
}
