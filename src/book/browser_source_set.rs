//! Raw browser source-set admission without JsValue, so failures are testable
//! on the native target as well as at the generated WASM boundary.

use super::{BookWorkspace, book_inputs, checked_source_revision};
use crate::{RenderError, Result};

pub(super) fn replace(
    workspace: &mut BookWorkspace,
    paths: Vec<String>,
    sources: Vec<String>,
    include_paths: Vec<String>,
    include_sources: Vec<String>,
    expected_revision: f64,
) -> Result<String> {
    let invalid = |reason: &str| RenderError::InvalidInput(format!("book_source_set: {reason}"));
    let expected = checked_source_revision(expected_revision).map_err(invalid)?;
    if expected != workspace.source_revision() {
        return Err(invalid("stale source revision; refresh the source capture before editing"));
    }
    let chapters = book_inputs(paths, sources).map_err(invalid)?;
    let resources = if include_paths.is_empty() && include_sources.is_empty() {
        Vec::new()
    } else {
        book_inputs(include_paths, include_sources).map_err(invalid)?
    };
    workspace
        .replace_sources_at_revision(&chapters, &resources, expected)
        .map(|report| report.to_json())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::book::BookInput;

    fn workspace() -> BookWorkspace {
        BookWorkspace::from_sources(&[BookInput {
            path: "old.md".into(), source: "# Old".into(),
        }], &[]).unwrap()
    }

    #[test]
    fn raw_arrays_reach_the_complete_graph_transaction() {
        let mut book = workspace();
        let report = replace(&mut book, vec!["new.md".into()],
            vec!["# New\n\n{{#include snippet.txt}}".into()],
            vec!["snippet.txt".into()], vec!["Body".into()], 0.0).unwrap();
        assert!(report.contains("\"schema\":\"fmd-book-source-set-v1\""));
        assert!(report.contains("\"resource_count\":1"));
        assert_eq!(book.source_revision(), 1);
        assert_eq!(book.book().chapters[0].path, "new.md");
        assert!(format!("{:?}", book.book().chapters[0].doc).contains("Body"));
    }

    #[test]
    fn numeric_and_stale_revisions_precede_array_admission() {
        let mut book = workspace();
        for revision in [f64::NAN, f64::INFINITY, -1.0, 0.5, 4_294_967_296.0] {
            let error = replace(&mut book, vec![], vec![], vec![], vec![], revision).unwrap_err();
            assert!(error.to_string().contains("revision"));
            assert_eq!(book.source_revision(), 0);
        }
        let error = replace(&mut book, vec![], vec![], vec![], vec![], 1.0).unwrap_err();
        assert!(error.to_string().contains("stale source revision"));
        assert_eq!(book.book().chapters[0].title, "Old");
    }

    #[test]
    fn mismatched_arrays_leave_the_previous_book_usable() {
        let mut book = workspace();
        for (paths, sources, includes, texts) in [
            (vec!["new.md".into()], vec![], vec![], vec![]),
            (vec!["new.md".into()], vec!["# New".into()], vec!["part.txt".into()], vec![]),
            (vec!["new.md".into()], vec!["# New".into()], vec![], vec!["Body".into()]),
        ] {
            assert!(replace(&mut book, paths, sources, includes, texts, 0.0).is_err());
            assert_eq!(book.source_revision(), 0);
            assert_eq!(book.book().chapters[0].title, "Old");
        }
        assert!(replace(&mut book, vec!["recovered.md".into()],
            vec!["# Recovered".into()], vec![], vec![], 0.0).is_ok());
        assert_eq!(book.book().chapters[0].title, "Recovered");
    }
}
