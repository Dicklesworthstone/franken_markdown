//! Navigation regressions inspect the emitted PDF, including its real xref,
//! outline relationships, annotation destinations, and selectable TOC text.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::book::{BookInput, BookRenderer};
use franken_markdown::{
    Block, PdfASettings, PdfEmitOptions, PdfOptions, PdfPageEmission, parse_markdown, render_pdf,
    render_pdf_document, render_pdf_document_emitted, render_pdf_emitted,
};

struct Objects<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    xref: usize,
}

impl<'a> Objects<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|p| p == marker)
            .unwrap();
        let xref: usize = std::str::from_utf8(&bytes[at + marker.len()..])
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let mut lines = std::str::from_utf8(&bytes[xref..]).unwrap().lines();
        assert_eq!(lines.next(), Some("xref"));
        let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
        assert_eq!(header[0], "0");
        let count: usize = header[1].parse().unwrap();
        let mut offsets = vec![0];
        assert_eq!(lines.next().unwrap().trim(), "0000000000 65535 f");
        for id in 1..count {
            let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
            assert_eq!(&row[1..], &["00000", "n"]);
            let offset: usize = row[0].parse().unwrap();
            assert!(offset < xref);
            assert!(bytes[offset..].starts_with(format!("{id} 0 obj\n").as_bytes()));
            offsets.push(offset);
        }
        assert_eq!(lines.next(), Some("trailer"));
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
            .filter(|&at| at > start)
            .min()
            .unwrap_or(self.xref);
        let header = format!("{id} 0 obj\n");
        &self.bytes[start + header.len()..end]
    }

    fn dictionary(&self, id: usize) -> &'a str {
        let body = self.body(id);
        let end = body
            .windows(8)
            .position(|p| p == b"\nstream\n")
            .unwrap_or(body.len());
        std::str::from_utf8(&body[..end]).unwrap()
    }

    fn stream(&self, id: usize) -> String {
        let body = self.body(id);
        let at = body.windows(8).position(|p| p == b"\nstream\n").unwrap();
        let dict = self.dictionary(id);
        let length = integer(dict, "/Length ").unwrap();
        let payload = &body[at + 8..at + 8 + length];
        let bytes = if dict.contains("/FlateDecode") {
            franken_markdown::zlib_decompress(payload, 1 << 24).unwrap()
        } else {
            assert!(!dict.contains("/Filter"));
            payload.to_vec()
        };
        String::from_utf8(bytes).unwrap()
    }

    fn pages(&self) -> Vec<usize> {
        let catalog = self.dictionary(1);
        let pages = self.dictionary(integer(catalog, "/Pages ").unwrap());
        references(pages, "/Kids [")
    }
}

// The fixtures use simple ASCII dictionary values. Object boundaries always
// come from xref, never a search inside compressed or embedded font bytes.
fn integer(dictionary: &str, key: &str) -> Option<usize> {
    dictionary
        .split_once(key)?
        .1
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn references(dictionary: &str, key: &str) -> Vec<usize> {
    let Some((_, tail)) = dictionary.split_once(key) else {
        return Vec::new();
    };
    let fields: Vec<_> = tail.split_once(']').unwrap().0.split_whitespace().collect();
    assert_eq!(fields.len() % 3, 0);
    fields
        .chunks_exact(3)
        .map(|fields| {
            assert_eq!(&fields[1..], &["0", "R"]);
            fields[0].parse().unwrap()
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Destination {
    page: usize,
    y: f64,
}

fn destination(dictionary: &str) -> Option<Destination> {
    let fields: Vec<_> = dictionary
        .split_once("/Dest [")?
        .1
        .split_once(']')?
        .0
        .split_whitespace()
        .collect();
    assert_eq!(&fields[1..5], &["0", "R", "/XYZ", "null"]);
    assert_eq!(fields[6], "null");
    Some(Destination {
        page: fields[0].parse().unwrap(),
        y: fields[5].parse().unwrap(),
    })
}

#[derive(Debug)]
struct Bookmark {
    id: usize,
    title: String,
    destination: Destination,
}

fn outline(objects: &Objects<'_>) -> (usize, Vec<Bookmark>) {
    fn siblings(
        objects: &Objects<'_>,
        parent: usize,
        seen: &mut BTreeSet<usize>,
        out: &mut Vec<Bookmark>,
    ) -> usize {
        let parent_dictionary = objects.dictionary(parent);
        let Some(first) = integer(parent_dictionary, "/First ") else {
            assert!(integer(parent_dictionary, "/Last ").is_none());
            assert!(integer(parent_dictionary, "/Count ").is_none());
            return 0;
        };
        let mut current = Some(first);
        let mut previous = None;
        let mut descendants = 0;
        while let Some(id) = current {
            assert!(
                seen.insert(id),
                "outline cycle or multiply-owned child {id}"
            );
            let dictionary = objects.dictionary(id);
            assert_eq!(integer(dictionary, "/Parent "), Some(parent));
            assert_eq!(integer(dictionary, "/Prev "), previous);
            let title = dictionary
                .split_once("/Title (")
                .unwrap()
                .1
                .split_once(')')
                .unwrap()
                .0;
            out.push(Bookmark {
                id,
                title: title.to_owned(),
                destination: destination(dictionary).unwrap(),
            });
            descendants += 1 + siblings(objects, id, seen, out);
            previous = Some(id);
            current = integer(dictionary, "/Next ");
        }
        assert_eq!(integer(parent_dictionary, "/Last "), previous);
        assert_eq!(
            integer(parent_dictionary, "/Count "),
            Some(descendants),
            "all bookmark descendants are initially open"
        );
        descendants
    }

    let root = integer(objects.dictionary(1), "/Outlines ").unwrap();
    assert!(objects.dictionary(root).contains("/Type /Outlines "));
    let mut seen = BTreeSet::new();
    let mut bookmarks = Vec::new();
    siblings(objects, root, &mut seen, &mut bookmarks);
    let emitted: BTreeSet<_> = (1..objects.offsets.len())
        .filter(|&id| {
            let dictionary = objects.dictionary(id);
            dictionary.contains("/Title ") && destination(dictionary).is_some()
        })
        .collect();
    assert_eq!(seen, emitted, "no orphan bookmark objects");
    (root, bookmarks)
}

fn assert_tree(
    objects: &Objects<'_>,
    titles: &[&str],
    parents: &[Option<usize>],
    descendants: &[usize],
) -> Vec<Bookmark> {
    let (root, bookmarks) = outline(objects);
    assert_eq!(
        bookmarks
            .iter()
            .map(|b| b.title.as_str())
            .collect::<Vec<_>>(),
        titles
    );
    assert_eq!(parents.len(), bookmarks.len());
    assert_eq!(descendants.len(), bookmarks.len());
    for (index, bookmark) in bookmarks.iter().enumerate() {
        let dictionary = objects.dictionary(bookmark.id);
        let expected_parent = parents[index].map_or(root, |parent| bookmarks[parent].id);
        assert_eq!(
            integer(dictionary, "/Parent "),
            Some(expected_parent),
            "parent of {}",
            bookmark.title
        );
        assert_eq!(
            integer(dictionary, "/Count "),
            (descendants[index] != 0).then_some(descendants[index])
        );
    }
    bookmarks
}

fn annotation_destinations(objects: &Objects<'_>) -> Vec<Destination> {
    objects
        .pages()
        .into_iter()
        .flat_map(|page| {
            references(objects.dictionary(page), "/Annots [")
                .into_iter()
                .filter_map(|id| {
                    let dictionary = objects.dictionary(id);
                    assert!(dictionary.contains("/Subtype /Link"));
                    destination(dictionary)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn unicode_map(objects: &Objects<'_>, font: usize) -> BTreeMap<u16, String> {
    let cmap = objects.stream(integer(objects.dictionary(font), "/ToUnicode ").unwrap());
    let mut active = false;
    let mut map = BTreeMap::new();
    for line in cmap.lines() {
        if line.ends_with(" beginbfchar") {
            active = true;
            continue;
        }
        if line == "endbfchar" {
            active = false;
            continue;
        }
        if !active {
            continue;
        }
        let fields: Vec<_> = line.split_whitespace().collect();
        let glyph = u16::from_str_radix(fields[0].trim_matches(['<', '>']), 16).unwrap();
        let hex = fields[1].trim_matches(['<', '>']);
        let units: Vec<_> = hex
            .as_bytes()
            .chunks_exact(4)
            .map(|unit| u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16).unwrap())
            .collect();
        map.insert(glyph, String::from_utf16(&units).unwrap());
    }
    assert!(!map.is_empty());
    map
}

struct PaintedRun {
    page: usize,
    x: f64,
    y: f64,
    text: String,
}

fn painted_runs(objects: &Objects<'_>) -> Vec<PaintedRun> {
    let mut runs = Vec::new();
    for page in objects.pages() {
        let dictionary = objects.dictionary(page);
        let resources: Vec<_> = dictionary
            .split_once("/Font <<")
            .unwrap()
            .1
            .split_once(">>")
            .unwrap()
            .0
            .split_whitespace()
            .collect();
        assert_eq!(resources.len() % 4, 0);
        let fonts: BTreeMap<_, _> = resources
            .chunks_exact(4)
            .map(|entry| (entry[0], unicode_map(objects, entry[1].parse().unwrap())))
            .collect();
        let content = objects.stream(integer(dictionary, "/Contents ").unwrap());
        for line in content.lines().filter(|line| line.contains("BT /F")) {
            let spaced = line
                .replace('[', " [ ")
                .replace(']', " ] ")
                .replace('<', " <")
                .replace('>', "> ");
            let words: Vec<_> = spaced.split_whitespace().collect();
            let mut font = None;
            let (mut x, mut y) = (0.0, 0.0);
            let mut mode = 0;
            let mut text = String::new();
            for (index, word) in words.iter().enumerate() {
                match *word {
                    "Tf" => font = Some(&fonts[words[index - 2]]),
                    "Tr" => mode = words[index - 1].parse::<u8>().unwrap(),
                    "Tm" => {
                        x = words[index - 2].parse().unwrap();
                        y = words[index - 1].parse().unwrap();
                    }
                    "Tj" | "TJ" => {
                        if mode != 3 && !text.is_empty() {
                            runs.push(PaintedRun {
                                page,
                                x,
                                y,
                                text: std::mem::take(&mut text),
                            });
                        } else {
                            text.clear();
                        }
                    }
                    hex if hex.starts_with('<') && hex.ends_with('>') => {
                        let hex = hex.trim_matches(['<', '>']);
                        assert_eq!(hex.len() % 4, 0);
                        for glyph in hex.as_bytes().chunks_exact(4) {
                            let id = u16::from_str_radix(std::str::from_utf8(glyph).unwrap(), 16)
                                .unwrap();
                            text.push_str(&font.unwrap()[&id]);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    runs
}

/// Match painted titles to actual clickable rectangles, then independently
/// resolve their page numbers through the PDF's page tree and destinations.
fn assert_one_toc_row_per_heading(objects: &Objects<'_>, titles: &[&str]) {
    let (_, headings) = outline(objects);
    let runs = painted_runs(objects);
    let pages = objects.pages();
    for title in titles {
        let heading = headings
            .iter()
            .find(|heading| heading.title == *title)
            .unwrap();
        let toc: Vec<_> = runs
            .iter()
            .filter(|run| {
                run.text == *title
                    && references(objects.dictionary(run.page), "/Annots [")
                        .into_iter()
                        .any(|id| {
                            let dict = objects.dictionary(id);
                            if destination(dict) != Some(heading.destination) {
                                return false;
                            }
                            let rect: Vec<f64> = dict
                                .split_once("/Rect [")
                                .unwrap()
                                .1
                                .split_once(']')
                                .unwrap()
                                .0
                                .split_whitespace()
                                .map(|value| value.parse().unwrap())
                                .collect();
                            run.x >= rect[0] - 0.02
                                && run.x <= rect[2] + 0.02
                                && run.y >= rect[1]
                                && run.y <= rect[3]
                        })
            })
            .collect();
        assert_eq!(toc.len(), 1, "one document TOC entry for {title}");
        let printed_page = runs
            .iter()
            .filter(|run| run.page == toc[0].page && (run.y - toc[0].y).abs() < 0.01)
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .unwrap();
        let actual_page = pages
            .iter()
            .position(|&page| page == heading.destination.page)
            .unwrap()
            + 1;
        assert_eq!(
            printed_page.text,
            actual_page.to_string(),
            "TOC page for {title}"
        );
    }
}

#[test]
fn automatic_toc_is_owned_once_across_quotes_lists_and_page_notes() {
    let source = "# Main\n\n> ## Quoted\n> Quoted text.\n\n- ### Listed\n\n## End\n\nCitation.[^n]\n\n[^n]: ### Notehead\n\n    Note body.\n";
    for optimal_pagination in [false, true] {
        let options = PdfOptions {
            toc: true,
            optimal_pagination,
            ..PdfOptions::default()
        };
        let pdf = render_pdf(source, &options).unwrap();
        let objects = Objects::new(&pdf);
        assert_one_toc_row_per_heading(&objects, &["Main", "Quoted", "Listed", "End"]);
        let runs = painted_runs(&objects);
        assert_eq!(runs.iter().filter(|run| run.text == "Notehead").count(), 1);
        assert_eq!(
            pdf,
            render_pdf_emitted(
                source,
                &options,
                PdfEmitOptions {
                    emission: PdfPageEmission::Monolithic,
                    ..Default::default()
                }
            )
            .unwrap()
        );
    }
}

#[test]
fn nested_toc_markers_render_at_their_requested_location_with_both_flag_states() {
    for marker in [
        "> [[TOC:2]]\n",
        "- [[TOC:2]]\n",
        "- Intro.\n\n  [[TOC:2]]\n",
        "> - [[TOC:2]]\n",
    ] {
        let source = format!("# Main\n\n{marker}\n## End\n\n### Hidden\n");
        for toc in [false, true] {
            let pdf = render_pdf(
                &source,
                &PdfOptions {
                    toc,
                    ..PdfOptions::default()
                },
            )
            .unwrap();
            let objects = Objects::new(&pdf);
            assert_one_toc_row_per_heading(&objects, &["Main", "End"]);
            let runs = painted_runs(&objects);
            assert_eq!(runs.iter().filter(|run| run.text == "Hidden").count(), 1);
            assert!(!runs.iter().any(|run| run.text.contains("TOC")));
        }
    }
}

#[test]
fn automatic_toc_starts_at_a_nested_first_heading_and_keeps_default_placement_bytes() {
    for first in ["> ## First\n> Body.\n", "- ## First\n\n  Body.\n"] {
        let source = format!("{first}\n# Last\n");
        let pdf = render_pdf(
            &source,
            &PdfOptions {
                toc: true,
                ..PdfOptions::default()
            },
        )
        .unwrap();
        assert_one_toc_row_per_heading(&Objects::new(&pdf), &["First", "Last"]);
    }
    let ordinary = "# Main\n\nBody.\n\n## End\n";
    let automatic = render_pdf(
        ordinary,
        &PdfOptions {
            toc: true,
            ..PdfOptions::default()
        },
    )
    .unwrap();
    let explicit = render_pdf(&format!("[[TOC]]\n\n{ordinary}"), &PdfOptions::default()).unwrap();
    assert_eq!(
        automatic, explicit,
        "ordinary automatic placement remains the explicit front TOC"
    );
}

#[test]
fn a_leading_note_marker_survives_numbering_and_uses_body_destinations() {
    let source = "# Main\n\nCitation.[^n]\n\n[^n]: [[TOC:1]]\n\n    Note body.\n";
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let objects = Objects::new(&pdf);
    assert_one_toc_row_per_heading(&objects, &["Main"]);
    let runs = painted_runs(&objects);
    let painted: String = runs.iter().map(|run| run.text.as_str()).collect();
    assert!(painted.contains('¹'), "the note label remains visible");
    assert!(
        !painted.contains("TOC"),
        "numbering must not destroy marker recognition"
    );
}

#[test]
fn bookmarks_preserve_real_heading_hierarchy_and_skipped_levels() {
    let source = "#### Preface\n\n# Alpha\n\n### Deep\n\n###### Leaf\n\n## Middle\n\n#### Detail\n\n# Beta\n\n## Child\n";
    let options = PdfOptions::default();
    let pdf = render_pdf(source, &options).unwrap();
    let objects = Objects::new(&pdf);
    assert_tree(
        &objects,
        &[
            "Preface", "Alpha", "Deep", "Leaf", "Middle", "Detail", "Beta", "Child",
        ],
        &[
            None,
            None,
            Some(1),
            Some(2),
            Some(1),
            Some(4),
            None,
            Some(6),
        ],
        &[0, 4, 1, 0, 1, 0, 1, 0],
    );
    assert_eq!(pdf, render_pdf(source, &options).unwrap());
    assert_eq!(
        pdf,
        render_pdf_emitted(
            source,
            &options,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..Default::default()
            }
        )
        .unwrap()
    );
}

#[test]
fn alert_labels_do_not_capture_real_heading_fragments_or_duplicate_slugs() {
    let source = "> [!NOTE]\n> Context survives.\n\n[first](#note) [literal](#note-2) [duplicate](#note-3)\n\n# Note\n\nFirst section.\n\n# Note-2\n\nLiteral suffix.\n\n# Note\n\nDuplicate section.\n\n> [!WARNING]\n> Caution survives.\n\n# Warning\n\n[warning](#warning)\n";
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let objects = Objects::new(&pdf);
    let headings = assert_tree(
        &objects,
        &["Note", "Note-2", "Note", "Warning"],
        &[None; 4],
        &[0; 4],
    );
    assert_eq!(
        annotation_destinations(&objects),
        headings.iter().map(|h| h.destination).collect::<Vec<_>>()
    );
    let painted: String = painted_runs(&objects)
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(
        painted.contains("Context") && painted.contains("Caution"),
        "alert bodies remain painted"
    );
}

#[test]
fn alert_only_documents_have_no_invented_bookmarks() {
    let pdf = render_pdf("> [!NOTE]\n> Still visible.\n", &PdfOptions::default()).unwrap();
    let objects = Objects::new(&pdf);
    assert!(integer(objects.dictionary(1), "/Outlines ").is_none());
    let painted: String = painted_runs(&objects)
        .into_iter()
        .map(|run| run.text)
        .collect();
    assert!(painted.contains("Note") && painted.contains("Still"));
}

#[test]
fn heading_names_resembling_footnote_ids_keep_bookmarks_and_book_targets() {
    // This is an ordinary source heading, with no footnote definitions. Its
    // generated slug happens to resemble the renderer's private note IDs.
    let source = "# fmd fn 1\n\n[local](#fmd-fn-1)\n";
    let options = PdfOptions::default();
    let pdf = render_pdf(source, &options).unwrap();
    let objects = Objects::new(&pdf);
    let headings = assert_tree(&objects, &["fmd fn 1"], &[None], &[0]);
    assert_eq!(annotation_destinations(&objects), [headings[0].destination]);

    let book = BookRenderer::new(&[
        BookInput {
            path: "guide/prefix.md".into(),
            source: source.into(),
        },
        BookInput {
            path: "guide/end.md".into(),
            source: "# End\n\n[return](prefix.md#fmd-fn-1)\n".into(),
        },
    ])
    .unwrap();
    let pdf = book
        .render_pdf_with_options(&options, PdfASettings::OFF)
        .unwrap();
    let objects = Objects::new(&pdf);
    let headings = assert_tree(&objects, &["fmd fn 1", "End"], &[None, None], &[0, 0]);
    assert_eq!(
        annotation_destinations(&objects),
        [headings[0].destination; 2]
    );
}

#[test]
fn toc_printed_pages_and_links_use_real_headings_after_alerts() {
    let mut document = parse_markdown(
        "[[TOC]]\n\n> [!NOTE]\n> Early context.\n\n> [!WARNING]\n> Early caution.\n",
    );
    document.blocks.push(Block::PageBreak);
    document
        .blocks
        .extend(parse_markdown("# Note\n\nSection text.\n\n## Child\n\nChild text.\n").blocks);
    document.blocks.push(Block::PageBreak);
    document
        .blocks
        .extend(parse_markdown("# Warning\n\nLater section.\n").blocks);
    let options = PdfOptions::default();
    let pdf = render_pdf_document(&document, &options).unwrap();
    let objects = Objects::new(&pdf);
    let headings = assert_tree(
        &objects,
        &["Note", "Child", "Warning"],
        &[None, Some(0), None],
        &[1, 0, 0],
    );
    let pages = objects.pages();
    assert_eq!(
        pages.len(),
        3,
        "explicit chapter breaks exercise page numbers"
    );
    assert_eq!(
        headings
            .iter()
            .map(|h| h.destination.page)
            .collect::<Vec<_>>(),
        [pages[1], pages[1], pages[2]]
    );
    let runs = painted_runs(&objects);
    let annotations = references(objects.dictionary(pages[0]), "/Annots [");
    for heading in &headings {
        let title = runs
            .iter()
            .find(|run| {
                run.page == pages[0]
                    && run.text == heading.title
                    && annotations.iter().any(|&id| {
                        let dictionary = objects.dictionary(id);
                        if destination(dictionary) != Some(heading.destination) {
                            return false;
                        }
                        let rect: Vec<f64> = dictionary
                            .split_once("/Rect [")
                            .unwrap()
                            .1
                            .split_once(']')
                            .unwrap()
                            .0
                            .split_whitespace()
                            .map(|value| value.parse().unwrap())
                            .collect();
                        run.x >= rect[0] - 0.02
                            && run.x <= rect[2] + 0.02
                            && run.y >= rect[1]
                            && run.y <= rect[3]
                    })
            })
            .unwrap_or_else(|| panic!("no correctly linked TOC title for {}", heading.title));
        let page_number = runs
            .iter()
            .filter(|run| run.page == title.page && (run.y - title.y).abs() < 0.01)
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .unwrap();
        let expected = pages
            .iter()
            .position(|&page| page == heading.destination.page)
            .unwrap()
            + 1;
        assert_eq!(
            page_number.text,
            expected.to_string(),
            "printed TOC page for {}",
            heading.title
        );
    }
    assert_eq!(
        pdf,
        render_pdf_document_emitted(
            &document,
            &options,
            PdfEmitOptions {
                emission: PdfPageEmission::Monolithic,
                ..Default::default()
            }
        )
        .unwrap()
    );
}

#[test]
fn book_links_bind_across_alerts_duplicate_titles_and_nested_bookmarks() {
    let input = |path: &str, source: &str| BookInput {
        path: path.into(),
        source: source.into(),
    };
    let book = BookRenderer::new(&[
        input("guide/one.md", "# Same\n\n> [!NOTE]\n> Chapter context.\n\n## Detail\n\n[next](two.md#same) [local](#detail) [deep](two.md#leaf)\n"),
        input("guide/two.md", "> [!WARNING]\n> Second context.\n\n# Same\n\n### Leaf\n\n[back](one.md#same)\n"),
    ]).unwrap();
    let options = PdfOptions::default();
    for settings in [PdfASettings::OFF, PdfASettings::a2b_strict()] {
        let pdf = book.render_pdf_with_options(&options, settings).unwrap();
        let objects = Objects::new(&pdf);
        let headings = assert_tree(
            &objects,
            &["Same", "Detail", "Same", "Leaf"],
            &[None, Some(0), None, Some(2)],
            &[1, 0, 1, 0],
        );
        assert_eq!(
            annotation_destinations(&objects),
            [
                headings[2].destination,
                headings[1].destination,
                headings[3].destination,
                headings[0].destination
            ]
        );
        assert_ne!(headings[0].destination.page, headings[2].destination.page);
        assert!(
            !pdf.windows(b"fmd-book-pdf-nav/".len())
                .any(|bytes| bytes == b"fmd-book-pdf-nav/")
        );
        assert_eq!(
            pdf,
            book.render_pdf_with_options(&options, settings).unwrap()
        );
    }
}
