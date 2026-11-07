//! Logical PDF structure must survive pagination without losing source identity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use franken_markdown::{
    PageMargins, PageSize, PdfEmitOptions, PdfImageAsset, PdfOptions, PdfPageEmission, render_pdf,
    render_pdf_emitted,
};

#[derive(Debug)]
struct Node {
    id: usize,
    tag: String,
    parent: usize,
    body: String,
}

fn small_pages() -> PdfOptions {
    let mut opts = PdfOptions::default();
    opts.theme.page.size = PageSize {
        name: "accessibility-test",
        width_pt: 260.0,
        height_pt: 160.0,
    };
    opts.theme.page.margins = PageMargins {
        top_pt: 20.0,
        right_pt: 20.0,
        bottom_pt: 20.0,
        left_pt: 20.0,
    };
    opts
}

fn number_after(text: &str, needle: &str) -> usize {
    text.split(needle)
        .nth(1)
        .and_then(|tail| tail.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("missing number after {needle:?}: {text}"))
}

fn nodes(pdf: &[u8]) -> Vec<Node> {
    let text = String::from_utf8_lossy(pdf);
    let needle = " 0 obj\n<< /Type /StructElem /S /";
    let mut rest = text.as_ref();
    let mut result = Vec::new();
    while let Some(at) = rest.find(needle) {
        let id = rest[..at]
            .split_whitespace()
            .last()
            .unwrap()
            .parse()
            .unwrap();
        rest = &rest[at + needle.len()..];
        let end = rest.find("\nendobj").unwrap();
        let body = &rest[..end];
        result.push(Node {
            id,
            tag: body.split_whitespace().next().unwrap().to_owned(),
            parent: number_after(body, " /P "),
            body: body.to_owned(),
        });
        rest = &rest[end..];
    }
    result
}

fn mcr_pages(node: &Node) -> BTreeSet<usize> {
    node.body
        .split("/Type /MCR /Pg ")
        .skip(1)
        .map(|tail| tail.split_whitespace().next().unwrap().parse().unwrap())
        .collect()
}

fn tagged_count(nodes: &[Node], tag: &str) -> usize {
    nodes.iter().filter(|node| node.tag == tag).count()
}

fn assert_unique_content_owners(nodes: &[Node]) {
    let mut owners = BTreeMap::new();
    for node in nodes {
        for tail in node.body.split("/Type /MCR /Pg ").skip(1) {
            let page: usize = tail.split_whitespace().next().unwrap().parse().unwrap();
            let mcid = number_after(tail, " /MCID ");
            assert!(
                owners.insert((page, mcid), node.id).is_none(),
                "page {page}, MCID {mcid} must have exactly one logical owner"
            );
        }
    }
    assert!(!owners.is_empty());
    let pages: BTreeSet<_> = owners.keys().map(|&(page, _)| page).collect();
    for page in pages {
        let mcids: Vec<_> = owners
            .keys()
            .filter_map(|&(p, mcid)| (p == page).then_some(mcid))
            .collect();
        assert_eq!(mcids, (0..mcids.len()).collect::<Vec<_>>());
    }
}

fn assert_link_annotation_owners(pdf: &[u8], nodes: &[Node]) -> usize {
    let raw = String::from_utf8_lossy(pdf);
    let nums = raw
        .split_once("/Nums [ ")
        .unwrap()
        .1
        .split_once(" ] >>")
        .unwrap()
        .0;
    let mut annotations = BTreeSet::new();
    for node in nodes {
        for reference in node.body.split("/Type /OBJR /Pg ").skip(1) {
            let id = number_after(reference, " /Obj ");
            assert_eq!(
                node.tag, "Link",
                "annotation {id} needs a semantic Link owner"
            );
            assert!(
                annotations.insert(id),
                "annotation {id} has exactly one owner"
            );
            let object = raw
                .split_once(&format!("\n{id} 0 obj\n"))
                .unwrap()
                .1
                .split_once("\nendobj")
                .unwrap()
                .0;
            assert!(object.contains("/Subtype /Link "));
            let key = number_after(object, " /StructParent ");
            assert!(
                nums.contains(&format!(" {key} {} 0 R", node.id)),
                "annotation {id} must resolve back to its Link through the parent tree"
            );
        }
    }
    assert_eq!(annotations.len(), raw.matches("/Subtype /Link ").count());
    annotations.len()
}

#[test]
fn paragraphs_links_and_code_remain_single_logical_elements_across_pages() {
    let linked = "A linked paragraph with selectable words and stable reading order. ".repeat(25);
    let code = (0..40).map(|n| format!("record_{n}\n")).collect::<String>();
    let source = format!("[{linked}](https://example.com/guide)\n\n```text\n{code}```\n");
    let opts = small_pages();
    let pdf = render_pdf(&source, &opts).unwrap();
    let elements = nodes(&pdf);
    for tag in ["P", "Link", "Code"] {
        assert_eq!(tagged_count(&elements, tag), 1, "one source {tag}");
        let element = elements.iter().find(|node| node.tag == tag).unwrap();
        assert!(
            mcr_pages(element).len() > 1,
            "{tag} must actually span pages"
        );
    }
    let paragraph = elements.iter().find(|node| node.tag == "P").unwrap();
    let link = elements.iter().find(|node| node.tag == "Link").unwrap();
    assert_eq!(link.parent, paragraph.id);
    assert!(link.body.matches("/Type /OBJR").count() > 1);
    assert_unique_content_owners(&elements);

    let monolithic = render_pdf_emitted(
        &source,
        &opts,
        PdfEmitOptions {
            emission: PdfPageEmission::Monolithic,
            ..PdfEmitOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        pdf, monolithic,
        "emitter modes share the same logical hierarchy"
    );
    assert_eq!(pdf, render_pdf(&source, &opts).unwrap());
}

#[test]
fn continued_tables_keep_source_rows_and_headers_in_one_logical_table() {
    let mut source = "| [Name](https://example.com) | Value |\n|---|---|\n".to_owned();
    for row in 0..24 {
        source.push_str(&format!("| row{row} | value{row} |\n"));
    }
    source.push('\n');
    source.push_str("| Other | Table |\n|---|---|\n| distinct | cells |\n");
    let pdf = render_pdf(&source, &small_pages()).unwrap();
    let elements = nodes(&pdf);
    assert_eq!(tagged_count(&elements, "Table"), 2);
    assert_eq!(tagged_count(&elements, "TR"), 27);
    assert_eq!(tagged_count(&elements, "TH"), 4);
    assert_eq!(tagged_count(&elements, "TD"), 50);
    let tables: Vec<_> = elements.iter().filter(|node| node.tag == "Table").collect();
    assert_eq!(
        elements
            .iter()
            .filter(|node| node.tag == "TR" && node.parent == tables[0].id)
            .count(),
        25,
    );
    let first_header = elements.iter().find(|node| node.tag == "TH").unwrap();
    let links: Vec<_> = elements.iter().filter(|node| node.tag == "Link").collect();
    assert_eq!(
        links.len(),
        1,
        "one logical link survives header repetition"
    );
    let header_link = links[0];
    assert_eq!(header_link.parent, first_header.id);
    assert!(
        mcr_pages(header_link).len() > 1,
        "fixture must repeat its header"
    );
    assert!(header_link.body.matches("/Type /OBJR").count() > 1);
    assert!(!first_header.body.contains("/Type /OBJR"));
    assert!(assert_link_annotation_owners(&pdf, &elements) > 1);
    let raw = String::from_utf8_lossy(&pdf);
    assert!(raw.contains("EMC\n/Artifact <</Type /Pagination /Subtype /Header>> BDC"));
    assert_unique_content_owners(&elements);
}

#[test]
fn mixed_inline_links_share_one_owner_across_pages_and_keep_object_semantics() {
    let mut options = small_pages();
    options.image_assets = vec![PdfImageAsset::new(
        "status.svg",
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#007ec6"/></svg>"##.to_vec(),
    )];
    let label = "**Ready** ![Status](status.svg) $x_1$ and more words ".repeat(18);
    let source = format!(
        "Before [{label}](https://example.com/mixed) after.\n\nPlain ![Unlinked](status.svg) and $u$.\n"
    );
    let pdf = render_pdf(&source, &options).unwrap();
    let elements = nodes(&pdf);
    let links: Vec<_> = elements.iter().filter(|node| node.tag == "Link").collect();
    assert_eq!(
        links.len(),
        1,
        "styled text, figures and formulas share the source link"
    );
    let link = links[0];
    assert_eq!(
        elements
            .iter()
            .find(|node| node.id == link.parent)
            .unwrap()
            .tag,
        "P"
    );
    assert!(
        mcr_pages(link).len() > 1,
        "the mixed link actually spans pages"
    );
    for (tag, alt) in [("Figure", "Status"), ("Formula", "x_1")] {
        let linked: Vec<_> = elements
            .iter()
            .filter(|node| node.tag == tag && node.parent == link.id)
            .collect();
        assert_eq!(linked.len(), 18, "each {tag} occurrence remains distinct");
        for node in linked {
            assert!(node.body.contains(&format!("/Alt ({alt})")));
            assert!(node.body.contains("/BBox ["));
            assert_eq!(mcr_pages(node).len(), 1);
            assert!(!node.body.contains("/Type /OBJR"));
        }
        let unlinked: Vec<_> = elements
            .iter()
            .filter(|node| node.tag == tag && node.parent != link.id)
            .collect();
        assert_eq!(unlinked.len(), 1);
        assert_eq!(
            elements
                .iter()
                .find(|node| node.id == unlinked[0].parent)
                .unwrap()
                .tag,
            "P",
            "unlinked objects stay ordinary paragraph children"
        );
    }
    assert!(assert_link_annotation_owners(&pdf, &elements) >= 36);
    assert_unique_content_owners(&elements);
    let monolithic = render_pdf_emitted(&source, &options, PdfEmitOptions::monolithic()).unwrap();
    assert_eq!(pdf, monolithic);
    assert_eq!(pdf, render_pdf(&source, &options).unwrap());
}

#[test]
fn inline_object_links_resolve_destinations_without_reviving_missing_targets() {
    let options = PdfOptions {
        image_assets: vec![PdfImageAsset::new(
            "status.svg",
            br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#007ec6"/></svg>"##.to_vec(),
        )],
        ..PdfOptions::default()
    };
    let source = "Read [![Jump](status.svg) $x$](#destination), and [![Missing](status.svg) $y$](#missing).\n\n# Destination\n";
    let pdf = render_pdf(source, &options).unwrap();
    let elements = nodes(&pdf);
    let links: Vec<_> = elements.iter().filter(|node| node.tag == "Link").collect();
    assert_eq!(links.len(), 2);
    assert!(links[0].body.contains("/Type /OBJR"));
    assert!(!links[1].body.contains("/Type /OBJR"));
    for link in links {
        for tag in ["Figure", "Formula"] {
            let children: Vec<_> = elements
                .iter()
                .filter(|node| node.tag == tag && node.parent == link.id)
                .collect();
            assert_eq!(children.len(), 1);
            assert!(!children[0].body.contains("/Type /OBJR"));
        }
    }
    let count = assert_link_annotation_owners(&pdf, &elements);
    assert!(count >= 2);
    let raw = String::from_utf8_lossy(&pdf);
    for annotation in raw.split("/Subtype /Link ").skip(1) {
        let dictionary = annotation.split_once("\nendobj").unwrap().0;
        assert!(dictionary.contains("/Dest ["));
    }
    assert!(!raw.contains("/S /URI"));
    assert_unique_content_owners(&elements);
}

#[test]
fn nested_lists_and_quotes_keep_their_hierarchy_across_pages() {
    let mut source = String::new();
    for item in 0..8 {
        source.push_str(&format!(
            "> - Item {item} with {}\n>   - Nested item {item}.\n",
            "many words ".repeat(25)
        ));
    }
    let pdf = render_pdf(&source, &small_pages()).unwrap();
    let elements = nodes(&pdf);
    assert_eq!(tagged_count(&elements, "BlockQuote"), 1);
    assert_eq!(tagged_count(&elements, "L"), 9);
    assert_eq!(tagged_count(&elements, "LI"), 16);
    assert_eq!(tagged_count(&elements, "LBody"), 16);
    let quote = elements
        .iter()
        .find(|node| node.tag == "BlockQuote")
        .unwrap();
    let outer_list = elements.iter().find(|node| node.tag == "L").unwrap();
    assert_eq!(outer_list.parent, quote.id);
    assert_unique_content_owners(&elements);
}

#[test]
fn separately_laid_out_footnote_containers_never_merge_with_body_or_each_other() {
    let source = "> Body quotation.\n\nNotes[^one] and notes[^two].\n\n\
        [^one]:\n    > First note quotation.\n\n\
        [^two]:\n    > Second note quotation.\n";
    let pdf = render_pdf(source, &PdfOptions::default()).unwrap();
    let elements = nodes(&pdf);
    assert_eq!(tagged_count(&elements, "BlockQuote"), 3);
    assert_unique_content_owners(&elements);
}

#[test]
fn inline_formula_page_local_mcids_never_merge_different_occurrences() {
    let source = (0..40)
        .map(|index| format!("Term {index}: $x_{{{index}}}$  \n"))
        .collect::<String>();
    let pdf = render_pdf(&source, &small_pages()).unwrap();
    let elements = nodes(&pdf);
    assert_eq!(tagged_count(&elements, "P"), 1);
    assert_eq!(tagged_count(&elements, "Formula"), 40);
    for node in elements.iter().filter(|node| node.tag == "Formula") {
        assert_eq!(
            mcr_pages(node).len(),
            1,
            "each formula is a distinct occurrence"
        );
    }
    assert_unique_content_owners(&elements);
}

#[test]
fn small_type_footnote_tables_preserve_valid_adaptive_scaling_bounds() {
    // Seven indivisible wide headings force the adaptive path even at the
    // reduced note size. Short cells would never exercise the former panic.
    let heading = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnop";
    let source = format!(
        "A note[^table].\n\n[^table]:\n    | {} |\n    |---|---|---|---|---|---|---|\n    | one | two | three | four | five | six | seven |\n",
        [heading; 7].join(" | "),
    );
    let opts = PdfOptions {
        base_font_size: Some(6.0),
        table_font_size: Some(5.0),
        ..PdfOptions::default()
    };
    let pdf = render_pdf(&source, &opts).unwrap();
    let elements = nodes(&pdf);
    assert_eq!(tagged_count(&elements, "Table"), 1);
    assert_eq!(tagged_count(&elements, "TH"), 7);
    assert_eq!(tagged_count(&elements, "TD"), 7);
}
