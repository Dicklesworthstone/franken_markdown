//! Interactive documents must keep Markdown as data and preserve complete
//! renderer output, including notes deferred until after the block walk.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, html, parse_markdown, render_interactive_html};

#[test]
fn fragments_and_interactive_preview_preserve_nested_footnotes() {
    let source = "# Notes\n\nSee[^first], again[^first], and[^second].\n\n\
                  [^first]: First with *emphasis* and[^nested].\n\
                  [^second]: Second note.\n\
                  [^nested]: Nested note.\n\
                  [^unused]: Unreferenced note.\n";
    let doc = parse_markdown(source);
    let opts = HtmlOptions::default();
    let fragment = html::render_fragment(&doc.blocks, &opts);
    let full = html::render(&doc, &opts);
    let full_body = full
        .split_once("<main class=\"fmd\">\n")
        .unwrap()
        .1
        .split_once("</main>")
        .unwrap()
        .0;
    assert_eq!(
        fragment, full_body,
        "fragment must include all deferred notes"
    );
    assert_eq!(fragment.matches("class=\"footnotes\"").count(), 1);
    assert_eq!(fragment.matches("class=\"footnote-backref\"").count(), 3);
    assert_eq!(fragment.matches("id=\"fnref-1\"").count(), 1);
    for id in ["first", "second", "nested"] {
        assert!(fragment.contains(&format!("id=\"fn-{id}\"")));
        assert!(fragment.contains(&format!("href=\"#fn-{id}\"")));
    }
    assert!(fragment.contains("First with <em>emphasis</em>"));
    assert!(!fragment.contains("Unreferenced note."));

    let interactive = render_interactive_html(&doc, source, &opts);
    let preview = interactive
        .split_once("<div class=\"fmd-content\" id=\"fmd-content\">")
        .unwrap()
        .1
        .split_once("</div>\n  </main>")
        .unwrap()
        .0;
    assert_eq!(preview, fragment);
}

#[test]
fn fragment_does_not_create_notes_for_undefined_or_unused_references() {
    for source in ["Undefined[^missing].", "[^unused]: Unused."] {
        let doc = parse_markdown(source);
        let html = html::render_fragment(&doc.blocks, &HtmlOptions::default());
        assert!(!html.contains("class=\"footnotes\""), "{html}");
        if source.starts_with("Undefined") {
            assert!(html.contains("[^missing]"));
        }
    }
}

#[test]
fn source_data_cannot_enter_html_script_tokenizer_states() {
    let source = "\n\r\0\u{1f}\u{2028}\u{2029}é中😀 \\\" &lt; \\u003c\n\
                  </ScRiPt><script>globalThis.injected=true</script>\n\
                  <!--<script>comment and double escape</script>-->";
    let doc = parse_markdown(source);
    let opts = HtmlOptions {
        lang: Some("en\" onmouseover=\"alert(1)&<>".into()),
        ..HtmlOptions::default()
    };
    let html = render_interactive_html(&doc, source, &opts);
    let data = html
        .split_once("<script type=\"application/json\" id=\"fmd-raw-source\">")
        .unwrap()
        .1
        .split_once("</script>")
        .unwrap()
        .0;
    assert!(data.starts_with('"') && data.ends_with('"'));
    assert!(
        !data.contains('<'),
        "source must never enter a script tokenizer state"
    );
    assert!(!data.chars().any(|ch| ch <= '\u{1f}'));
    assert!(html.contains("<html lang=\"en&quot; onmouseover=&quot;alert(1)&amp;&lt;&gt;\">"));
    assert_eq!(html.to_ascii_lowercase().matches("</script").count(), 2);
}

/// Python's standard HTML and JSON parsers independently inspect the emitted
/// document. Node then executes the actual bundled app code against a DOM
/// adapter, including input, debounce, stats, toolbar and print interactions.
/// Neither tool is a dependency of the renderer; the external-runtime test
/// skips explicitly on developer machines where those tools are unavailable.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn interactive_html_parser_and_javascript_behavior() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let python = ["python3", "python"].into_iter().find(|name| {
        Command::new(name)
            .args(["-c", "import sys; assert sys.version_info.major == 3"])
            .output()
            .is_ok_and(|output| output.status.success())
    });
    let Some(python) = python else {
        eprintln!("SKIP interactive HTML parser/runtime proof: Python 3 unavailable");
        return;
    };

    let sources = [
        "",
        "\n\n# Leading blank lines\n\nUnicode: café 中文 😀.\n",
        "\r\nCRLF\rcarriage\nnull\0\u{1}\u{8}\u{b}\u{c}\u{1f}\u{2028}\u{2029}",
        "```html\n</ScRiPt><img id=\"injected\" src=x onerror=\"alert(1)\">\n```",
        "</SCRIPT \t><script>globalThis.fmdInjected=true</script>",
        "</script/><svg onload=\"alert(1)\"></svg>",
        "<!--<script>double-escaped state\n</script>-->",
        "</textarea><script>globalThis.fmdInjected=true</script>",
        r#"Literal \u003c/script> \n \r \" and &lt; &amp; &#13;."#,
    ];
    for (index, source) in sources.into_iter().enumerate() {
        let doc = parse_markdown(source);
        let html = render_interactive_html(
            &doc,
            source,
            &HtmlOptions {
                lang: Some("en\" onmouseover=\"alert(1)&<>".into()),
                title: Some("</title><script>globalThis.fmdInjected=true</script>".into()),
                ..HtmlOptions::default()
            },
        );
        let mut child = Command::new(python)
            .args(["-c", include_str!("support/interactive_html_check.py")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start independent HTML parser");
        let mut stdin = child.stdin.take().unwrap();
        writeln!(stdin, "{}", source.len()).unwrap();
        stdin.write_all(source.as_bytes()).unwrap();
        stdin.write_all(html.as_bytes()).unwrap();
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "case {index}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        eprintln!(
            "case={index} {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
    }
}

/// Generate real workspace manifests and execute their bundled image renderer
/// through the production controller. The Node adapter performs actual Blob
/// downloads and reopens source/manifest/preview for three edit/save generations.
#[test]
#[cfg(not(target_arch = "wasm32"))]
fn interactive_source_images_and_rejected_bindings_survive_edit_save_reopen() {
    use franken_markdown::PdfImageAsset;
    use std::io::Write;
    use std::process::{Command, Stdio};

    if Command::new("node").arg("--version").output().is_err() {
        eprintln!("SKIP native workspace image continuity proof: Node unavailable");
        return;
    }
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="6"><rect width="12" height="6" fill="red"/></svg>"#;
    let encoded = html::base64_encode(svg);
    let canonical = format!("data:image/svg+xml;base64,{encoded}");
    let percent: String = svg.iter().map(|byte| format!("%{byte:02X}")).collect();
    let escaped_base64: String = encoded.bytes().map(|byte| format!("%{byte:02X}")).collect();
    let binding = |assets| HtmlOptions {
        image_assets: assets,
        ..HtmlOptions::default()
    };
    let cases = [
        (
            "percent SVG source",
            format!("data:image/svg+xml,{percent}"),
            HtmlOptions::default(),
        ),
        (
            "charset base64 source",
            format!("DATA:IMAGE/SVG+XML;CHARSET=UTF-8;BASE64,{encoded}"),
            HtmlOptions::default(),
        ),
        (
            "percent base64 source",
            format!("data:image/svg+xml;base64,{escaped_base64}"),
            HtmlOptions::default(),
        ),
        (
            "invalid explicit override",
            canonical.clone(),
            binding(vec![PdfImageAsset::new(&canonical, Vec::new())]),
        ),
        (
            "invalid first override with valid later duplicate",
            canonical.clone(),
            binding(vec![
                PdfImageAsset::new(&canonical, b"not an image".as_slice()),
                PdfImageAsset::new(&canonical, svg.as_slice()),
            ]),
        ),
        (
            "valid first override with invalid later duplicate",
            canonical.clone(),
            binding(vec![
                PdfImageAsset::new(&canonical, svg.as_slice()),
                PdfImageAsset::new(&canonical, Vec::new()),
            ]),
        ),
        (
            "explicit image replaces malformed payload",
            "data:image/png;base64,invalid".to_owned(),
            binding(vec![PdfImageAsset::new(
                "data:image/png;base64,invalid",
                svg.as_slice(),
            )]),
        ),
        (
            "non-image source with a valid image binding",
            "data:text/html;base64,PHN2Zy8+".to_owned(),
            binding(vec![PdfImageAsset::new(
                "data:text/html;base64,PHN2Zy8+",
                svg.as_slice(),
            )]),
        ),
    ];
    for (name, destination, opts) in cases {
        let source = format!("# Portable document\n\n![Figure]({destination})\n");
        let workspace = render_interactive_html(&parse_markdown(&source), &source, &opts);
        let mut child = Command::new("node")
            .args([
                "tests/interactive_controller.test.mjs",
                "--native-image-workspace",
            ])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start native-generated workspace continuity check");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(workspace.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{name}: {}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
