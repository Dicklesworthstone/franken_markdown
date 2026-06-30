//! Real-world corpus soak (bead grn.5.3).
//!
//! The fuzz + metamorphic suites generate synthetic inputs; this one renders the
//! project's OWN real Markdown — README, docs/, examples/, the wasm package docs —
//! to both HTML and PDF and asserts the cross-cutting invariants on every file:
//! no panic, well-formed self-contained HTML, a structurally-valid deterministic
//! PDF, and byte-identical re-renders. Real documents catch real emitter/layout
//! regressions that curated fixtures and random soup both miss.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use franken_markdown::{HtmlOptions, PdfOptions, render_html, render_pdf};

/// Collect every `.md` file under the repo, skipping build/output/vcs trees.
fn collect_markdown(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // Skip regenerable / vendored / VCS trees so the soak stays fast and
            // deterministic (and never recurses into rendered artifacts).
            if matches!(
                name.as_ref(),
                "target"
                    | ".git"
                    | ".beads"
                    | "node_modules"
                    | "pkg"
                    | "scratch"
                    | "beads_compliance_audit"
            ) || path.ends_with("tests/artifacts")
            {
                continue;
            }
            collect_markdown(&path, out);
        } else if name.ends_with(".md") || name.ends_with(".markdown") {
            out.push(path);
        }
    }
}

#[test]
fn every_repo_markdown_renders_to_valid_deterministic_html_and_pdf() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_markdown(&root, &mut files);
    files.sort();
    assert!(
        files.len() >= 5,
        "expected to find the project's own Markdown corpus, found {}",
        files.len()
    );

    let pdf_opts = PdfOptions {
        metadata_epoch_seconds: Some(1_700_000_000),
        ..PdfOptions::default()
    };

    let mut rendered = 0usize;
    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        let label = path.strip_prefix(&root).unwrap_or(path).display();

        // HTML: well-formed self-contained document.
        let html = render_html(&src, &HtmlOptions::default())
            .unwrap_or_else(|e| panic!("HTML render failed for {label}: {e}"));
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "{label}: HTML lacks doctype"
        );
        assert!(
            html.trim_end().ends_with("</html>"),
            "{label}: HTML not closed"
        );
        assert!(html.contains("<main"), "{label}: HTML has no <main> body");

        // PDF: well-formed, deterministic.
        let pdf = render_pdf(&src, &pdf_opts)
            .unwrap_or_else(|e| panic!("PDF render failed for {label}: {e}"));
        assert!(pdf.starts_with(b"%PDF-"), "{label}: PDF lacks header");
        assert!(
            String::from_utf8_lossy(&pdf).trim_end().ends_with("%%EOF"),
            "{label}: PDF lacks %%EOF trailer"
        );

        // Determinism: identical inputs + options yield identical bytes.
        let html2 = render_html(&src, &HtmlOptions::default()).unwrap();
        let pdf2 = render_pdf(&src, &pdf_opts).unwrap();
        assert_eq!(html, html2, "{label}: HTML render is non-deterministic");
        assert_eq!(pdf, pdf2, "{label}: PDF render is non-deterministic");

        rendered += 1;
    }
    eprintln!("corpus soak: rendered {rendered} real Markdown files to HTML + PDF");
}
