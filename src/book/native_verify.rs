//! Read-only verification of the exact configured native book publication.

use std::io::{self, Write};
use std::process::ExitCode;

use super::{Failure, Receipt, error, inputs, json_string, preflight, publishing, stdout_line};
use crate::PdfOptions;

pub(super) const MAX_REPORT_BYTES: u64 = 8 * 1024 * 1024;

pub(super) struct Summary {
    pages: usize,
    pdf_findings: usize,
    navigation_findings: usize,
    parser_diagnostics: usize,
    details: Vec<String>,
    source_details: Vec<String>,
    source_diagnostics: Option<String>,
}

pub(super) fn inspect(
    loaded: inputs::LoadedBook,
    options: &PdfOptions,
    publication: &publishing::Publishing,
    max_report_bytes: u64,
) -> Result<Receipt, Failure> {
    // These are the already parsed chapters and their mapped source metadata.
    // Rebuilding them from the merged document would lose chapter destinations,
    // include origins, independent note identities and chapter language scopes.
    let report = crate::book::verify_book_pdf(
        &loaded.book,
        &loaded.source_diagnostics,
        options,
        publication.pdf_a,
    )
    .map_err(|value| {
        let tag = match &value {
            crate::RenderError::InvalidInput(message)
                if message.starts_with("book_pdf_verification_limit:") =>
            {
                "report_too_large"
            }
            crate::RenderError::Pagination(_) => value.code(),
            _ => "book_pdf_verify_error",
        };
        error(70, tag, value)
    })?;
    publication.check_page_budget(report.layout.page_count as u64)?;
    let json = report.to_json().map_err(|value| {
        let tag = if matches!(
            &value,
            crate::RenderError::InvalidInput(message)
                if message.starts_with("book_pdf_verification_limit:")
        ) {
            "report_too_large"
        } else {
            "book_pdf_report_error"
        };
        error(70, tag, value)
    })?;
    if json.len() as u64 > max_report_bytes {
        return Err(error(
            70,
            "report_too_large",
            format!("book PDF verification JSON exceeds the {max_report_bytes}-byte limit"),
        ));
    }
    let navigation = report.navigation.report();
    let details: Vec<_> = report
        .layout
        .findings
        .iter()
        .map(|finding| format!("{}: {}", finding.code, finding.detail))
        .collect();
    let summary = Summary {
        pages: report.layout.page_count,
        pdf_findings: report.layout.findings.len(),
        navigation_findings: navigation.finding_count(),
        parser_diagnostics: loaded.source_diagnostics.diagnostic_count(),
        details,
        source_details: preflight::source_warnings(&loaded.source_diagnostics),
        source_diagnostics: (!loaded.source_diagnostics.is_empty())
            .then(|| loaded.source_diagnostics.to_json()),
    };
    Ok(Receipt {
        json,
        chapters: loaded.book.chapters.len(),
        outputs: Vec::new(),
        warnings: loaded.warnings,
        verification: Some(summary),
    })
}

pub(super) fn emit(receipt: Receipt, json: bool) -> ExitCode {
    let Some(summary) = receipt.verification else {
        return ExitCode::from(70);
    };
    let output = (|| -> io::Result<()> {
        let mut stderr = io::stderr().lock();
        for warning in &receipt.warnings {
            if json {
                writeln!(
                    stderr,
                    "{{\"level\":\"warning\",\"code\":\"book_input_warning\",\"message\":{}}}",
                    json_string(warning)
                )?;
            } else {
                writeln!(stderr, "fmd: warning: {warning}")?;
            }
        }
        if json {
            if let Some(source) = &summary.source_diagnostics {
                writeln!(
                    stderr,
                    "{{\"level\":\"warning\",\"code\":\"book_source_diagnostics\",\"report\":{source}}}"
                )?;
            }
            stderr.flush()?;
            stdout_line(&receipt.json)
        } else {
            writeln!(
                stderr,
                "fmd: assembled PDF verification: {} chapter(s), {} page(s), {} finding(s), including {} source-navigation finding(s); {} parser diagnostic(s).",
                receipt.chapters,
                summary.pages,
                summary.pdf_findings,
                summary.navigation_findings,
                summary.parser_diagnostics,
            )?;
            for detail in &summary.details {
                writeln!(stderr, "fmd: {detail}")?;
            }
            for detail in &summary.source_details {
                writeln!(stderr, "fmd: source warning: {detail}")?;
            }
            stderr.flush()
        }
    })();
    if output.is_err() {
        return ExitCode::from(74);
    }
    let has_findings = summary.pdf_findings != 0
        || summary.navigation_findings != 0
        || summary.parser_diagnostics != 0;
    ExitCode::from(if has_findings { 65 } else { 0 })
}
