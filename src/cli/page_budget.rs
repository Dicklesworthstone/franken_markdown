//! Enforce the native publication budget against the actual emitted PDF.
//!
//! The renderer's adaptive target remains best effort for library/browser
//! callers. Native commands promise a hard output budget and check it before
//! publishing any member of a combined render or writing to stdout.

pub(crate) struct Failure {
    pub code: &'static str,
    pub message: String,
}

pub(crate) fn check(bytes: &[u8], target: Option<usize>) -> Result<(), Failure> {
    let Some(target) = target else {
        return Ok(());
    };
    let pages = crate::book::emitted_pdf_page_count(bytes).ok_or_else(|| Failure {
        code: "pdf_page_count_unavailable",
        message: "could not read the emitted PDF page count".to_string(),
    })?;
    check_count(pages, Some(target))
}

/// The verifier measures the same final layout without serializing a PDF.
/// Keep its hard page-budget refusal identical to native publication.
pub(crate) fn check_count(pages: u64, target: Option<usize>) -> Result<(), Failure> {
    if let Some(target) = target
        && pages > target as u64
    {
        return Err(Failure {
            code: "page_budget_unmet",
            message: format!(
                "the document still needs {pages} pages after fitting, exceeding --fit-to-pages {target}; increase the budget or shorten the source"
            ),
        });
    }
    Ok(())
}
