#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

fn file(path: &str, source: &str) -> BookInput {
    BookInput {
        path: path.into(),
        source: source.into(),
    }
}

fn rename(files: &[BookInput], resources: &[BookInput], old: &str, new: &str) -> BookSourceRename {
    rename_book_source(files, resources, old, new, None).unwrap()
}

#[test]
fn incoming_links_references_and_self_links_follow_the_moved_chapter() {
    let files = [
        file(
            "start.md",
            "# Start\n\n[direct](guide/old.md?print#part \"Title\") [ref]\n\n[ref]: guide/old.md#part\n\n`[literal](guide/old.md)`\n",
        ),
        file(
            "guide/old.md",
            "# Part\n\n[self](old.md#part) [local](#part)\n",
        ),
    ];
    let result = rename(&files, &[], "guide/old.md", "manual/new.md");
    assert_eq!(result.files[0].path, "start.md");
    assert_eq!(result.files[1].path, "manual/new.md");
    assert!(
        result.files[0]
            .source
            .contains("[direct](<manual/new.md?print#part> \"Title\")")
    );
    assert!(
        result.files[0]
            .source
            .contains("[ref]: <manual/new.md#part>")
    );
    assert!(result.files[0].source.contains("`[literal](guide/old.md)`"));
    assert!(
        result.files[1]
            .source
            .contains("[self](<new.md#part>) [local](#part)")
    );
    assert_eq!((result.changed_source_count, result.edit_count), (2, 3));
    assert_eq!(files[1].path, "guide/old.md");
}

#[test]
fn moved_source_retains_outgoing_images_downloads_and_directory_targets() {
    let source = "# Moved\n\n![image](../assets/image.png?v=1#view) [download](../files/archive.zip) [dir](../downloads/) [other](other.md) [root](/static/a.pdf)\n";
    let files = [
        file("guide/old.md", source),
        file("guide/other.md", "# Other"),
    ];
    let result = rename(&files, &[], "guide/old.md", "new.md");
    let source = &result.files[0].source;
    for destination in [
        "assets/image.png?v=1#view",
        "files/archive.zip",
        "downloads/",
        "guide/other.md",
    ] {
        assert!(source.contains(&format!("<{destination}>")), "{source}");
    }
    assert!(source.contains("[root](/static/a.pdf)"));
}

#[test]
fn chapter_aliases_resolve_before_rename_without_capturing_other_images() {
    let files = [
        file(
            "start.md",
            "[extension](guide) [directory](manual/) ![asset](guide)",
        ),
        file("guide.md", "# Guide"),
        file("manual/index.md", "# Manual"),
    ];
    let result = rename(&files, &[], "guide.md", "docs/guide.md");
    assert_eq!(
        result.files[0].source,
        "[extension](<docs/guide.md>) [directory](manual/) ![asset](guide)"
    );
    let result = rename(&files, &[], "manual/index.md", "manual/reference.md");
    assert!(
        result.files[0]
            .source
            .contains("[directory](<manual/reference.md>)")
    );
}

#[test]
fn nested_includes_and_selectors_keep_their_source_ownership() {
    let files = [file(
        "start.md",
        "# Start\n\n{{#include parts/note.md:sample}}\n",
    )];
    let resources = [
        file(
            "parts/note.md",
            "<!-- ANCHOR: sample -->\n[guide](../guide.md)\n{{#include nested/end.txt:2:2}}\n<!-- ANCHOR_END: sample -->\n",
        ),
        file("parts/nested/end.txt", "Ignored\nEnd\n"),
        file("guide.md", "# Guide"),
    ];
    let result = rename(&files, &resources, "parts/note.md", "notes/renamed.md");
    assert_eq!(
        result.files[0].source,
        "# Start\n\n{{#include \"notes/renamed.md\":sample}}\n"
    );
    assert!(
        result.include_sources[0]
            .source
            .contains("{{#include \"../parts/nested/end.txt\":2:2}}")
    );
    assert_eq!(result.include_sources[0].path, "notes/renamed.md");
    assert_eq!((result.changed_source_count, result.edit_count), (2, 2));
}

#[test]
fn renamed_include_updates_all_parents_and_rebases_its_assets() {
    let files = [
        file("one.md", "{{#include parts/card.md}}\n"),
        file("chapters/two.md", "{{#include ../parts/card.md}}\n"),
    ];
    let resources = [file("parts/card.md", "Card\n\n![icon](icon.png)\n")];
    let result = rename(&files, &resources, "parts/card.md", "other/deep/card.md");
    assert!(result.files[0].source.contains("\"other/deep/card.md\""));
    assert!(result.files[1].source.contains("\"../other/deep/card.md\""));
    assert!(
        result.include_sources[0]
            .source
            .contains("<../../parts/icon.png>")
    );
    assert_eq!((result.changed_source_count, result.edit_count), (3, 3));
}

#[test]
fn quoted_literal_include_paths_are_not_percent_decoded() {
    let files = [file("start.md", "{{#include \"parts/a%20#x.md\":1}}\n")];
    let resources = [file("parts/a%20#x.md", "Body\n")];
    let result = rename(&files, &resources, "parts/a%20#x.md", "new/中%.md");
    assert_eq!(result.files[0].source, "{{#include \"new/中%.md\":1}}\n");
    assert_eq!(result.include_sources[0].path, "new/中%.md");
}

#[test]
fn uri_paths_decode_once_and_retain_unicode_queries_and_fragments() {
    let files = [
        file("start.md", "[one](parts/a%2520%23x.md?q=1&amp;x=2#λ)"),
        file("parts/a%20#x.md", "# Target"),
    ];
    let result = rename(&files, &[], "parts/a%20#x.md", "中/space name?.md");
    assert_eq!(
        result.files[0].source,
        "[one](<%E4%B8%AD/space%20name%3F.md?q=1&amp;x=2#λ>)"
    );
}

#[test]
fn code_math_labels_titles_and_external_urls_are_not_rewritten() {
    let source = "# Start\n\n`[old](old.md)` $old.md$ [old.md](https://example.test/old.md \"old.md\")\n\n```md\n[old](old.md)\n{{#include old.md}}\n```\n\n    {{#include old.md}}\n";
    let result = rename(
        &[file("start.md", source), file("old.md", "# Old")],
        &[],
        "old.md",
        "new.md",
    );
    assert_eq!(result.files[0].source, source);
    assert_eq!((result.changed_source_count, result.edit_count), (1, 0));
}

#[test]
fn shared_reference_cannot_move_a_source_link_and_an_immutable_image_key() {
    let files = [
        file(
            "start.md",
            "[link][same] ![image][same]\n\n[same]: old.md\n",
        ),
        file("old.md", "# Old"),
    ];
    assert!(
        rename_book_source(&files, &[], "old.md", "new.md", None)
            .unwrap_err()
            .to_string()
            .contains("incompatible")
    );
}

#[test]
fn source_and_image_asset_namespaces_remain_separate_through_pdf_publication() {
    let files = [
        file("start.md", "# Start\n\n[chapter](old.md) ![icon](old.md)\n"),
        file("old.md", "# Old\n\n![self-named asset](old.md)\n"),
    ];
    let result = rename_book_source_with_images(
        &files,
        &[],
        "old.md",
        "nested/new.md",
        None,
        &["old.md".into()],
    )
    .unwrap();
    assert!(
        result.files[0]
            .source
            .contains("[chapter](<nested/new.md>) ![icon](old.md)")
    );
    assert!(
        result.files[1]
            .source
            .contains("![self-named asset](old.md)")
    );
    let image = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"8\"><rect width=\"12\" height=\"8\"/></svg>";
    for sources in [&files[..], result.files.as_slice()] {
        let mut renderer = BookRenderer::from_sources(sources, &[]).unwrap();
        renderer.set_image("old.md", image.to_vec()).unwrap();
        let pdf = renderer.render_pdf_reported().unwrap();
        assert!(
            !pdf.warnings
                .iter()
                .any(|warning| matches!(warning.code(), "missing_image" | "unsupported_image")),
            "{:?}",
            pdf.warnings
        );
    }
}

#[test]
fn image_inventory_preserves_qualified_literal_and_trimmed_fallback_bindings() {
    for (keys, expected) in [
        (vec!["figure.svg"], "figure.svg"),
        (vec!["guide/figure.svg", "figure.svg"], "guide/figure.svg"),
        (vec!["figure.svg", "manual/figure.svg"], "figure.svg"),
        (
            vec!["guide/figure.svg", "manual/guide/figure.svg", "figure.svg"],
            "guide/figure.svg",
        ),
    ] {
        for spelling in ["figure.svg", "< figure.svg >"] {
            let files = [file(
                "guide/old.md",
                &format!("# Image\n\n![figure]({spelling})\n"),
            )];
            let keys: Vec<String> = keys.iter().map(|key| (*key).to_owned()).collect();
            let result = rename_book_source_with_images(
                &files,
                &[],
                "guide/old.md",
                "manual/new.md",
                None,
                &keys,
            )
            .unwrap();
            for source in [&files[..], result.files.as_slice()] {
                let mut renderer = BookRenderer::from_sources(source, &[]).unwrap();
                for (index, key) in keys.iter().enumerate() {
                    renderer.set_image(key, format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"8\"><rect width=\"4\" height=\"8\"/></svg>", 12 + index).into_bytes()).unwrap();
                }
                let mut prepared = super::super::book_pdf_document_with_assets(
                    renderer.book(),
                    &renderer.options().pdf_image_assets,
                )
                .unwrap();
                let mut selected = Vec::new();
                visit_destinations(&mut prepared.blocks, &mut |destination, kind| {
                    if kind == UrlKind::Image {
                        selected.push(destination.trim().to_owned());
                    }
                });
                assert_eq!(selected, [expected], "{keys:?}: {}", source[0].source);
                let pdf = renderer.render_pdf_reported().unwrap();
                assert!(!pdf.warnings.iter().any(|warning| matches!(
                    warning.code(),
                    "missing_image" | "unsupported_image"
                )));
            }
        }
    }
}

#[test]
fn provided_asset_keys_remain_exact_across_include_origin_rebasing() {
    let files = [file("start.md", "# Start\n\n{{#include parts/note.md}}\n")];
    let resources = [file("parts/note.md", "![icon](icon.svg)\n")];
    let keys = ["parts/icon.svg".into(), "icon.svg".into()];
    let result = rename_book_source_with_images(
        &files,
        &resources,
        "parts/note.md",
        "manual/deep/note.md",
        None,
        &keys,
    )
    .unwrap();
    assert!(
        result.include_sources[0]
            .source
            .contains("</parts/icon.svg>")
    );
    let renderer = BookRenderer::from_sources(&result.files, &result.include_sources).unwrap();
    assert!(format!("{:?}", renderer.book().chapters[0].doc).contains("/parts/icon.svg"));
    // Raw snippet lookup would see the literal grant, but expansion has
    // already rebased it to parts/icon.svg. A move must not alter that missing
    // expanded binding based on an incomplete standalone-source interpretation.
    assert!(
        rename_book_source_with_images(
            &files,
            &resources,
            "parts/note.md",
            "manual/deep/note.md",
            None,
            &["icon.svg".into()]
        )
        .unwrap_err()
        .to_string()
        .contains("expanded Markdown semantics")
    );
}

#[test]
fn image_move_cannot_turn_missing_content_into_an_unrelated_granted_asset() {
    let files = [file("guide/old.md", "![missing](figure.svg)")];
    let keys = ["manual/figure.svg".into()];
    let result =
        rename_book_source_with_images(&files, &[], "guide/old.md", "manual/new.md", None, &keys)
            .unwrap();
    assert_eq!(result.files[0].source, "![missing](<../guide/figure.svg>)");
    let keys = [
        "manual/figure.svg".into(),
        "../guide/figure.svg".into(),
        "/guide/figure.svg".into(),
    ];
    assert!(
        rename_book_source_with_images(&files, &[], "guide/old.md", "manual/new.md", None, &keys)
            .unwrap_err()
            .to_string()
            .contains("unresolved asset")
    );
}

#[test]
fn image_key_inventory_admits_uri_keys_but_refuses_duplicates_and_excess() {
    let files = [file(
        "guide/old.md",
        "![external](https://host/icon.svg) ![root](/icon.svg) ![fragment](#icon)",
    )];
    let keys = [
        "https://host/icon.svg".into(),
        "/icon.svg".into(),
        "#icon".into(),
    ];
    let result =
        rename_book_source_with_images(&files, &[], "guide/old.md", "manual/new.md", None, &keys)
            .unwrap();
    assert_eq!(result.files[0].source, files[0].source);
    for invalid in [
        vec!["".into()],
        vec!["  ".into()],
        vec!["x".into(), " x ".into()],
        vec!["a".repeat(4097)],
        vec!["x".into(); 4097],
    ] {
        assert!(
            rename_book_source_with_images(
                &files,
                &[],
                "guide/old.md",
                "manual/new.md",
                None,
                &invalid
            )
            .is_err()
        );
    }
}

#[test]
fn mixed_reference_alias_roles_fail_atomically_instead_of_changing_an_image() {
    let files = [
        file("start.md", "[link][same] ![image][same]\n\n[same]: old\n"),
        file("old.md", "# Old"),
    ];
    let error = rename_book_source(&files, &[], "old.md", "new.md", None).unwrap_err();
    assert!(error.to_string().contains("incompatible"));
    assert_eq!(files[1].path, "old.md");
}

#[test]
fn raw_html_destinations_are_refused_if_they_need_an_unproven_edit() {
    let files = [
        file("start.md", "<a href=\"old.md\">old</a>"),
        file("old.md", "# Old"),
    ];
    assert!(
        rename_book_source(&files, &[], "old.md", "new.md", None)
            .unwrap_err()
            .to_string()
            .contains("semantics")
    );
}

#[test]
fn a_missing_raw_html_destination_or_unused_include_cannot_capture_the_move() {
    let files = [
        file("start.md", "<a href=\"new.md\">new</a>"),
        file("old.md", "# Old"),
    ];
    assert!(
        rename_book_source(&files, &[], "old.md", "new.md", None)
            .unwrap_err()
            .to_string()
            .contains("capture an existing destination")
    );
    let files = [file("old.md", "# Old")];
    let resources = [file("unused.md", "{{#include new.md}}")];
    assert!(
        rename_book_source(&files, &resources, "old.md", "new.md", None)
            .unwrap_err()
            .to_string()
            .contains("capture an existing include")
    );
}

#[test]
fn snippet_context_conflicts_fail_without_modifying_literal_examples() {
    let files = [
        file("start.md", "{{#include part.md:2:2}}\n"),
        file("old.md", "# Old"),
    ];
    let resources = [file("part.md", "```md\n[link](old.md)\n```\n")];
    let error = rename_book_source(&files, &resources, "old.md", "new.md", None).unwrap_err();
    assert!(error.to_string().contains("expanded Markdown semantics"));
    assert_eq!(resources[0].source, "```md\n[link](old.md)\n```\n");
}

#[test]
fn aliases_cannot_capture_an_unrelated_existing_destination() {
    for (destination, new) in [
        ("new", "new.md"),
        ("new.md", "new.md"),
        ("new.md#part", "new.md"),
        ("/new.md", "new.md"),
        ("new/", "new/index.md"),
        ("new", "new/index.md"),
    ] {
        let files = [
            file("start.md", &format!("[missing]({destination})")),
            file("old.md", "# Old"),
        ];
        assert!(
            rename_book_source(&files, &[], "old.md", new, None).is_err(),
            "{destination} -> {new}"
        );
    }
}

#[test]
fn canonical_paths_missing_endpoints_and_collisions_are_rejected() {
    let files = [file("start.md", "# Start"), file("old.md", "# Old")];
    for new in [
        "../out.md",
        "./new.md",
        "/new.md",
        "a\\new.md",
        "a/../new.md",
        "start.md",
        "START.md",
        "start.md/child.md",
        "x:y.md",
    ] {
        assert!(
            rename_book_source(&files, &[], "old.md", new, None).is_err(),
            "{new}"
        );
    }
    assert!(rename_book_source(&files, &[], "missing.md", "new.md", None).is_err());
    assert!(
        rename_book_source(&[file("./old.md", "# Old")], &[], "old.md", "new.md", None).is_err()
    );
}

#[test]
fn output_name_collision_and_broken_include_graph_have_no_partial_proposal() {
    let files = [file("a/b.md", "# A"), file("old.md", "# Old")];
    assert!(rename_book_source(&files, &[], "old.md", "a__b.md", None).is_err());
    let files = [file("old.md", "{{#include missing.md}}")];
    assert!(rename_book_source(&files, &[], "old.md", "new.md", None).is_err());
}

#[test]
fn exact_noop_retains_source_spelling_and_reports_no_work() {
    let files = [file("old.md", "# Old\r\n\r\n[self](./old.md)\r\n")];
    let result = rename(&files, &[], "old.md", "old.md");
    assert_eq!(result.files[0].source, files[0].source);
    assert_eq!((result.changed_source_count, result.edit_count), (0, 0));
}

#[test]
fn profiles_use_the_same_authoring_grammar_for_proofs_and_expansion() {
    let files = [
        file("start.md", "[link](old.md)\n\nTerm\n: description\n"),
        file("old.md", "# Old"),
    ];
    for profile in [None, Some(Profile::GfmPlus), Some(Profile::CommonMarkGfm)] {
        let result = rename_book_source(&files, &[], "old.md", "new.md", profile).unwrap();
        assert_eq!(
            result.files[0].source,
            "[link](<new.md>)\n\nTerm\n: description\n"
        );
    }
}

#[test]
fn report_json_escapes_source_control_bytes_and_contains_only_owned_captures() {
    let result = rename(
        &[file("old.md", "# \"quoted\"\n\\\u{0001}")],
        &[],
        "old.md",
        "new.md",
    );
    let json = result.to_json().unwrap();
    assert!(json.starts_with(
        "{\"schema\":\"fmd-book-source-rename-v1\",\"oldPath\":\"old.md\",\"newPath\":\"new.md\""
    ));
    assert!(
        json.contains("\"source\":\"# \\\"quoted\\\"\\u000a\\\\\\u0001\""),
        "{json}"
    );
    assert!(json.ends_with("\"includeSources\":[],\"changedSourceCount\":1,\"editCount\":0}"));
}

#[test]
fn source_token_and_edit_budgets_refuse_instead_of_returning_partial_edits() {
    let plan = Plan {
        old: "old.md",
        new: "new.md",
        before: BTreeSet::from(["old.md".into()]),
        after: BTreeSet::from(["new.md".into()]),
        image_keys: BTreeSet::new(),
    };
    let source = file("start.md", "[link](old.md)");
    let mut tokens = Budget {
        tokens: MAX_TOKENS,
        ..Budget::default()
    };
    assert!(
        rewrite_source(&source, &plan, None, &mut tokens)
            .unwrap_err()
            .to_string()
            .contains("source tokens")
    );
    let mut hidden = Budget {
        tokens: MAX_TOKENS - 1,
        ..Budget::default()
    };
    assert!(
        rewrite_source(
            &file("start.md", "{{#include [x](old.md)}}"),
            &plan,
            None,
            &mut hidden
        )
        .unwrap_err()
        .to_string()
        .contains("source tokens")
    );
    let mut edits = Budget {
        edits: MAX_EDITS,
        ..Budget::default()
    };
    assert!(
        rewrite_source(&source, &plan, None, &mut edits)
            .unwrap_err()
            .to_string()
            .contains("source-edit budget")
    );
}

#[test]
fn include_inventory_uses_actual_code_protection_and_exact_quoted_path_ranges() {
    let source = "`literal\r\n{{#include hidden.md}}\r\nend`\r\n\r\n{{#include \"parts/a`b.md\":2:3}}\r\n{{#include next.md}}\n";
    let tokens = crate::transclude::include_target_spans(source, None).unwrap();
    assert_eq!(tokens.len(), 2);
    assert_eq!(&source[tokens[0].range.clone()], "\"parts/a`b.md\"");
    assert_eq!(tokens[0].path, "parts/a`b.md");
    assert_eq!(&source[tokens[1].range.clone()], "next.md");
}

#[test]
fn renamed_capture_publishes_pdf_epub_and_site_with_existing_asset_keys() {
    let files = [
        file("start.md", "# Start\n\n[details](guide/old.md#details)\n"),
        file(
            "guide/old.md",
            "# Details\n\n{{#include ../parts/note.md}}\n\n![Diagram](diagram.svg)\n",
        ),
    ];
    let resources = [file("parts/note.md", "Read [start](../start.md#start).\n")];
    let result = rename_book_source_with_images(
        &files,
        &resources,
        "guide/old.md",
        "manual/details.md",
        None,
        &["guide/diagram.svg".into()],
    )
    .unwrap();
    let mut renderer = BookRenderer::from_sources(&result.files, &result.include_sources).unwrap();
    renderer.set_image("guide/diagram.svg", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"12\" height=\"8\"><rect width=\"12\" height=\"8\"/></svg>".to_vec()).unwrap();
    assert_eq!(renderer.validate_links().unwrap().finding_count(), 0);
    let pdf = renderer.render_pdf_reported().unwrap();
    assert!(pdf.bytes.starts_with(b"%PDF-"));
    assert!(
        !pdf.warnings
            .iter()
            .any(|warning| matches!(warning.code(), "missing_image" | "unsupported_image"))
    );
    assert!(renderer.render_epub().unwrap().starts_with(b"PK\x03\x04"));
    assert!(renderer.render_site().unwrap().starts_with(b"PK\x03\x04"));
}
