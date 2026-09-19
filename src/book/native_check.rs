//! Source-only native navigation checks. This module neither renders pages nor
//! loads images, fonts, stylesheets, presentation config, or publication paths.
//! Both read-only checking and strict publication use the same core report.

use std::io::{self, Write};
use std::process::ExitCode;

use super::{BookArgs, Failure, error, failure, inputs, json_string, stdout_line};
use crate::book::Book;
use crate::book::validation::{LinkReport, check_book_links};

/// Serialize the complete report before emitting anything. A validator or
/// report-budget failure must never look like a successful partial check.
fn inspect(book: &Book) -> Result<(LinkReport, String), Failure> {
    let report = check_book_links(book).map_err(|e| error(70, "book_check_error", e))?;
    let json = report.to_json().map_err(|e| error(70, "book_check_error", e))?;
    Ok((report, json))
}

pub(super) fn require_clean(book: &Book) -> Result<String, Failure> {
    let (report, json) = inspect(book)?;
    let findings = report.finding_count();
    if findings != 0 {
        return Err(error(65, "book_link_findings", format!(
            "{findings} broken expanded-book navigation reference(s); no publication was written. Run fmd book <DIR> --check-links --json for the complete report. This checks HTML navigation, not final PDF/EPUB conformance.",
        )));
    }
    Ok(json)
}

pub(super) fn run(args: &BookArgs, json_requested: bool) -> ExitCode {
    let loaded = match inputs::load_sources(&args.input, args.max_input_bytes) {
        Ok(loaded) => loaded,
        Err(message) => return failure(66, "book_error", &message, json_requested),
    };
    let (report, json) = match inspect(&loaded.book) {
        Ok(checked) => checked,
        Err(e) => return failure(e.code, e.tag, &e.message, json_requested),
    };
    let output = if json_requested {
        // Warnings are diagnostics, never additions to the portable core JSON
        // schema on stdout. They cannot be mistaken for navigation findings.
        write_warnings(&loaded.warnings, true, &mut io::stderr().lock())
            .and_then(|()| stdout_line(&json))
    } else {
        let mut stderr = io::stderr().lock();
        write_warnings(&loaded.warnings, false, &mut stderr)
            .and_then(|()| write_human(&report, &mut stderr))
            .and_then(|()| stderr.flush())
    };
    if output.is_err() { return ExitCode::from(74); }
    ExitCode::from(if report.finding_count() == 0 { 0 } else { 65 })
}

fn write_warnings(warnings: &[String], json: bool, out: &mut impl Write) -> io::Result<()> {
    for warning in warnings {
        if json {
            writeln!(out, "{{\"level\":\"warning\",\"code\":\"book_input_warning\",\"message\":{}}}", json_string(warning))?;
        } else {
            writeln!(out, "fmd: input warning: {warning:?}")?;
        }
    }
    out.flush()
}

fn write_human(report: &LinkReport, out: &mut impl Write) -> io::Result<()> {
    // This function receives a core-validated report whose counts and text
    // budgets were checked by inspect(), including overflow-safe serialization.
    let checked: usize = report.chapters.iter().map(|chapter| chapter.checked).sum();
    let external: usize = report.chapters.iter().map(|chapter| chapter.external).sum();
    let unchecked: usize = report.chapters.iter().map(|chapter| chapter.unchecked).sum();
    writeln!(out, "fmd: expanded HTML navigation: {} chapter(s), {checked} local reference(s) checked, {} finding(s).",
        report.chapters.len(), report.finding_count())?;
    for chapter in &report.chapters {
        for finding in &chapter.findings {
            // Source-derived text is escaped for terminals, never printed as
            // raw control sequences. Findings have chapter scope, not spans.
            writeln!(out, "fmd: {:?}: {}: {:?}: {}",
                chapter.path, finding.code, finding.destination, finding.message)?;
        }
    }
    writeln!(out, "fmd: not verified: {external} external URL(s), {unchecked} other local resource(s); no URL was fetched.")?;
    writeln!(out, "fmd: images, raw HTML IDs, and final PDF/EPUB conformance are outside this check. No publication outputs were written.")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::book::{BookInput, build_book};
    use crate::book::validation::{ChapterLinks, LinkFinding};

    fn book(source: &str) -> Book {
        build_book(&[BookInput { path: "start.md".into(), source: source.into() }]).unwrap()
    }

    #[test]
    fn strict_check_returns_the_unmodified_core_report_for_clean_navigation() {
        let book = book("# Start\n\n[Here](#start) [Remote](https://example.invalid/) [File](archive.zip)\n");
        let expected = check_book_links(&book).unwrap().to_json().unwrap();
        assert_eq!(require_clean(&book).unwrap_or_else(|e| panic!("{}", e.message)), expected);
        assert!(expected.contains("\"external\":1"));
        assert!(expected.contains("\"unchecked\":1"));
    }

    #[test]
    fn strict_check_distinguishes_findings_from_an_incomplete_check() {
        let e = require_clean(&book("# Start\n\n[Broken](#missing)\n")).err().unwrap();
        assert_eq!(e.code, 65);
        assert_eq!(e.tag, "book_link_findings");
        assert!(e.message.contains("--check-links --json"));
        let e = inspect(&Book { chapters: Vec::new() }).err().unwrap();
        assert_eq!(e.code, 70);
        assert_eq!(e.tag, "book_check_error");
    }

    #[test]
    fn human_output_escapes_source_controls_and_reports_unverified_categories() {
        let report = LinkReport { chapters: vec![ChapterLinks {
            path: "source\n\u{1b}[31m.md".into(), checked: 1, external: 2, unchecked: 3,
            findings: vec![LinkFinding {
                code: "missing_anchor", destination: "#x\r\u{1b}[2J".into(), message: "Missing anchor.",
            }],
        }] };
        let mut bytes = Vec::new();
        write_human(&report, &mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains('\r'));
        assert!(text.contains("source\\n"));
        assert!(text.contains("2 external URL(s), 3 other local resource(s)"));
        assert!(text.contains("No publication outputs were written"));
    }

    #[test]
    fn warning_diagnostics_escape_input_and_propagate_io_errors() {
        let mut bytes = Vec::new();
        write_warnings(&["symlink_skipped: a\n\u{1b}.md".into()], true, &mut bytes).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\\n\\u001b"));
        assert!(text.contains("\"code\":\"book_input_warning\""));
        struct Unwritable;
        impl Write for Unwritable {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> { Err(io::ErrorKind::PermissionDenied.into()) }
            fn flush(&mut self) -> io::Result<()> { Ok(()) }
        }
        assert!(write_warnings(&["warning".into()], false, &mut Unwritable).is_err());
        let report = check_book_links(&book("# Start\n")).unwrap();
        assert!(write_human(&report, &mut Unwritable).is_err());
    }
}
