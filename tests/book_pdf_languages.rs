//! Chapter language affects actual PDF typography and emitted reading ownership.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::book::{BookInput, BookRenderer, book_pdf_document, render_book_pdf};
use franken_markdown::{PdfASettings, PdfOptions, Profile, render_pdf, render_pdf_document};

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

    fn section<'a>(&'a self, node: &'a Node) -> Option<&'a Node> {
        let mut current = node;
        for _ in 0..self.nodes.len() {
            if current.tag == "Sect" {
                return Some(current);
            }
            current = self
                .nodes
                .iter()
                .find(|candidate| candidate.id == current.parent)?;
        }
        panic!("structure ancestry contains a cycle");
    }

    fn language<'a>(&self, section: &'a Node) -> &'a str {
        section
            .dictionary
            .split_once(" /Lang (")
            .unwrap()
            .1
            .split_once(')')
            .unwrap()
            .0
    }

    fn lines(&self, node: &Node) -> Vec<String> {
        node.mcrs
            .iter()
            .map(|key| self.marks[key].text.clone())
            .collect()
    }

    fn section_text(&self, section: &Node) -> String {
        self.nodes
            .iter()
            .filter(|node| {
                self.section(node)
                    .is_some_and(|owner| owner.id == section.id)
            })
            .map(|node| self.text(node))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn assert_owners(&self) {
        let mut owned = BTreeSet::new();
        for node in &self.nodes {
            for key in &node.mcrs {
                assert!(owned.insert(*key), "every mark has one structural owner");
                assert!(!self.marks[key].tag.is_empty());
            }
        }
        assert_eq!(owned, self.marks.keys().copied().collect());
    }
}

fn input(path: &str, language: Option<&str>, text: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: language.map_or_else(
            || text.to_string(),
            |lang| format!("---\nlang: {lang}\n---\n{text}"),
        ),
    }
}

fn options() -> PdfOptions {
    let mut options = PdfOptions {
        lang: Some("en-US".into()),
        ..PdfOptions::default()
    };
    options.theme.page.size.width_pt = 260.0;
    options.theme.page.size.height_pt = 480.0;
    options.theme.page.margins = franken_markdown::PageMargins {
        left_pt: 36.0,
        right_pt: 36.0,
        top_pt: 36.0,
        bottom_pt: 36.0,
    };
    options
}

const WORDS: &str = "verantwoordelijkheid onafhankelijk aardappel Donaudampfschifffahrt Computer constitution informatique extraordinary ";

#[test]
fn actual_chapter_breaks_use_language_and_shared_caches_keep_source_sections_separate() {
    let text = WORDS.repeat(8);
    let options = options();
    let languages = ["de-DE", "nl-NL", "de-DE", "en-US"];
    let book = BookRenderer::new(
        &languages
            .iter()
            .enumerate()
            .map(|(index, lang)| input(&format!("chapter-{index}.md"), Some(lang), &text))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let bytes = render_book_pdf(book.book(), &options).unwrap();
    let pdf = TaggedPdf::new(&bytes);
    let sections = pdf.tagged("Sect");
    assert_eq!(sections.len(), languages.len());
    let root = pdf.tagged("Document")[0];
    assert_eq!(
        pdf.children(root)
            .iter()
            .map(|node| node.id)
            .collect::<Vec<_>>(),
        sections.iter().map(|node| node.id).collect::<Vec<_>>()
    );
    let mut layouts = Vec::new();
    for (section, language) in sections.iter().zip(languages) {
        assert_eq!(pdf.language(section), language);
        let paragraphs: Vec<_> = pdf
            .tagged("P")
            .into_iter()
            .filter(|node| {
                pdf.section(node)
                    .is_some_and(|owner| owner.id == section.id)
            })
            .collect();
        assert_eq!(paragraphs.len(), 1);
        let actual = pdf.lines(paragraphs[0]);
        let selected = PdfOptions {
            lang: Some(language.into()),
            ..options.clone()
        };
        let standalone = TaggedPdf::new(&render_pdf(&text, &selected).unwrap());
        assert_eq!(
            actual,
            standalone.lines(standalone.tagged("P")[0]),
            "{language}"
        );
        layouts.push(actual);
    }
    assert_ne!(
        layouts[0], layouts[1],
        "fixture must distinguish German and Dutch breaking"
    );
    assert_ne!(
        layouts[0], layouts[3],
        "fixture must distinguish German and English breaking"
    );
    assert_eq!(
        layouts[0], layouts[2],
        "same-language cache reuse retains exact typography"
    );
    assert!(pdf.pages >= languages.len());
    pdf.assert_owners();
    assert_eq!(bytes, render_book_pdf(book.book(), &options).unwrap());
}

#[test]
fn normalized_blocks_navigation_landings_and_rich_notes_retain_defining_language() {
    let oversized = (0..28)
        .map(|index| format!("    GERMANEND row {index}  "))
        .collect::<Vec<_>>()
        .join("\n");
    let first = "# French\n\n[Second chapter](second.md) and [German note](second.md#fn-long).\n\nFRENCHBODY[^short].\n\n[^short]: FRENCHNOTE.\n";
    let second = format!(
        "GERMANBODY.\n\n<!-- a removed block -->\n\n<dl><dt>TERMONE</dt><dd>DESCONE</dd><dt>TERMTWO</dt><dd>DESCTWO</dd></dl>\n\n> QUOTEDTEXT\n>\n> - LISTTEXT\n\n| HEADER | VALUE |\n| --- | --- |\n| CELLTEXT | entry |\n\n```text\nCODETEXT\n```\n\n<!-- pagebreak -->\n\nAFTERBREAK[^long].\n\n[^long]: GERMANEND starts.\n{oversized}\n\n[^unused]: UNUSEDNOTE.\n"
    );
    let book = BookRenderer::new(&[
        input("first.md", Some("fr-CA"), first),
        input("second.md", Some("de-DE"), &second),
    ])
    .unwrap();
    let mut options = options();
    options.profile = Some(Profile::CommonMarkGfm);
    options.toc = true;
    options.footnote_backlinks = true;
    let bytes = book
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let pdf = TaggedPdf::new(&bytes);
    let sections = pdf.tagged("Sect");
    assert_eq!(sections.len(), 2);
    assert_eq!(pdf.language(sections[0]), "fr-CA");
    assert_eq!(pdf.language(sections[1]), "de-DE");
    let french = pdf.section_text(sections[0]);
    let german = pdf.section_text(sections[1]);
    for text in ["FRENCHBODY", "FRENCHNOTE"] {
        assert!(french.contains(text), "{french}");
    }
    for text in [
        "GERMANBODY",
        "TERMONE",
        "TERMTWO",
        "LISTTEXT",
        "CELLTEXT",
        "CODETEXT",
        "AFTERBREAK",
        "GERMANEND",
        "UNUSEDNOTE",
    ] {
        assert!(german.contains(text), "missing {text}: {german}");
        assert!(!french.contains(text));
    }
    assert!(
        pdf.tagged("H1")
            .iter()
            .any(|node| pdf.text(node) == "second"
                && pdf
                    .section(node)
                    .is_some_and(|owner| owner.id == sections[1].id)),
        "landing heading inherits chapter language"
    );
    assert!(
        pdf.tagged("P")
            .iter()
            .any(|node| pdf.section(node).is_none()),
        "global TOC rows remain in document language"
    );
    assert!(
        pdf.tagged("H2")
            .iter()
            .any(|node| pdf.text(node) == "Notes" && pdf.section(node).is_none()),
        "generated Notes heading remains global"
    );
    for tag in ["L", "BlockQuote", "Table", "Code"] {
        assert!(
            pdf.tagged(tag).iter().any(|node| pdf
                .section(node)
                .is_some_and(|owner| owner.id == sections[1].id)),
            "{tag}"
        );
    }
    pdf.assert_owners();
    let report = book
        .render_pdf_with_options_reported(&options, PdfASettings::OFF)
        .unwrap();
    assert_eq!(bytes, report.bytes);
    let archived = book
        .render_pdf_with_options(&options, PdfASettings::a2b_strict())
        .unwrap();
    let archived = TaggedPdf::new(&archived);
    assert_eq!(
        archived
            .tagged("Sect")
            .iter()
            .map(|node| archived.language(node))
            .collect::<Vec<_>>(),
        ["fr-CA", "de-DE"]
    );
}

#[test]
fn paragraph_and_table_cache_hits_retain_each_chapter_owner() {
    let body =
        "SAMEPARAGRAPH\n\n- SAMELIST\n\n| HEADER | VALUE |\n| --- | --- |\n| SAMECELL | entry |\n";
    let book = BookRenderer::new(&[
        input("a.md", Some("de"), body),
        input("b.md", Some("en"), body),
        input("c.md", Some("de"), body),
    ])
    .unwrap();
    let pdf = TaggedPdf::new(&render_book_pdf(book.book(), &options()).unwrap());
    let sections = pdf.tagged("Sect");
    assert_eq!(sections.len(), 3);
    for (section, language) in sections.iter().zip(["de", "en", "de"]) {
        assert_eq!(pdf.language(section), language);
        let text = pdf.section_text(section);
        for marker in ["SAMEPARAGRAPH", "SAMELIST", "SAMECELL"] {
            assert!(text.contains(marker));
        }
        for tag in ["L", "Lbl", "Table", "TH", "TD"] {
            assert!(
                pdf.tagged(tag).iter().any(|node| pdf
                    .section(node)
                    .is_some_and(|owner| owner.id == section.id)),
                "{language}: {tag}"
            );
        }
    }
    pdf.assert_owners();
}

#[test]
fn regional_unknown_and_blank_languages_keep_existing_fallback_rules() {
    let mut book = BookRenderer::new(&[
        input("a.md", Some("fr-CA"), "REGIONAL"),
        input("b.md", Some("x-private"), "UNKNOWN"),
        input("c.md", Some("en"), "BLANK"),
        input("d.md", None, "ABSENT"),
    ])
    .unwrap()
    .book()
    .clone();
    book.chapters[2].frontmatter.as_mut().unwrap().lang = Some("  \t  ".into());
    let options = PdfOptions {
        lang: Some("nl-NL".into()),
        ..options()
    };
    let pdf = TaggedPdf::new(&render_book_pdf(&book, &options).unwrap());
    assert_eq!(
        pdf.tagged("Sect")
            .iter()
            .map(|node| pdf.language(node))
            .collect::<Vec<_>>(),
        ["fr-CA", "x-private", "nl-NL", "nl-NL"]
    );
    let catalog = Objects::new(&render_book_pdf(&book, &options).unwrap())
        .dictionary(1)
        .to_owned();
    assert!(catalog.contains("/Lang (nl-NL)"));
}

#[test]
fn books_without_language_overrides_keep_the_ordinary_pdf_bytes() {
    let mut book = BookRenderer::new(&[
        input(
            "a.md",
            None,
            "# Alpha\n\nFirst[^note].\n\n[^note]: A complete note.",
        ),
        input(
            "b.md",
            Some("en"),
            "# Beta\n\nSecond.\n\n| H | V |\n| --- | --- |\n| C | D |",
        ),
    ])
    .unwrap()
    .book()
    .clone();
    book.chapters[1].frontmatter.as_mut().unwrap().lang = None;
    let options = options();
    let bytes = render_book_pdf(&book, &options).unwrap();
    let merged = book_pdf_document(&book);
    assert_eq!(bytes, render_pdf_document(&merged, &options).unwrap());
    assert!(TaggedPdf::new(&bytes).tagged("Sect").is_empty());
    book.chapters[1].frontmatter.as_mut().unwrap().lang = Some(" \t ".into());
    assert_eq!(bytes, render_book_pdf(&book, &options).unwrap());
}

#[test]
fn page_fitting_trials_use_the_same_chapter_language_as_final_layout() {
    let text = WORDS.repeat(30);
    let book = BookRenderer::new(&[input("dutch.md", Some("nl-NL"), &text)]).unwrap();
    let mut options = options();
    options.fit_to_pages = Some(1);
    options.theme.page.size.height_pt = 660.0;
    let pdf = TaggedPdf::new(&render_book_pdf(book.book(), &options).unwrap());
    let standalone_options = PdfOptions {
        lang: Some("nl-NL".into()),
        ..options.clone()
    };
    let standalone = TaggedPdf::new(&render_pdf(&text, &standalone_options).unwrap());
    assert_eq!(pdf.pages, standalone.pages);
    assert_eq!(
        pdf.lines(pdf.tagged("P")[0]),
        standalone.lines(standalone.tagged("P")[0])
    );
    assert_eq!(pdf.language(pdf.tagged("Sect")[0]), "nl-NL");
}

#[test]
fn chapter_language_uses_the_existing_pdf_string_encoding() {
    let mut book = BookRenderer::new(&[input("a.md", Some("en"), "BODY")])
        .unwrap()
        .book()
        .clone();
    for (language, encoded) in [
        (
            "de) /OpenAction << /S /JavaScript >> (\\",
            r"(de\) /OpenAction << /S /JavaScript >> \(\\)",
        ),
        ("français", "<FEFF006600720061006E00E7006100690073>"),
    ] {
        book.chapters[0].frontmatter.as_mut().unwrap().lang = Some(language.into());
        let pdf = TaggedPdf::new(&render_book_pdf(&book, &options()).unwrap());
        assert!(
            pdf.tagged("Sect")[0]
                .dictionary
                .contains(&format!(" /Lang {encoded}"))
        );
    }
}

#[test]
fn nested_toc_keeps_one_source_list_and_overrides_only_generated_text_language() {
    let book = BookRenderer::new(&[input(
        "one.md",
        Some("de-DE"),
        "# Heading\n\n- [TOC]\n\n- ORDINARY\n\n> QUOTETEXT\n>\n> [TOC]\n",
    )])
    .unwrap();
    let pdf = TaggedPdf::new(&render_book_pdf(book.book(), &options()).unwrap());
    assert_eq!(pdf.tagged("Sect").len(), 1);
    let section = pdf.tagged("Sect")[0];
    assert_eq!(pdf.language(section), "de-DE");
    assert_eq!(
        pdf.tagged("L").len(),
        1,
        "one source list cannot split across language scopes"
    );
    assert_eq!(pdf.tagged("LI").len(), 2);
    assert_eq!(pdf.tagged("BlockQuote").len(), 1);
    for tag in ["L", "LI", "BlockQuote"] {
        for node in pdf.tagged(tag) {
            assert_eq!(pdf.section(node).unwrap().id, section.id);
        }
    }
    let generated: Vec<_> = pdf
        .tagged("P")
        .into_iter()
        .filter(|node| node.dictionary.contains(" /Lang (en-US)"))
        .collect();
    assert_eq!(
        generated.len(),
        2,
        "both nested TOCs explicitly use the document language"
    );
    for node in generated {
        assert_eq!(pdf.section(node).unwrap().id, section.id);
    }
    let ordinary: Vec<_> = pdf
        .tagged("P")
        .into_iter()
        .filter(|node| {
            let text = pdf.text(node);
            text.contains("ORDINARY") || text.contains("QUOTETEXT")
        })
        .collect();
    assert_eq!(ordinary.len(), 2);
    for node in ordinary {
        assert!(
            !node.dictionary.contains(" /Lang "),
            "ordinary text inherits its chapter"
        );
    }
    pdf.assert_owners();
}
