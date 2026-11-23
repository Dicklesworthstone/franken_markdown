//! Source-dialect ownership for browser search and revision review.

use std::borrow::Cow;

use wasm_bindgen::prelude::*;

use super::{FmdRenderResult, artifact_result, parse_authoring_profile};
use crate::{Document, DocumentDiff, HtmlOptions, Profile};

// Explicit analysis follows publication's grammar and authoring transforms.
// Omission retains the historical source-AST treatment of page-break comments.
fn visible_document(document: &Document, profile: Option<Profile>) -> Cow<'_, Document> {
    if profile.is_some() {
        crate::apply_profile(document, profile)
    } else {
        Cow::Borrowed(document)
    }
}

/// Build the heading/body-paragraph search index using the publication dialect.
///
/// Omission preserves [`super::search_index`] byte for byte. Explicit profiles
/// select the grammar before parsing, so cross-line inline constructs and
/// definition-list-shaped paragraphs have the same ownership as publication.
///
/// # Errors
/// Rejects an unknown authoring profile before parsing the source.
#[wasm_bindgen(js_name = searchIndexProfiled)]
pub fn search_index_profiled(
    markdown: &str,
    authoring_profile: Option<String>,
) -> Result<String, JsValue> {
    search_index_inner(markdown, authoring_profile.as_deref()).map_err(JsValue::from_str)
}

fn search_index_inner(
    markdown: &str,
    authoring_profile: Option<&str>,
) -> Result<String, &'static str> {
    let profile = parse_authoring_profile(authoring_profile)?;
    let document = crate::parse_markdown_with_profile(markdown, profile);
    let document = visible_document(&document, profile);
    Ok(crate::search_index_json(&crate::build_search_index(
        &document,
    )))
}

/// Compare two revisions using one captured authoring dialect.
///
/// Both originals use the selected grammar and publication transforms. The
/// JSON schema and optional revision labels are unchanged; an omitted profile
/// preserves [`super::semantic_diff`] exactly.
///
/// # Errors
/// Rejects an unknown authoring profile before parsing either revision.
#[wasm_bindgen(js_name = semanticDiffProfiled)]
pub fn semantic_diff_profiled(
    old_markdown: &str,
    new_markdown: &str,
    old_name: Option<String>,
    new_name: Option<String>,
    authoring_profile: Option<String>,
) -> Result<String, JsValue> {
    let profile =
        parse_authoring_profile(authoring_profile.as_deref()).map_err(JsValue::from_str)?;
    Ok(diff_with_profile(
        old_markdown,
        new_markdown,
        old_name.as_deref(),
        new_name.as_deref(),
        profile,
    )
    .to_json())
}

fn diff_with_profile(
    old_markdown: &str,
    new_markdown: &str,
    old_name: Option<&str>,
    new_name: Option<&str>,
    profile: Option<Profile>,
) -> DocumentDiff {
    let old = crate::parse_markdown_with_profile(old_markdown, profile);
    let new = crate::parse_markdown_with_profile(new_markdown, profile);
    let old = visible_document(&old, profile);
    let new = visible_document(&new, profile);
    crate::compute_diff(
        &old,
        &new,
        old_name.unwrap_or("Before"),
        new_name.unwrap_or("Current"),
    )
}

/// Render a visual revision review with the same dialect as the publications.
///
/// The selected grammar governs both revisions and the HTML emitter. Source
/// byte accounting, safe escaping, labels and the legacy omitted-profile
/// output remain unchanged.
///
/// # Errors
/// Rejects an unknown authoring profile before parsing either revision.
#[wasm_bindgen(js_name = renderSemanticDiffHtmlProfiled)]
pub fn render_semantic_diff_html_profiled(
    old_markdown: &str,
    new_markdown: &str,
    old_name: Option<String>,
    new_name: Option<String>,
    authoring_profile: Option<String>,
) -> Result<FmdRenderResult, JsValue> {
    let profile =
        parse_authoring_profile(authoring_profile.as_deref()).map_err(JsValue::from_str)?;
    let diff = diff_with_profile(
        old_markdown,
        new_markdown,
        old_name.as_deref(),
        new_name.as_deref(),
        profile,
    );
    let options = HtmlOptions {
        profile,
        ..HtmlOptions::default()
    };
    Ok(artifact_result(
        "diff-html",
        "text/html; charset=utf-8",
        "html",
        diff.to_html_with_options(&options).into_bytes(),
        old_markdown.len() + new_markdown.len(),
        "[]".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::{Block, DiffBlock, Inline};

    #[test]
    fn omitted_profiles_preserve_all_legacy_search_and_review_outputs() {
        let old = "# Résumé\n\nTerm\n: Old value\n\n<!-- pagebreak -->\n\nTail.\n";
        let new = old.replace("Old", "New");
        assert_eq!(
            search_index_profiled(old, None).unwrap(),
            super::super::search_index(old)
        );
        for (old_name, new_name) in [(None, None), (Some("Before <&>"), Some("Current Ω"))] {
            assert_eq!(
                semantic_diff_profiled(
                    old,
                    &new,
                    old_name.map(str::to_owned),
                    new_name.map(str::to_owned),
                    None
                )
                .unwrap(),
                super::super::semantic_diff(
                    old,
                    &new,
                    old_name.map(str::to_owned),
                    new_name.map(str::to_owned)
                ),
            );
            let actual = render_semantic_diff_html_profiled(
                old,
                &new,
                old_name.map(str::to_owned),
                new_name.map(str::to_owned),
                None,
            )
            .unwrap();
            let legacy = super::super::render_semantic_diff_html(
                old,
                &new,
                old_name.map(str::to_owned),
                new_name.map(str::to_owned),
            );
            assert_eq!(actual.bytes(), legacy.bytes());
            assert_eq!(actual.source_length(), old.len() + new.len());
            assert_eq!(actual.diagnostics_json(), "[]");
            assert_eq!(actual.format(), "diff-html");
            assert_eq!(actual.extension(), "html");
            assert_eq!(actual.mime_type(), "text/html; charset=utf-8");
        }
    }

    #[test]
    fn search_uses_source_grammar_before_extracting_visible_paragraphs() {
        let source = "## Shared\n\n*Term\n: café*\n";
        let commonmark = search_index_profiled(source, Some("commonmark-gfm".into())).unwrap();
        let plus = search_index_profiled(source, Some("gfm-plus".into())).unwrap();
        assert!(
            commonmark.contains(r#""text":"Term : café""#),
            "{commonmark}"
        );
        assert!(commonmark.contains(r#""anchor":"shared""#), "{commonmark}");
        assert!(
            !plus.contains("café"),
            "the legacy heading/paragraph index omits definition lists: {plus}"
        );
        assert_eq!(plus, super::super::search_index(source));
        let html = crate::render_html(
            source,
            &HtmlOptions {
                profile: Some(Profile::CommonMarkGfm),
                ..HtmlOptions::default()
            },
        )
        .unwrap();
        assert!(html.contains(r#"id="shared""#));
        assert!(html.contains("<em>Term\n: café</em>"), "{html}");
    }

    #[test]
    fn revision_review_selects_grammar_before_resolving_cross_line_links() {
        let old = "[Term\n: definition](/old)\n";
        let new = "[Term\n: definition](/new)\n";
        let commonmark = diff_with_profile(old, new, None, None, Some(Profile::CommonMarkGfm));
        let plus = diff_with_profile(old, new, None, None, Some(Profile::GfmPlus));
        assert_eq!(commonmark.stats.modified_blocks, 1);
        let DiffBlock::Modified { old, new, .. } = &commonmark.blocks[0] else {
            panic!("paragraph must be modified")
        };
        for (block, destination) in [(old, "/old"), (new, "/new")] {
            let Block::Paragraph(inlines) = block.as_ref() else {
                panic!("CommonMark must retain a paragraph")
            };
            assert!(matches!(&inlines[0], Inline::Link { dest, .. } if dest == destination));
        }
        assert!(
            plus.blocks
                .iter()
                .any(|block| matches!(block, DiffBlock::Deleted(Block::DefinitionList(_))))
        );
        assert!(
            plus.blocks
                .iter()
                .any(|block| matches!(block, DiffBlock::Inserted(Block::DefinitionList(_))))
        );
    }

    #[test]
    fn explicit_review_profiles_compare_publication_page_breaks() {
        let old = "Before.\n\n<!-- pagebreak -->\n\nAfter.\n";
        let new = "Before.\n\n<!-- pagebreak --> \t\n\nAfter.\n";
        let plus = diff_with_profile(old, new, None, None, Some(Profile::GfmPlus));
        assert_eq!(plus.stats.unchanged_blocks, 3);
        assert_eq!(
            plus.stats.inserted_blocks + plus.stats.deleted_blocks + plus.stats.modified_blocks,
            0
        );
        assert!(matches!(
            plus.blocks[1],
            DiffBlock::Unchanged(Block::PageBreak)
        ));
        let commonmark = diff_with_profile(old, new, None, None, Some(Profile::CommonMarkGfm));
        assert!(
            commonmark.stats.inserted_blocks
                + commonmark.stats.deleted_blocks
                + commonmark.stats.modified_blocks
                > 0
        );
        assert_eq!(commonmark, diff_with_profile(old, new, None, None, None));
    }

    #[test]
    fn visual_review_retains_selected_inline_meaning_and_escaped_labels() {
        let old = "[Term\n: definition](/old)\n";
        let new = "[Term\n: definition](/new)\n";
        let output = render_semantic_diff_html_profiled(
            old,
            new,
            Some("<Before>".into()),
            Some("<After>".into()),
            Some("commonmark-gfm".into()),
        )
        .unwrap();
        let html = String::from_utf8(output.bytes()).unwrap();
        assert!(html.contains(r#"href="/old""#), "{html}");
        assert!(html.contains(r#"href="/new""#), "{html}");
        assert!(html.contains("&lt;Before&gt;"));
        assert!(html.contains("&lt;After&gt;"));
        assert!(!html.contains("<dl"));
        assert_eq!(output.source_length(), old.len() + new.len());
    }

    #[test]
    fn search_rejects_unknown_profiles_instead_of_using_legacy_grammar() {
        for profile in ["", " ", "gfm", "GFM-PLUS", "gfm-plus ", "2b"] {
            assert_eq!(
                search_index_inner("source", Some(profile)),
                Err("profile must be commonmark-gfm or gfm-plus")
            );
        }
    }
}
