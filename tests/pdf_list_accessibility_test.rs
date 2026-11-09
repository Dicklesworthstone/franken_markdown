//! Lists expose labels and bodies as separate, source-owned PDF structure.
//! The assertions decode emitted objects and font CMaps, not layout metadata.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    Align, BaselineGrid, Block, Document, Inline, List, ListItem, PageMargins, PdfASettings,
    PdfEmitOptions, PdfImageAsset, PdfOptions, Table, render_pdf, render_pdf_document,
    render_pdf_document_emitted, render_pdf_pdfa,
};

fn integer_after(text: &str, needle: &str) -> usize {
    text.split_once(needle)
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .and_then(|number| number.trim_end_matches(">>").parse().ok())
        .unwrap_or_else(|| panic!("missing integer after {needle:?}: {text}"))
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

    fn font_map(&self, id: usize) -> BTreeMap<u16, String> {
        let cmap = self.stream(integer_after(self.dictionary(id), "/ToUnicode "));
        let mut result = BTreeMap::new();
        let mut in_chars = false;
        for line in cmap.lines() {
            if line.ends_with(" beginbfchar") {
                in_chars = true;
            } else if line == "endbfchar" {
                in_chars = false;
            } else if in_chars {
                let (cid, text) = line.split_once(' ').unwrap();
                let cid = u16::from_str_radix(cid.trim_matches(['<', '>']), 16).unwrap();
                let units: Vec<_> = text
                    .trim_matches(['<', '>'])
                    .as_bytes()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|unit| {
                        u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap()
                    })
                    .collect();
                result.insert(cid, String::from_utf16(&units).unwrap());
            }
        }
        result
    }
}

#[derive(Debug)]
struct Node {
    id: usize,
    tag: String,
    parent: usize,
    dictionary: String,
    mcrs: Vec<(usize, usize)>,
}

#[derive(Debug, Default)]
struct Mark {
    tag: String,
    text: String,
}

struct TaggedPdf {
    nodes: Vec<Node>,
    marks: BTreeMap<(usize, usize), Mark>,
    pages: usize,
}

impl TaggedPdf {
    fn new(bytes: &[u8]) -> Self {
        let objects = Objects::new(bytes);
        let mut nodes = Vec::new();
        let mut marks = BTreeMap::new();
        let mut pages = 0;
        for id in 1..objects.offsets.len() {
            if objects.offsets[id] == 0 {
                continue;
            }
            let dictionary = objects.dictionary(id);
            if let Some((_, structure)) = dictionary.split_once("/Type /StructElem /S /") {
                let mcrs = dictionary
                    .split("/Type /MCR /Pg ")
                    .skip(1)
                    .map(|mcr| {
                        (
                            mcr.split_whitespace().next().unwrap().parse().unwrap(),
                            integer_after(mcr, " /MCID "),
                        )
                    })
                    .collect();
                nodes.push(Node {
                    id,
                    tag: structure.split_whitespace().next().unwrap().into(),
                    parent: integer_after(structure, " /P "),
                    dictionary: dictionary.into(),
                    mcrs,
                });
            } else if dictionary.contains("/Type /Page ") {
                pages += 1;
                let resources: Vec<_> = dictionary
                    .split_once("/Font <<")
                    .unwrap()
                    .1
                    .split_once(">>")
                    .unwrap()
                    .0
                    .split_whitespace()
                    .collect();
                let fonts: BTreeMap<_, _> = resources
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|entry| (entry[0], objects.font_map(entry[1].parse().unwrap())))
                    .collect();
                let content = objects.stream(integer_after(dictionary, "/Contents "));
                let mut owners = Vec::new();
                for line in content.lines() {
                    if line.ends_with(" BDC") || line.ends_with(" BMC") {
                        let owner = if line.contains("/MCID ") {
                            let mcid = integer_after(line, "/MCID ");
                            assert!(
                                marks
                                    .insert(
                                        (id, mcid),
                                        Mark {
                                            tag: line
                                                .split_whitespace()
                                                .next()
                                                .unwrap()
                                                .trim_start_matches('/')
                                                .into(),
                                            text: String::new(),
                                        }
                                    )
                                    .is_none(),
                                "MCIDs cannot repeat on one page"
                            );
                            Some(mcid)
                        } else {
                            None
                        };
                        owners.push(owner);
                    } else if line == "EMC" {
                        owners.pop().expect("marked content must be balanced");
                    } else if let Some(mcid) = owners.iter().rev().find_map(|&owner| owner) {
                        for run in line.split("BT ").skip(1) {
                            let (head, array) = run.split_once('[').unwrap();
                            let head: Vec<_> = head.split_whitespace().collect();
                            let tf = head.iter().position(|&token| token == "Tf").unwrap();
                            let font = &fonts[head[tf - 2]];
                            let array = array.split_once(']').unwrap().0;
                            for hex in array.split('<').skip(1) {
                                let hex = hex.split_once('>').unwrap().0;
                                for cid in hex.as_bytes().as_chunks::<4>().0 {
                                    let cid =
                                        u16::from_str_radix(std::str::from_utf8(cid).unwrap(), 16)
                                            .unwrap();
                                    marks
                                        .get_mut(&(id, mcid))
                                        .unwrap()
                                        .text
                                        .push_str(&font[&cid]);
                                }
                            }
                        }
                    }
                }
                assert!(owners.is_empty(), "page marked content must be balanced");
            }
        }
        Self {
            nodes,
            marks,
            pages,
        }
    }

    fn tagged(&self, tag: &str) -> Vec<&Node> {
        self.nodes.iter().filter(|node| node.tag == tag).collect()
    }

    fn children(&self, node: &Node) -> Vec<&Node> {
        let Some((_, rest)) = node.dictionary.split_once(" /K [ ") else {
            return Vec::new();
        };
        let references: Vec<_> = rest.split_once(']').unwrap().0.split_whitespace().collect();
        assert_eq!(
            references.len() % 3,
            0,
            "container children must be references"
        );
        references
            .as_chunks::<3>()
            .0
            .iter()
            .map(|reference| {
                assert_eq!(&reference[1..], ["0", "R"]);
                let id = reference[0].parse::<usize>().unwrap();
                let child = self.nodes.iter().find(|node| node.id == id).unwrap();
                assert_eq!(child.parent, node.id);
                child
            })
            .collect()
    }

    fn text(&self, node: &Node) -> String {
        node.mcrs
            .iter()
            .map(|key| self.marks[key].text.as_str())
            .collect()
    }

    fn labels(&self) -> Vec<String> {
        self.tagged("Lbl")
            .into_iter()
            .map(|node| self.text(node))
            .collect()
    }

    fn assert_list_structure(&self) {
        for item in self.tagged("LI") {
            let children = self.children(item);
            assert_eq!(
                children
                    .iter()
                    .map(|node| node.tag.as_str())
                    .collect::<Vec<_>>(),
                ["Lbl", "LBody"],
                "each source item has an ordered label/body pair"
            );
            assert!(!children[0].mcrs.is_empty());
            assert!(!children[0].dictionary.contains("/Type /OBJR"));
            for key in &children[0].mcrs {
                assert_eq!(self.marks[key].tag, "Lbl");
            }
        }
        assert_eq!(self.tagged("LI").len(), self.tagged("Lbl").len());
        let mut owners = BTreeSet::new();
        for node in &self.nodes {
            for key in &node.mcrs {
                assert!(
                    self.marks.contains_key(key),
                    "structure must reference painted content"
                );
                assert!(
                    owners.insert(*key),
                    "one logical owner for each marked occurrence"
                );
            }
        }
        assert_eq!(owners, self.marks.keys().copied().collect());
        for page in self
            .marks
            .keys()
            .map(|&(page, _)| page)
            .collect::<BTreeSet<_>>()
        {
            let mcids: Vec<_> = self
                .marks
                .keys()
                .filter_map(|&(p, mcid)| (p == page).then_some(mcid))
                .collect();
            assert_eq!(mcids, (0..mcids.len()).collect::<Vec<_>>());
        }
    }
}

fn options(width: f32, height: f32) -> PdfOptions {
    let mut opts = PdfOptions {
        page_numbers: false,
        ..PdfOptions::default()
    };
    opts.theme.page.size.width_pt = width + 72.0;
    opts.theme.page.size.height_pt = height + 72.0;
    opts.theme.page.margins = PageMargins {
        top_pt: 36.0,
        right_pt: 36.0,
        bottom_pt: 36.0,
        left_pt: 36.0,
    };
    opts
}

fn paragraph(text: &str) -> Block {
    Block::Paragraph(vec![Inline::Text(text.into())])
}

fn item(blocks: Vec<Block>) -> ListItem {
    ListItem { task: None, blocks }
}

fn list(ordered: bool, start: u64, items: Vec<ListItem>) -> Block {
    Block::List(List {
        ordered,
        start,
        tight: false,
        items,
    })
}

#[test]
fn bullets_ordinals_and_task_states_have_separate_selectable_labels() {
    let source = "- Body with literal [x], 4. and • text.\n- **Second** [linked item](https://example.com)\n\n---\n\n3. Third\n4. Fourth\n\n---\n\n- [x] Finished\n- [ ] Pending\n";
    let opts = options(440.0, 600.0);
    for pdf in [
        render_pdf(source, &opts).unwrap(),
        render_pdf_pdfa(source, &opts, PdfASettings::a2b_strict()).unwrap(),
    ] {
        let tagged = TaggedPdf::new(&pdf);
        tagged.assert_list_structure();
        assert_eq!(tagged.labels(), ["•", "•", "3.", "4.", "[x]", "[ ]"]);
        let lists = tagged.tagged("L");
        assert_eq!(lists.len(), 3);
        for (node, numbering) in lists.into_iter().zip(["Disc", "Decimal", "None"]) {
            assert!(
                node.dictionary
                    .contains(&format!("/A << /O /List /ListNumbering /{numbering} >>"))
            );
        }
        let paragraphs: Vec<_> = tagged
            .tagged("P")
            .into_iter()
            .map(|node| tagged.text(node))
            .collect();
        assert!(
            paragraphs
                .iter()
                .any(|text| text.contains("literal [x], 4. and • text."))
        );
        assert_eq!(tagged.tagged("Link").len(), 1);
        let link = tagged.tagged("Link")[0];
        assert_eq!(tagged.text(link), "linked item");
        assert!(link.dictionary.contains("/Type /OBJR"));
    }
}

#[test]
fn nonparagraph_first_children_and_empty_items_keep_labels_outside_their_bodies() {
    let doc = Document {
        blocks: vec![list(
            false,
            1,
            vec![
                item(vec![Block::Heading {
                    level: 2,
                    inlines: vec![Inline::Text("Heading".into())],
                }]),
                item(vec![Block::CodeBlock {
                    lang: None,
                    code: "code_body\nsecond_line".into(),
                }]),
                item(vec![Block::Table(Table {
                    align: vec![Align::None],
                    head: vec![vec![Inline::Text("Header".into())]],
                    rows: vec![vec![vec![Inline::Text("Cell".into())]]],
                })]),
                item(vec![Block::BlockQuote(vec![paragraph("Quoted body")])]),
                item(vec![Block::Paragraph(vec![Inline::Link {
                    dest: "https://example.com/figure".into(),
                    title: None,
                    content: vec![Inline::Image {
                        dest: "diagram.svg".into(),
                        title: None,
                        alt: "List figure".into(),
                    }],
                }])]),
                item(Vec::new()),
                item(vec![list(
                    true,
                    7,
                    vec![item(vec![paragraph("Nested item")])],
                )]),
            ],
        )],
    };
    let mut opts = options(280.0, 240.0);
    opts.image_assets.push(PdfImageAsset::new("diagram.svg", br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#2080c0"/></svg>"##.to_vec()));
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let tagged = TaggedPdf::new(&pdf);
    tagged.assert_list_structure();
    assert_eq!(tagged.labels(), ["•", "•", "•", "•", "•", "•", "•", "7."]);
    let outer = tagged.tagged("L")[0];
    let items = tagged.children(outer);
    assert_eq!(items.len(), 7);
    for (index, tag) in ["H2", "Code", "Table", "BlockQuote", "Link"]
        .into_iter()
        .enumerate()
    {
        let body = tagged.children(items[index])[1];
        assert_eq!(tagged.children(body)[0].tag, tag);
    }
    let figure = tagged.tagged("Figure")[0];
    assert!(figure.dictionary.contains("/Alt (List figure)"));
    assert!(figure.dictionary.contains("/BBox ["));
    assert_eq!(
        tagged
            .nodes
            .iter()
            .find(|node| node.id == figure.parent)
            .unwrap()
            .tag,
        "Link"
    );
    let empty_body = tagged.children(items[5])[1];
    assert!(tagged.children(empty_body).is_empty());
    assert!(empty_body.mcrs.is_empty());
    let nested = tagged.children(tagged.children(items[6])[1]);
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].tag, "L");
}

#[test]
fn continued_items_keep_one_label_and_body_across_pages_and_emitters() {
    // Monosyllabic prose keeps exact text comparisons independent of optional
    // discretionary hyphens while still requiring several physical pages.
    let prose =
        "A list has a link and its text runs on from page to page with each new line. ".repeat(35);
    let doc = Document {
        blocks: vec![Block::BlockQuote(vec![list(
            false,
            1,
            vec![item(vec![
                Block::Paragraph(vec![Inline::Link {
                    dest: "https://example.com/long".into(),
                    title: None,
                    content: vec![Inline::Text(prose.clone())],
                }]),
                list(true, 12, vec![item(vec![paragraph("Nested continuation")])]),
            ])],
        )])],
    };
    for grid in [false, true] {
        let mut opts = options(230.0, 150.0);
        opts.optimal_pagination = true;
        if grid {
            opts.theme.page.baseline_grid = Some(BaselineGrid::from_points(14.0).unwrap());
        }
        let pdf = render_pdf_document(&doc, &opts).unwrap();
        let tagged = TaggedPdf::new(&pdf);
        tagged.assert_list_structure();
        assert!(tagged.pages > 2);
        assert_eq!(tagged.labels(), ["•", "12."]);
        assert!(tagged.tagged("Lbl").iter().all(|node| node.mcrs.len() == 1));
        let link = tagged.tagged("Link")[0];
        assert!(
            link.mcrs
                .iter()
                .map(|&(page, _)| page)
                .collect::<BTreeSet<_>>()
                .len()
                > 2
        );
        let compact = |text: &str| {
            text.chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
        assert_eq!(compact(&tagged.text(link)), compact(&prose));
        assert_eq!(tagged.tagged("LI").len(), 2);
        assert_eq!(tagged.tagged("LBody").len(), 2);
        assert_eq!(
            pdf,
            render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap()
        );
        assert_eq!(pdf, render_pdf_document(&doc, &opts).unwrap());
    }
}

#[test]
fn wrapped_oversized_ordinals_keep_all_fragments_in_one_label() {
    let doc = Document {
        blocks: vec![list(true, u64::MAX, vec![item(vec![paragraph("Body")])])],
    };
    let mut opts = options(55.0, 140.0);
    opts.base_font_size = Some(20.0);
    opts.optimal_pagination = true;
    let pdf = render_pdf_document(&doc, &opts).unwrap();
    let tagged = TaggedPdf::new(&pdf);
    tagged.assert_list_structure();
    assert_eq!(tagged.labels(), [format!("{}.", u64::MAX)]);
    assert!(
        tagged.tagged("Lbl")[0].mcrs.len() > 1,
        "fixture must wrap the ordinal"
    );
    assert_eq!(tagged.tagged("P").len(), 1);
    assert_eq!(tagged.text(tagged.tagged("P")[0]), "Body");
    assert_eq!(
        pdf,
        render_pdf_document_emitted(&doc, &opts, PdfEmitOptions::monolithic()).unwrap()
    );
}

#[test]
fn mixed_task_lists_use_custom_numbering_without_reclassifying_literal_body_text() {
    let doc = Document {
        blocks: vec![list(
            true,
            9,
            vec![
                item(vec![paragraph("[x] is prose here")]),
                ListItem {
                    task: Some(false),
                    blocks: vec![paragraph("10. is prose here")],
                },
                ListItem {
                    task: Some(true),
                    blocks: Vec::new(),
                },
            ],
        )],
    };
    let tagged = TaggedPdf::new(&render_pdf_document(&doc, &options(300.0, 300.0)).unwrap());
    tagged.assert_list_structure();
    assert_eq!(tagged.labels(), ["9.", "[ ]", "[x]"]);
    assert!(
        tagged.tagged("L")[0]
            .dictionary
            .contains("/ListNumbering /None")
    );
    assert_eq!(
        tagged
            .tagged("P")
            .iter()
            .map(|node| tagged.text(node))
            .collect::<Vec<_>>(),
        ["[x] is prose here", "10. is prose here"]
    );
}
