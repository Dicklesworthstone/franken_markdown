//! Scientific conditions inside TeX text boxes survive the public HTML and
//! EPUB publication paths, including streaming and the browser-safe adapter.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{
    BookInput, HtmlOptions, build_book, parse_markdown, render_epub, render_html,
    render_html_document, render_html_document_to_writer,
};

const CONDITION: &str = r"x=1\text{ if $n_1>0$ and $m^2=4$}";
const STYLED: &str = r"\textbf{Case $k_j$: \textit{positive} \& <proof>}";

fn source() -> String {
    format!(
        "# Scientific conditions\n\nBefore $x_{{\\text{{angle \\alpha}}}}$ after.\n\n$$\n{CONDITION}\n$$\n\n$$\n{STYLED}\n$$\n"
    )
}

fn options() -> HtmlOptions {
    HtmlOptions {
        custom_css: Some(String::new()),
        ..HtmlOptions::default()
    }
}

fn assert_semantics(html: &str) {
    for required in [
        "<mi>α</mi>",
        "<msub><mi>n</mi><mn>1</mn></msub>",
        "<mo>&gt;</mo><mn>0</mn>",
        "<msup><mi>m</mi><mn>2</mn></msup>",
        "<mo>=</mo><mn>4</mn>",
        "<msub><mi>k</mi><mi>j</mi></msub>",
        "<mtext mathvariant=\"italic\">positive</mtext>",
        "&amp; &lt;proof&gt;",
    ] {
        assert!(
            html.contains(required),
            "missing {required} from published mathematics"
        );
    }
    assert!(!html.contains("<proof>"));
    assert!(!html.contains("<mtext> if  and </mtext>"));
    assert_eq!(html.matches("<math ").count(), 3);
    assert_eq!(html.matches("<mi>n</mi>").count(), 1);
    assert_eq!(html.matches("<mi>m</mi>").count(), 1);
}

#[test]
fn buffered_streamed_fragment_and_browser_adapter_keep_the_complete_mathematics() {
    let source = source();
    let document = parse_markdown(&source);
    let original = document.clone();
    let options = options();
    let html = render_html(&source, &options).unwrap();
    assert_semantics(&html);
    assert_eq!(html, render_html_document(&document, &options).unwrap());
    let mut streamed = Vec::new();
    render_html_document_to_writer(&document, &options, &mut streamed).unwrap();
    assert_eq!(streamed, html.as_bytes());
    assert_semantics(&franken_markdown::html::render_fragment(
        &document.blocks,
        &options,
    ));
    let browser_options = franken_markdown::wasm::WasmRenderOptions {
        custom_css: Some(String::new()),
        ..Default::default()
    };
    let browser = franken_markdown::wasm::render_html(&source, &browser_options).unwrap();
    assert_eq!(browser.bytes, html.as_bytes());
    assert_eq!(
        document, original,
        "publication must preserve the source AST"
    );
}

#[test]
fn tag_and_ordinary_text_boxes_share_the_semantic_renderer() {
    let source = format!("$$\n{STYLED}\n$$\n\n$$\nx\\tag*{{{STYLED}}}\n$$\n");
    let html = render_html(&source, &options()).unwrap();
    assert_eq!(html.matches("<msub><mi>k</mi><mi>j</mi></msub>").count(), 2);
    assert_eq!(
        html.matches("<mtext mathvariant=\"italic\">positive</mtext>")
            .count(),
        2
    );
    assert_eq!(html.matches("class=\"fmd-equation-tag\"").count(), 1);
    assert!(!html.contains("<proof>"));
}

#[test]
fn single_and_book_epub_keep_real_mathml_atoms_and_well_formed_xml() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // Python's ZIP inflater and XML parser independently inspect the package;
    // this optional check adds no runtime or crate dependency to rendering.
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("SKIP independent EPUB inspection: python3 is unavailable");
        return;
    }
    let source = source();
    let document = parse_markdown(&source);
    let book = build_book(&[BookInput {
        path: "conditions.md".into(),
        source,
    }])
    .unwrap();
    for archive in [
        render_epub(&document, &options()).unwrap(),
        franken_markdown::epub::render_book_epub(&book, &options()).unwrap(),
    ] {
        let mut child = Command::new("python3")
            .args(["-c", EPUB_INSPECTOR])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&archive).unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            String::from_utf8(result.stdout).unwrap().trim(),
            "complete scientific MathML"
        );
    }
}

const EPUB_INSPECTOR: &str = r#"
import io, sys, zipfile, xml.etree.ElementTree as ET
with zipfile.ZipFile(io.BytesIO(sys.stdin.buffer.read())) as archive:
    chapter = ET.fromstring(archive.read('OEBPS/chapter-1.xhtml'))
    ns = '{http://www.w3.org/1998/Math/MathML}'
    formulas = list(chapter.iter(ns + 'math'))
    assert len(formulas) == 3, 'all inline and display formulas must be MathML'
    names = [node.text for math in formulas for node in math.iter(ns + 'mi')]
    for symbol in ['α', 'n', 'm', 'k', 'j']:
        assert symbol in names, ('lost mathematical atom', symbol, names)
    assert sum(1 for math in formulas for node in math.iter(ns + 'msub')) >= 3
    assert sum(1 for math in formulas for node in math.iter(ns + 'msup')) == 1
    texts = [node for math in formulas for node in math.iter(ns + 'mtext')]
    assert any(node.text == 'positive' and node.get('mathvariant') == 'italic' for node in texts)
    assert any('& <proof>' in (node.text or '') for node in texts)
    assert not any(node.tag.endswith('proof') for node in chapter.iter())
    package = ET.fromstring(archive.read('OEBPS/content.opf'))
    items = package.findall('.//{http://www.idpf.org/2007/opf}item')
    assert any(item.get('href') == 'chapter-1.xhtml' and 'mathml' in item.get('properties', '').split() for item in items)
print('complete scientific MathML')
"#;
