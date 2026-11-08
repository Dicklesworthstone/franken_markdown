//! Source dialects are selected before parsing, expansion, and navigation.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use franken_markdown::book::{
    BookInput, BookRenderer, BookWorkspace, build_book_with_diagnostics_and_profile,
    build_book_with_profile,
};
use franken_markdown::{Profile, parse_markdown_spanned_with_profile, parse_markdown_with_profile};

const COMMONMARK: Option<Profile> = Some(Profile::CommonMarkGfm);

fn file(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

#[test]
fn profiled_builders_preserve_cross_line_inline_syntax_and_exact_diagnostics() {
    for source in [
        "*Term\n: definition*\n",
        "`Term\n: definition`\n",
        "[Term\n: definition](https://example.com)\n",
        "First\n: value one\nSecond\n: value two\n",
        "# Start\n\n*Term\n: definition*\n\n```rust\nunclosed\n",
    ] {
        let inputs = [file("chapter.md", source)];
        for profile in [None, Some(Profile::GfmPlus), COMMONMARK] {
            let book = build_book_with_profile(&inputs, profile).unwrap();
            let (diagnosed, report) =
                build_book_with_diagnostics_and_profile(&inputs, profile).unwrap();
            assert_eq!(
                book.chapters[0].doc,
                parse_markdown_with_profile(source, profile)
            );
            assert_eq!(diagnosed.chapters[0].doc, book.chapters[0].doc);
            let expected = parse_markdown_spanned_with_profile(source, profile);
            assert_eq!(report.chapters()[0].source_bytes, source.len());
            assert_eq!(report.chapters()[0].diagnostics, expected.diagnostics);
        }
    }
}

#[test]
fn included_cross_line_links_keep_their_origin_and_code_never_resolves_files() {
    let chapters = [
        file(
            "guide/deep/start.md",
            "# Start\n\n{{#include ../../parts/shared.txt}}\n",
        ),
        file("end.md", "# End\n"),
    ];
    let resources = [file(
        "parts/shared.txt",
        "[Term\n: definition](../end.md#end)\n\n`Term\n: definition\n{{#include missing.md}}\ntail`\n\n```rust\nunclosed\n",
    )];
    let renderer =
        BookRenderer::from_sources_with_profile(&chapters, &resources, COMMONMARK).unwrap();
    let html = renderer.render_chapter_html(0).unwrap();
    assert!(html.contains("href=\"end.html#end\""), "{html}");
    assert!(
        html.contains("<code>Term : definition {{#include missing.md}} tail</code>"),
        "{html}"
    );
    assert_eq!(renderer.validate_links().unwrap().finding_count(), 0);
    assert_eq!(renderer.validate_links().unwrap().chapters[0].checked, 1);
    let expanded = "# Start\n\n[Term\n: definition](<../../end.md#end>)\n\n`Term\n: definition\n{{#include missing.md}}\ntail`\n\n```rust\nunclosed\n";
    let expected = parse_markdown_spanned_with_profile(expanded, COMMONMARK);
    let report = &renderer.source_diagnostics().chapters()[0];
    assert_eq!(report.source_bytes, expanded.len());
    assert_eq!(report.diagnostics, expected.diagnostics);
    assert!(BookRenderer::from_sources(&chapters, &resources).is_err());
}

#[test]
fn dialect_changes_and_source_transactions_preserve_cache_identity_and_assets() {
    let first = "# A\n\n[Term\n: definition](b.md#b)\n";
    let chapters = [file("a.md", first), file("b.md", "# B\n")];
    let mut workspace = BookWorkspace::new(&chapters).unwrap();
    workspace
        .set_image("brand.svg", b"<svg/>".to_vec())
        .unwrap();
    workspace.options_mut().title = Some("Kept metadata".into());
    let asset = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    let original = workspace.book().chapters[0].doc.blocks.as_ptr();
    workspace
        .set_authoring_profile(Some(Profile::GfmPlus))
        .unwrap();
    assert_eq!(workspace.book().chapters[0].doc.blocks.as_ptr(), original);
    workspace.set_authoring_profile(COMMONMARK).unwrap();
    assert_eq!(workspace.source_revision(), 0);
    assert_eq!(
        workspace.options().pdf_image_assets[0].bytes.as_ptr(),
        asset
    );
    assert_eq!(workspace.options().title.as_deref(), Some("Kept metadata"));
    assert_eq!(workspace.validate_links().unwrap().chapters[0].checked, 1);
    assert!(
        workspace
            .render_chapter_html(0)
            .unwrap()
            .contains("href=\"b.html#b\"")
    );
    let commonmark_ast = workspace.book().chapters[0].doc.blocks.as_ptr();
    workspace.set_authoring_profile(COMMONMARK).unwrap();
    assert_eq!(
        workspace.book().chapters[0].doc.blocks.as_ptr(),
        commonmark_ast
    );
    let edit = workspace
        .update_sources_at_revision(&[file("b.md", "# B\n\nChanged.\n")], 0)
        .unwrap();
    assert_eq!(edit.reparsed_chapters, [1]);
    assert_eq!(
        workspace.book().chapters[0].doc.blocks.as_ptr(),
        commonmark_ast
    );
    let revised = "# A\n\n*Changed term\n: changed definition*\n";
    workspace
        .update_sources_at_revision(&[file("a.md", revised)], 1)
        .unwrap();
    assert!(
        workspace
            .render_chapter_html(0)
            .unwrap()
            .contains("<em>Changed term\n: changed definition</em>")
    );
    let replacement = [file("b.md", "# B\n"), file("a.md", revised)];
    workspace
        .replace_sources_at_revision(&replacement, &[], 2)
        .unwrap();
    assert_eq!(workspace.source_revision(), 3);
    assert_eq!(workspace.options().profile, COMMONMARK);
    assert!(
        workspace
            .render_chapter_html(1)
            .unwrap()
            .contains("<em>Changed term\n: changed definition</em>")
    );
    assert_eq!(
        workspace.options().pdf_image_assets[0].bytes.as_ptr(),
        asset
    );
    workspace.set_authoring_profile(None).unwrap();
    assert_eq!(workspace.source_revision(), 3);
    assert_eq!(
        workspace.options().pdf_image_assets[0].bytes.as_ptr(),
        asset
    );
    assert!(workspace.render_chapter_html(1).unwrap().contains("<dl>"));
}

#[test]
fn failed_dialect_expansion_preserves_the_previous_usable_capture() {
    let source = "# Safe\n\n`Term\n: definition\n{{#include unavailable.md}}\ntail`\n";
    let mut workspace =
        BookWorkspace::from_sources_with_profile(&[file("a.md", source)], &[], COMMONMARK).unwrap();
    workspace.set_image("kept.svg", b"<svg/>".to_vec()).unwrap();
    let asset = workspace.options().pdf_image_assets[0].bytes.as_ptr();
    let html = workspace.render_chapter_html(0).unwrap();
    let diagnostics = workspace.source_diagnostics().to_json();
    let ast = workspace.book().chapters[0].doc.blocks.as_ptr();
    for profile in [None, Some(Profile::GfmPlus)] {
        assert!(workspace.set_authoring_profile(profile).is_err());
        assert_eq!(workspace.options().profile, COMMONMARK);
        assert_eq!(workspace.source_revision(), 0);
        assert_eq!(workspace.book().chapters[0].doc.blocks.as_ptr(), ast);
        assert_eq!(
            workspace.options().pdf_image_assets[0].bytes.as_ptr(),
            asset
        );
        assert_eq!(workspace.render_chapter_html(0).unwrap(), html);
        assert_eq!(workspace.source_diagnostics().to_json(), diagnostics);
    }
    let update = workspace
        .update_sources_at_revision(&[file("a.md", source)], 0)
        .unwrap();
    assert!(update.reparsed_chapters.is_empty());
    assert_eq!(update.revision, 0);
}

#[cfg(feature = "wasm-book")]
#[test]
fn browser_constructors_select_profile_before_include_expansion_and_previews() {
    use franken_markdown::book::browser::FmdBook;
    let source =
        "# Browser\n\n*Term\n: definition*\n\n`Term\n: definition\n{{#include absent.md}}\ntail`\n";
    let mut book = FmdBook::from_sources_profiled(
        vec!["one.md".into()],
        vec![source.into()],
        vec![],
        vec![],
        Some("commonmark-gfm".into()),
    )
    .unwrap();
    let preview = String::from_utf8(book.render_chapter_preview(0.0).unwrap()).unwrap();
    assert!(preview.contains("<em>Term\\n: definition</em>"));
    assert!(preview.contains("{{#include absent.md}}"));
    assert_eq!(book.source_revision(), 0);
    book.update_sources(
        vec!["one.md".into()],
        vec![format!("{source}\nUpdated.\n")],
        0.0,
    )
    .unwrap();
    assert_eq!(book.source_revision(), 1);
    let preview = String::from_utf8(book.render_chapter_preview(0.0).unwrap()).unwrap();
    assert!(preview.contains("<em>Term\\n: definition</em>"));
}
