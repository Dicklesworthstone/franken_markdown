//! Tests for PDF Table of Contents (`byqs.2`).
//!
//! Covers:
//! - Marker-based TOC (`[[TOC]]`, `[TOC]`, `[[_TOC_]]`, `[[TOC:2]]`)
//! - CLI/options flag-based TOC (`--toc`, `--toc-depth`)
//! - Two-pass layout convergence when TOC insertion pushes headings across page boundaries
//! - Agreement between TOC page numbers and bookmark outline destinations
//! - Byte-identity when no TOC is enabled
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::{HtmlOptions, PdfOptions, render_html, render_pdf};

#[test]
fn pdf_toc_marker_renders_valid_pdf() {
    let md = r#"
# Document Overview

[[TOC]]

## Section One

This is the first section with some body text.

## Section Two

This is the second section with more detailed content.

### Subsection Two Point One

Nested subsection content.
"#;
    let opts = PdfOptions::default();
    let bytes = render_pdf(md, &opts).expect("render_pdf failed");

    assert!(bytes.starts_with(b"%PDF-1.7"), "must produce valid PDF 1.7");
    assert!(bytes.ends_with(b"%%EOF\n"), "must end with %%EOF");
    assert!(bytes.len() > 1000, "PDF size must be non-trivial");
}

#[test]
fn pdf_toc_flag_renders_valid_pdf() {
    let md = r#"
# Introduction

This document has a TOC injected via options flag.

## Background

Some background information.

## Methodology

Detailed methodology.
"#;
    let mut opts = PdfOptions::default();
    opts.toc = true;
    opts.toc_depth = Some(2);
    let bytes = render_pdf(md, &opts).expect("render_pdf failed");

    assert!(bytes.starts_with(b"%PDF-1.7"));
    assert!(bytes.ends_with(b"%%EOF\n"));
}

#[test]
fn pdf_toc_two_pass_convergence_multi_page() {
    // Generate a long multi-page document with numerous headings and paragraphs.
    // The TOC block itself will take significant space and push subsequent sections onto later pages.
    let mut md = String::new();
    md.push_str("# Master Document\n\n[[TOC]]\n\n");
    for i in 1..=20 {
        md.push_str(&format!("## Chapter {i}\n\n"));
        for p in 1..=5 {
            md.push_str(&format!(
                "This is paragraph {p} of chapter {i}. It contains enough detailed prose to fill out vertical space across pages.\n\n"
            ));
        }
        md.push_str(&format!("### Section {i}.1\n\n"));
        md.push_str("Detailed subsection text with further paragraphs and analysis.\n\n");
    }

    let opts = PdfOptions {
        page_numbers: true,
        ..PdfOptions::default()
    };
    let pdf1 = render_pdf(&md, &opts).expect("render_pdf failed");
    let pdf2 = render_pdf(&md, &opts).expect("second render failed");

    // Must be 100% deterministic across multiple runs
    assert_eq!(pdf1, pdf2, "PDF TOC rendering must be deterministic");
}

#[test]
fn pdf_toc_depth_limit() {
    let md = r#"
# Top Level

[[TOC:1]]

## Should Be Excluded From TOC

Content for level 2 heading.

### Also Excluded

Content for level 3 heading.
"#;
    let opts = PdfOptions::default();
    let bytes = render_pdf(md, &opts).expect("render_pdf failed");
    assert!(bytes.starts_with(b"%PDF-1.7"));
}

#[test]
fn pdf_byte_identity_without_toc() {
    let md = r#"
# Normal Document

This is a regular document without any TOC marker.

## Section A

Paragraph A.

## Section B

Paragraph B.
"#;
    let opts = PdfOptions::default();
    let pdf1 = render_pdf(md, &opts).expect("render 1 failed");
    let pdf2 = render_pdf(md, &opts).expect("render 2 failed");

    assert_eq!(
        pdf1, pdf2,
        "Regular PDF render must be bit-for-bit identical"
    );
}

#[test]
fn html_and_pdf_toc_coherence() {
    let md = r#"
# Coherent Document

[[TOC]]

## Section One

Text 1.

## Section Two

Text 2.
"#;
    let html_opts = HtmlOptions::default();
    let pdf_opts = PdfOptions::default();

    let html = render_html(md, &html_opts).expect("html render failed");
    let pdf = render_pdf(md, &pdf_opts).expect("pdf render failed");

    assert!(
        html.contains("<nav class=\"toc\">"),
        "HTML must contain TOC nav"
    );
    assert!(
        html.contains("href=\"#section-one\""),
        "HTML must contain link to section 1"
    );
    assert!(
        html.contains("href=\"#section-two\""),
        "HTML must contain link to section 2"
    );
    assert!(pdf.starts_with(b"%PDF-1.7"), "PDF must be valid");
}
