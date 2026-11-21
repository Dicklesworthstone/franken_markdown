use super::super::{Phase, Position, Server, integer, number};
use super::*;
use franken_markdown::parse::destination_spans;

type TestResult = Result<(), Box<dyn std::error::Error>>;
const ROOT: &str = "file:///docs/README.md";

fn buffers(entries: &[(&str, &str)]) -> BTreeMap<String, Buffer> {
    entries
        .iter()
        .enumerate()
        .map(|(index, (uri, source))| {
            (
                (*uri).to_string(),
                Buffer {
                    text: (*source).to_string(),
                    version: index as i32 + 11,
                    synchronized: true,
                },
            )
        })
        .collect()
}

fn params(moves: &[(&str, &str)]) -> Json {
    object([(
        "files",
        Json::Array(
            moves
                .iter()
                .map(|(old, new)| object([("oldUri", string(old)), ("newUri", string(new))]))
                .collect(),
        ),
    )])
}

fn array(value: &Json) -> Result<&[Json], &'static str> {
    match value {
        Json::Array(items) => Ok(items),
        _ => Err("expected array"),
    }
}

fn apply_changes(
    documents: &BTreeMap<String, Buffer>,
    result: &Json,
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let mut amended: BTreeMap<_, _> = documents
        .iter()
        .map(|(uri, buffer)| (uri.clone(), buffer.text.clone()))
        .collect();
    if result == &Json::Null {
        return Ok(amended);
    }
    for change in array(
        result
            .get("documentChanges")
            .ok_or("missing documentChanges")?,
    )? {
        assert!(
            change.get("kind").is_none(),
            "the server must not issue file operations"
        );
        let document = change.get("textDocument").ok_or("missing textDocument")?;
        let uri = document
            .get("uri")
            .and_then(Json::as_str)
            .ok_or("missing old URI")?;
        let original = documents
            .get(uri)
            .ok_or("edits must address an originally opened URI")?;
        assert_eq!(
            document.get("version").and_then(integer),
            Some(original.version)
        );
        let index = LineIndex::new(&original.text);
        let mut previous = 0;
        let mut edits = Vec::new();
        for edit in array(change.get("edits").ok_or("missing edits")?)? {
            let range = edit.get("range").ok_or("missing range")?;
            let start = index.offset(
                &original.text,
                Position::parse(range.get("start").ok_or("missing start")?)?,
            )?;
            let end = index.offset(
                &original.text,
                Position::parse(range.get("end").ok_or("missing end")?)?,
            )?;
            assert!(start >= previous && end >= start);
            previous = end;
            edits.push((
                start..end,
                edit.get("newText")
                    .and_then(Json::as_str)
                    .ok_or("missing text")?,
            ));
        }
        let output = amended.get_mut(uri).ok_or("missing old buffer")?;
        for (range, text) in edits.into_iter().rev() {
            output.replace_range(range, text);
        }
    }
    Ok(amended)
}

fn changed(
    documents: &BTreeMap<String, Buffer>,
    moves: &[(&str, &str)],
) -> Result<BTreeMap<String, String>, Box<dyn std::error::Error>> {
    let result =
        request(Parser::default(), documents, &params(moves)).map_err(|(_, reason)| reason)?;
    apply_changes(documents, &result)
}

#[test]
fn file_move_updates_inline_images_and_every_reference_definition_without_changing_titles()
-> TestResult {
    let source = "😀 [inline](<chapter.md?view=1#intro> \"Keep\") ![chart](chapter.md?raw=1) [full][guide] [collapsed][] [shortcut]\r\n\r\n[guide]: chapter.md#intro \"Title\"\r\n\r\n[collapsed]: chapter.md\r\n\r\n[shortcut]: chapter.md\r\n\r\n[unused]: chapter.md\r\n\r\n[guide]: chapter.md#shadow\r\n";
    let documents = buffers(&[(ROOT, source), ("file:///docs/chapter.md", "# Intro\n")]);
    let output = changed(
        &documents,
        &[("file:///docs/chapter.md", "file:///docs/chapter-new.md")],
    )?;
    assert_eq!(output[ROOT], source.replace("chapter.md", "chapter-new.md"));
    assert_eq!(
        documents[ROOT].text, source,
        "requests cannot mutate live buffers"
    );
    assert_eq!(output["file:///docs/chapter.md"], "# Intro\n");
    assert!(!output.contains_key("file:///docs/chapter-new.md"));
    Ok(())
}

#[test]
fn moving_a_source_rebases_unopened_assets_and_links_while_preserving_self_fragments() -> TestResult
{
    let origin = "file:///docs/ch/start.md";
    let source = "[up](../shared.md?q=1#part) ![pixel](../img/a.png) [sibling](sibling.md) [self](start.md#self) [anchor](#self) [query](?view=1#self) [dir](./) [root](/docs/shared.md) [web](https://example.test/a.md)\n";
    let documents = buffers(&[(origin, source)]);
    let output = changed(&documents, &[(origin, "file:///manual/start.md")])?;
    assert_eq!(
        output[origin],
        "[up](../docs/shared.md?q=1#part) ![pixel](../docs/img/a.png) [sibling](../docs/ch/sibling.md) [self](start.md#self) [anchor](#self) [query](?view=1#self) [dir](../docs/ch/) [root](/docs/shared.md) [web](https://example.test/a.md)\n"
    );
    Ok(())
}

#[test]
fn folder_move_uses_one_parent_event_and_keeps_relative_subtree_destinations() -> TestResult {
    let root = "[one](chapters/one.md#one) ![plot](chapters/assets/plot.png)\n";
    let chapter = "[two](two.md) ![plot](assets/plot.png) [outside](../cover.md)\n";
    let documents = buffers(&[(ROOT, root), ("file:///docs/chapters/one.md", chapter)]);
    let output = changed(&documents, &[("file:///docs/chapters", "file:///handbook")])?;
    assert_eq!(
        output[ROOT],
        "[one](../handbook/one.md#one) ![plot](../handbook/assets/plot.png)\n"
    );
    assert_eq!(
        output["file:///docs/chapters/one.md"],
        "[two](two.md) ![plot](assets/plot.png) [outside](../docs/cover.md)\n"
    );
    Ok(())
}

#[test]
fn simultaneous_swaps_and_chains_do_not_reapply_a_move_or_depend_on_request_order() -> TestResult {
    let a = "file:///docs/A.md";
    let b = "file:///docs/B.md";
    let c = "file:///docs/C.md";
    let documents = buffers(&[
        (ROOT, "[A](A.md) [B](B.md)\n"),
        (a, "[B](B.md)\n"),
        (b, "[A](A.md)\n"),
    ]);
    let swapped = changed(&documents, &[(a, b), (b, a)])?;
    assert_eq!(swapped[ROOT], "[A](B.md) [B](A.md)\n");
    assert_eq!(swapped[a], "[B](A.md)\n");
    assert_eq!(swapped[b], "[A](B.md)\n");
    assert_eq!(
        request(Parser::default(), &documents, &params(&[(a, b), (b, a)])),
        request(Parser::default(), &documents, &params(&[(b, a), (a, b)]))
    );
    let chained = changed(&documents, &[(a, b), (b, c)])?;
    assert_eq!(chained[ROOT], "[A](B.md) [B](C.md)\n");
    assert_eq!(chained[a], "[B](C.md)\n");
    assert_eq!(chained[b], "[A](B.md)\n");
    Ok(())
}

#[test]
fn unique_open_extension_and_directory_aliases_follow_their_actual_chapters() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[guide](guide?view=1#intro) [index](topic/#part)\n"),
        ("file:///docs/guide.md", "# Intro\n"),
        ("file:///docs/topic/index.md", "# Part\n"),
    ]);
    let output = changed(
        &documents,
        &[
            ("file:///docs/guide.md", "file:///docs/manual.md"),
            ("file:///docs/topic/index.md", "file:///docs/new/index.md"),
        ],
    )?;
    assert_eq!(
        output[ROOT],
        "[guide](manual.md?view=1#intro) [index](new/index.md#part)\n"
    );
    Ok(())
}

#[test]
fn extensionless_images_keep_resource_identity_when_a_same_named_chapter_exists() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[chapter](guide) ![asset](guide)\n"),
        ("file:///docs/guide.md", "# Guide\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///docs/guide.md", "file:///docs/manual.md")],
    )?;
    assert_eq!(output[ROOT], "[chapter](manual.md) ![asset](guide)\n");
    let output = changed(&documents, &[("file:///docs/guide", "file:///docs/asset")])?;
    assert_eq!(output[ROOT], "[chapter](guide) ![asset](asset)\n");
    Ok(())
}

#[test]
fn shared_and_unused_definitions_require_compatible_link_and_resource_edits() -> TestResult {
    let moves = [("file:///docs/guide.md", "file:///docs/manual.md")];
    for source in [
        "[chapter][shared] ![asset][shared]\n\n[shared]: guide\n",
        "[unused]: guide\n",
    ] {
        let documents = buffers(&[(ROOT, source), ("file:///docs/guide.md", "# Guide\n")]);
        assert!(
            request(Parser::default(), &documents, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("incompatible")),
            "{source}"
        );
    }
    let source = "[chapter][shared] ![asset][shared]\n\n[shared]: guide.md\n";
    let documents = buffers(&[(ROOT, source), ("file:///docs/guide.md", "# Guide\n")]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        source.replace("guide.md", "manual.md")
    );
    Ok(())
}

#[test]
fn new_alias_collisions_are_repaired_to_the_previous_explicit_target() -> TestResult {
    for (source, target, new, expected) in [
        (
            "[go](topic/#part)\n",
            "file:///docs/topic/index.md",
            "file:///docs/topic/README.md",
            "[go](topic/index.md#part)\n",
        ),
        (
            "[go](topic#part)\n",
            "file:///docs/topic.md",
            "file:///docs/topic",
            "[go](topic.md#part)\n",
        ),
    ] {
        for opened in [false, true] {
            let mut documents = buffers(&[(ROOT, source), (target, "# Part\n")]);
            if opened {
                documents.extend(buffers(&[("file:///docs/spare.md", "# Spare\n")]));
            }
            let output = changed(&documents, &[("file:///docs/spare.md", new)])?;
            assert_eq!(output[ROOT], expected, "renamed source open: {opened}");
        }
    }
    Ok(())
}

#[test]
fn closed_folder_endpoints_cannot_capture_aliases_or_overwrite_known_subtrees() -> TestResult {
    let documents = buffers(&[
        (ROOT, "[go](guide#part)\n"),
        ("file:///docs/guide.md", "# Part\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///closed/folder/", "file:///docs/guide/")],
    )?;
    assert_eq!(output[ROOT], "[go](guide.md#part)\n");

    let documents = buffers(&[
        (ROOT, "[go](topic/#part)\n"),
        ("file:///docs/topic/index.md", "# Part\n"),
    ]);
    assert!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///closed/folder/", "file:///docs/topic/")]),
        )
        .err()
        .is_some_and(|error| error.1.contains("subtree"))
    );
    Ok(())
}

#[test]
fn unopened_exact_targets_are_supported_but_unproven_aliases_are_refused() -> TestResult {
    let moves = [("file:///docs/guide.md", "file:///docs/manual.md")];
    let documents = buffers(&[(ROOT, "[go](guide.md#intro)\n")]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        "[go](manual.md#intro)\n"
    );
    let documents = buffers(&[(ROOT, "[go](guide#intro)\n")]);
    let error = request(Parser::default(), &documents, &params(&moves))
        .err()
        .ok_or("an unopened alias must not be guessed")?;
    assert!(error.1.contains("target opened"));
    let documents = buffers(&[
        (ROOT, "[go](guide#intro)\n"),
        ("file:///docs/guide.md", "# Intro\n"),
        ("file:///docs/guide/index.md", "# Intro\n"),
    ]);
    assert!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("ambiguous"))
    );
    Ok(())
}

#[test]
fn cross_drive_and_authority_moves_use_absolute_file_uris() -> TestResult {
    let origin = "file:///C:/docs/start.md";
    let observer = "file:///C:/overview.md";
    let documents = buffers(&[
        (
            origin,
            "![art](art.png) ![root](/images/a.png) ![explicit](/C:/images/a.png) ![other](/D:/images/a.png) ![encoded](/D%3A/images/a.png) ![letter](/%43:/images/a.png)\n",
        ),
        (observer, "[go](docs/start.md)\n"),
    ]);
    let output = changed(&documents, &[(origin, "file:///D:/manual/start.md")])?;
    assert_eq!(
        output[origin],
        "![art](file:///C:/docs/art.png) ![root](/C:/images/a.png) ![explicit](/C:/images/a.png) ![other](/D:/images/a.png) ![encoded](/C:/D%3A/images/a.png) ![letter](/C:/C%3A/images/a.png)\n"
    );
    assert_eq!(output[observer], "[go](file:///D:/manual/start.md)\n");
    let origin = "file://host/share/start.md";
    let documents = buffers(&[(origin, "[sibling](other.md) [root](/share/other.md)\n")]);
    let output = changed(&documents, &[(origin, "file://other/books/start.md")])?;
    assert_eq!(
        output[origin],
        "[sibling](file://host/share/other.md) [root](file://host/share/other.md)\n"
    );
    Ok(())
}

#[test]
fn unicode_percent_encoding_and_entity_queries_preserve_exact_decoded_destinations() -> TestResult {
    let source = "\u{feff}😀 [go](<chap&#116;er.md?view=1&amp;x=2&#35;caf%C3%A9> \"Keep\")\r\n";
    let documents = buffers(&[(ROOT, source)]);
    let result = request(
        Parser::default(),
        &documents,
        &params(&[(
            "file:///docs/chapter.md",
            "file:///docs/new%20dir/é%20(draft).md",
        )]),
    )
    .map_err(|(_, reason)| reason)?;
    let output = apply_changes(&documents, &result)?;
    assert_eq!(
        output[ROOT],
        "\u{feff}😀 [go](<new%20dir/%C3%A9%20%28draft%29.md?view=1\\&x=2#caf%C3%A9> \"Keep\")\r\n"
    );
    let changes = array(result.get("documentChanges").ok_or("missing changes")?)?;
    let edits = array(changes[0].get("edits").ok_or("missing edits")?)?;
    let start = edits[0]
        .get("range")
        .and_then(|range| range.get("start"))
        .ok_or("missing start")?;
    assert_eq!(start.get("line").and_then(Json::as_u64), Some(0));
    assert_eq!(start.get("character").and_then(Json::as_u64), Some(9));
    let tokens = destination_spans(&output[ROOT]);
    assert_eq!(tokens.len(), 1);
    assert_eq!(
        tokens[0].dest,
        "new%20dir/%C3%A9%20%28draft%29.md?view=1&x=2#caf%C3%A9"
    );
    Ok(())
}

#[test]
fn code_escaped_markers_math_html_and_external_urls_remain_literal() -> TestResult {
    let source = "[real](old.md)\n\n`[code](old.md)`\n\n```markdown\n[fence](old.md)\n{{#include old.md}}\n```\n\n\\[escaped](old.md)\n\n$[math](old.md)$\n\n<a href=\"old.md\">HTML</a>\n\n[remote](https://example.test/old.md) [network](//example.test/old.md) [email](mailto:old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(
        output[ROOT],
        source.replacen("[real](old.md)", "[real](new.md)", 1)
    );
    Ok(())
}

#[test]
fn nested_containers_and_dormant_note_destinations_use_real_parser_ownership() -> TestResult {
    let source = "> [quote](old.md)\n\n- ![alt](old.md)\n\n| Header |\n| --- |\n| [cell](old.md) |\n\nTerm\n: [definition](old.md)\n\n[^unused]: [dormant](old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(output[ROOT], source.replace("old.md", "new.md"));
    Ok(())
}

#[test]
fn affected_unmapped_autolinks_refuse_the_entire_edit_instead_of_returning_a_partial_fix()
-> TestResult {
    let source = "[mapped](old.md)\n\n<file:///docs/old.md>\n";
    let documents = buffers(&[(ROOT, source)]);
    let error = request(
        Parser::default(),
        &documents,
        &params(&[("file:///docs/old.md", "file:///docs/new.md")]),
    )
    .err()
    .ok_or("an unmapped affected semantic link must refuse the edit")?;
    assert_eq!(error.0, -32803);
    assert!(error.1.contains("preserve every affected destination"));
    assert_eq!(documents[ROOT].text, source);
    Ok(())
}

#[test]
fn active_and_malformed_includes_are_refused_but_code_examples_are_safe() -> TestResult {
    let moves = [("file:///docs/old.md", "file:///docs/new.md")];
    for source in [
        "{{#include old.md}}\n",
        "{{#include \"old.md\":3:8}}\n",
        "{{#include}}\n",
    ] {
        let documents = buffers(&[(ROOT, source)]);
        assert!(
            request(Parser::default(), &documents, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("transclusion")),
            "{source}"
        );
    }
    let source = "`{{#include old.md}}`\n\n[go](old.md)\n";
    let documents = buffers(&[(ROOT, source)]);
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        "`{{#include old.md}}`\n\n[go](new.md)\n"
    );
    Ok(())
}

#[test]
fn opened_nested_includes_follow_file_moves_and_preserve_selectors_and_line_endings() -> TestResult
{
    let root = "# Start\r\n\r\n  {{#include parts/card.md:sample}}  \r\n";
    let card = "<!-- ANCHOR: sample -->\n[guide](../guide.md#guide)\n\n{{#include nested/end.txt:2:2}}\n<!-- ANCHOR_END: sample -->\n";
    let documents = buffers(&[
        (ROOT, root),
        ("file:///docs/parts/card.md", card),
        ("file:///docs/parts/nested/end.txt", "ignored\nEnding\n"),
        ("file:///docs/guide.md", "# Guide\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///docs/parts/card.md", "file:///docs/notes/card.md")],
    )?;
    assert_eq!(
        output[ROOT],
        "# Start\r\n\r\n  {{#include \"notes/card.md\":sample}}  \r\n"
    );
    assert_eq!(
        output["file:///docs/parts/card.md"],
        card.replace("nested/end.txt", "\"../parts/nested/end.txt\"")
    );
    assert_eq!(documents[ROOT].text, root);
    assert_eq!(documents["file:///docs/parts/card.md"].text, card);
    Ok(())
}

#[test]
fn include_folder_moves_rebase_incoming_paths_and_outgoing_assets() -> TestResult {
    let card =
        "[guide](../guide.md#guide) ![chart](../images/chart.png)\n\n{{#include nested/end.txt}}\n";
    let documents = buffers(&[
        (ROOT, "{{#include parts/card.md}}\n"),
        ("file:///docs/parts/card.md", card),
        ("file:///docs/parts/nested/end.txt", "Ending\n"),
        ("file:///docs/guide.md", "# Guide\n"),
    ]);
    let output = changed(
        &documents,
        &[("file:///docs/parts", "file:///docs/manual/parts")],
    )?;
    assert_eq!(output[ROOT], "{{#include \"manual/parts/card.md\"}}\n");
    assert_eq!(
        output["file:///docs/parts/card.md"],
        card.replace("../guide.md", "../../guide.md")
            .replace("../images/chart.png", "../../images/chart.png")
    );
    Ok(())
}

#[test]
fn opened_include_swaps_and_chains_are_simultaneous() -> TestResult {
    let a = "file:///docs/A.md";
    let b = "file:///docs/B.md";
    let c = "file:///docs/C.md";
    let documents = buffers(&[
        (ROOT, "{{#include A.md}}\n\n{{#include B.md}}\n"),
        (a, "A\n\n{{#include B.md}}\n"),
        (b, "B\n"),
    ]);
    let swapped = changed(&documents, &[(a, b), (b, a)])?;
    assert_eq!(
        swapped[ROOT],
        "{{#include \"B.md\"}}\n\n{{#include \"A.md\"}}\n"
    );
    assert_eq!(swapped[a], "A\n\n{{#include \"A.md\"}}\n");
    assert_eq!(
        request(Parser::default(), &documents, &params(&[(a, b), (b, a)])),
        request(Parser::default(), &documents, &params(&[(b, a), (a, b)]))
    );
    let chained = changed(&documents, &[(a, b), (b, c)])?;
    assert_eq!(
        chained[ROOT],
        "{{#include \"B.md\"}}\n\n{{#include \"C.md\"}}\n"
    );
    assert_eq!(chained[a], "A\n\n{{#include \"C.md\"}}\n");
    Ok(())
}

#[test]
fn include_paths_keep_literal_percent_query_fragment_colon_and_unicode_bytes() -> TestResult {
    let old = "file:///docs/pieces/a%2520%23%3F%3Ax.md";
    let new = "file:///docs/chunks/%E4%B8%AD%27%2520%20x.md";
    let source = "😀\r\n\r\n{{#include \"pieces/a%20#?:x.md\":2:2}}\r\n";
    let documents = buffers(&[(ROOT, source), (old, "ignored\n# Part\n")]);
    let result =
        request(Parser::default(), &documents, &params(&[(old, new)])).map_err(|(_, why)| why)?;
    let output = apply_changes(&documents, &result)?;
    assert_eq!(
        output[ROOT],
        "😀\r\n\r\n{{#include \"chunks/中'%20 x.md\":2:2}}\r\n"
    );
    assert_eq!(output[old], "ignored\n# Part\n");
    let changes = array(result.get("documentChanges").ok_or("missing changes")?)?;
    let edits = array(changes[0].get("edits").ok_or("missing edits")?)?;
    let range = edits[0].get("range").ok_or("missing range")?;
    assert_eq!(
        range
            .get("start")
            .and_then(|p| p.get("line"))
            .and_then(Json::as_u64),
        Some(2)
    );
    assert_eq!(
        range
            .get("start")
            .and_then(|p| p.get("character"))
            .and_then(Json::as_u64),
        Some(11)
    );
    Ok(())
}

#[test]
fn markdown_shaped_include_paths_do_not_become_independent_link_edits() -> TestResult {
    let resource = "file:///docs/%5Bshadow%5D%28old.md%29";
    let documents = buffers(&[
        (
            ROOT,
            "{{#include \"[shadow](old.md)\"}}\n\n[real](old.md)\n",
        ),
        (resource, "# Included\n"),
        ("file:///docs/old.md", "# Old\n"),
    ]);
    let output = changed(
        &documents,
        &[
            (resource, "file:///docs/new-fragment.md"),
            ("file:///docs/old.md", "file:///docs/guide.md"),
        ],
    )?;
    assert_eq!(
        output[ROOT],
        "{{#include \"new-fragment.md\"}}\n\n[real](guide.md)\n"
    );
    Ok(())
}

#[test]
fn include_graph_refuses_missing_cycles_invalid_selections_and_stale_buffers() -> TestResult {
    let target = "file:///docs/part.md";
    let moves = [(target, "file:///docs/new.md")];
    for (root, part) in [
        ("{{#include part.md}}\n", "{{#include missing.md}}\n"),
        ("{{#include part.md}}\n", "{{#include README.md}}\n"),
        ("{{#include part.md:missing}}\n", "Text\n"),
        ("{{#include part.md:3:4}}\n", "Text\n"),
    ] {
        let documents = buffers(&[(ROOT, root), (target, part)]);
        let error = request(Parser::default(), &documents, &params(&moves))
            .err()
            .ok_or("unsafe graph was accepted")?;
        assert_eq!(error.0, -32803);
        assert!(error.1.contains("transclusion"));
        assert_eq!(documents[ROOT].text, root);
        assert_eq!(documents[target].text, part);
    }
    let mut documents = buffers(&[(ROOT, "{{#include part.md}}\n"), (target, "Current\n")]);
    documents
        .get_mut(target)
        .ok_or("missing target")?
        .synchronized = false;
    assert_eq!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .map(|e| e.0),
        Some(-32801)
    );
    documents
        .get_mut(target)
        .ok_or("missing target")?
        .synchronized = true;
    assert_eq!(
        changed(&documents, &moves)?[ROOT],
        "{{#include \"new.md\"}}\n"
    );
    Ok(())
}

#[test]
fn selected_code_contexts_cannot_hide_unmaintained_links_or_change_literal_examples() -> TestResult
{
    let target = "file:///docs/target.md";
    let moves = [(target, "file:///docs/next.md")];
    let part = "```md\n[go](target.md)\n```\n";
    let selected = buffers(&[
        (ROOT, "{{#include part.md:2:2}}\n"),
        ("file:///docs/part.md", part),
        (target, "# Target\n"),
    ]);
    assert!(
        request(Parser::default(), &selected, &params(&moves))
            .err()
            .is_some_and(|e| e.1.contains("selected include context"))
    );
    let whole = buffers(&[
        (ROOT, "{{#include part.md}}\n"),
        ("file:///docs/part.md", part),
        (target, "# Target\n"),
    ]);
    assert_eq!(
        request(Parser::default(), &whole, &params(&moves)).map_err(|(_, why)| why)?,
        Json::Null
    );
    let quoted = buffers(&[
        (
            ROOT,
            "{{#include open.txt}}\n{{#include body.md}}\n{{#include close.txt}}\n",
        ),
        ("file:///docs/open.txt", "```md\n"),
        ("file:///docs/body.md", "[go](target.md)\n"),
        ("file:///docs/close.txt", "```\n"),
        (target, "# Target\n"),
    ]);
    assert!(
        request(Parser::default(), &quoted, &params(&moves))
            .err()
            .is_some_and(|e| e.1.contains("selected include context"))
    );
    Ok(())
}

#[test]
fn selected_fragments_that_expose_nested_includes_require_the_complete_capture() -> TestResult {
    let part = "```md\n{{#include nested.md}}\n```\n";
    let documents = buffers(&[
        (ROOT, "{{#include part.md:2:2}}\n"),
        ("file:///docs/part.md", part),
        ("file:///docs/nested.md", "Nested\n"),
    ]);
    // The selected directive is not active in the complete part.md source;
    // changing its literal code example is outside this provider's authority.
    let moves = [("file:///docs/nested.md", "file:///docs/new.md")];
    assert!(request(Parser::default(), &documents, &params(&moves)).is_err());
    // A move that changes no selected target remains usable with that capture.
    assert_eq!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/other.md", "file:///docs/elsewhere.md"),])
        )
        .map_err(|(_, why)| why)?,
        Json::Null
    );
    Ok(())
}

#[test]
fn selected_fragments_refuse_unopened_extension_and_index_alias_moves() -> TestResult {
    for (destination, target) in [
        ("target?view=1#part", "file:///docs/target.md"),
        ("topic/#part", "file:///docs/topic/index.md"),
        ("topic#part", "file:///docs/topic/README.md"),
    ] {
        let part = format!("```md\n[go]({destination})\n```\n");
        let selected = buffers(&[
            (ROOT, "{{#include part.md:2:2}}\n"),
            ("file:///docs/part.md", &part),
        ]);
        let moves = [(target, "file:///docs/next.md")];
        assert!(
            request(Parser::default(), &selected, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("target opened")),
            "an alias exposed only by selection must still prove its target: {destination}"
        );
        assert_eq!(
            request(
                Parser::default(),
                &selected,
                &params(&[("file:///docs/other.md", "file:///docs/elsewhere.md")]),
            )
            .map_err(|(_, why)| why)?,
            Json::Null,
            "unrelated moves keep the selected link and its literal source example"
        );
        let whole = buffers(&[
            (ROOT, "{{#include part.md}}\n"),
            ("file:///docs/part.md", &part),
        ]);
        assert_eq!(
            request(Parser::default(), &whole, &params(&moves)).map_err(|(_, why)| why)?,
            Json::Null,
            "a whole-source code example has no active alias to maintain"
        );
    }
    let documents = buffers(&[
        (ROOT, "{{#include part.md:2:2}}\n"),
        ("file:///docs/part.md", "```md\n![asset](target)\n```\n"),
    ]);
    assert_eq!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/target.md", "file:///docs/next.md")]),
        )
        .map_err(|(_, why)| why)?,
        Json::Null,
        "image destinations keep exact resource identities without chapter aliases"
    );
    Ok(())
}

#[test]
fn selected_aliases_refuse_incoming_closed_endpoint_shadows() -> TestResult {
    for (destination, target, shadow) in [
        (
            "target#part",
            "file:///docs/target.md",
            "file:///docs/target",
        ),
        (
            "topic/#part",
            "file:///docs/topic/index.md",
            "file:///docs/topic/README.md",
        ),
    ] {
        let part = format!("```md\n[go]({destination})\n```\n");
        let selected = buffers(&[
            (ROOT, "{{#include part.md:2:2}}\n"),
            ("file:///docs/part.md", &part),
            (target, "# Part\n"),
        ]);
        assert!(
            request(
                Parser::default(),
                &selected,
                &params(&[("file:///docs/spare.md", shadow)]),
            )
            .err()
            .is_some_and(|error| error.1.contains("captured")),
            "a selected-only alias cannot be silently redirected: {destination}"
        );
    }
    let exact = buffers(&[
        (ROOT, "{{#include part.md:2:2}}\n"),
        ("file:///docs/part.md", "```md\n[go](target.md#part)\n```\n"),
        ("file:///docs/target.md", "# Part\n"),
    ]);
    assert_eq!(
        request(
            Parser::default(),
            &exact,
            &params(&[("file:///docs/spare.md", "file:///docs/target")]),
        )
        .map_err(|(_, why)| why)?,
        Json::Null,
        "an explicit opened file target is not captured by an extension alias"
    );
    Ok(())
}

#[test]
fn selected_destinations_share_collision_ownership_with_visible_links() -> TestResult {
    let part = "```md\n[own](x.md)\n```\n";
    let moves = [("file:///docs/a/", "file:///docs/b/")];
    let conflicting = buffers(&[
        (ROOT, "[existing](b/x.md)\n\n{{#include a/part.md:2:2}}\n"),
        ("file:///docs/a/part.md", part),
    ]);
    assert!(
        request(Parser::default(), &conflicting, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("merge distinct destinations")),
        "selected hidden a/x.md must not merge with the visible closed b/x.md resource"
    );
    for (existing, expected) in [("a/x.md", "b/x.md"), ("c/x.md", "c/x.md")] {
        let source = format!("[existing]({existing})\n\n{{{{#include a/part.md:2:2}}}}\n");
        let documents = buffers(&[(ROOT, &source), ("file:///docs/a/part.md", part)]);
        let output = changed(&documents, &moves)?;
        assert_eq!(
            output[ROOT],
            format!("[existing]({expected})\n\n{{{{#include \"b/part.md\":2:2}}}}\n")
        );
        assert_eq!(output["file:///docs/a/part.md"], part);
    }
    let whole = buffers(&[
        (ROOT, "[existing](b/x.md)\n\n{{#include a/part.md}}\n"),
        ("file:///docs/a/part.md", part),
    ]);
    assert_eq!(
        changed(&whole, &moves)?[ROOT],
        "[existing](b/x.md)\n\n{{#include \"b/part.md\"}}\n",
        "literal code cannot claim an active file destination"
    );
    Ok(())
}

#[test]
fn selected_destinations_enforce_the_same_per_destination_byte_budget() -> TestResult {
    let moves = [("file:///docs/a/", "file:///docs/b/")];
    for separator in ['?', '#'] {
        let destination = format!("x.md{separator}{}", "q".repeat(9000));
        let part = format!("```md\n[own]({destination})\n```\n");
        let selected = buffers(&[
            (ROOT, "{{#include a/part.md:2:2}}\n"),
            ("file:///docs/a/part.md", &part),
        ]);
        assert!(
            request(Parser::default(), &selected, &params(&moves))
                .err()
                .is_some_and(|error| error.1.contains("rename byte budget")),
            "an oversized selected destination must not become an unaddressed identity"
        );
        let whole = buffers(&[
            (ROOT, "{{#include a/part.md}}\n"),
            ("file:///docs/a/part.md", &part),
        ]);
        assert_eq!(
            changed(&whole, &moves)?[ROOT],
            "{{#include \"b/part.md\"}}\n",
            "the destination limit does not apply to literal fenced examples"
        );
    }
    let part = "```md\n[own](x.md?view=1#part)\n```\n";
    let selected = buffers(&[
        (ROOT, "{{#include a/part.md:2:2}}\n"),
        ("file:///docs/a/part.md", part),
    ]);
    let output = changed(&selected, &moves)?;
    assert_eq!(output[ROOT], "{{#include \"b/part.md\":2:2}}\n");
    assert_eq!(output["file:///docs/a/part.md"], part);
    Ok(())
}

#[test]
fn selected_profile_controls_include_inventory_expansion_and_destination_proofs() -> TestResult {
    let parser = |profile| {
        Parser::initialize(&object([(
            "initializationOptions",
            object([("profile", string(profile))]),
        )]))
    };
    let commonmark = parser("commonmark-gfm")?;
    let plus = parser("gfm-plus")?;
    let part = concat!(
        "[Term\n: definition](../end.md#end)\n\n",
        "`Term\n: definition\n{{#include missing.md}}\ntail`\n",
    );
    assert!(
        includes::inventory(commonmark, part)
            .map_err(|(_, why)| why)?
            .is_empty()
    );
    assert_eq!(
        includes::inventory(plus, part)
            .map_err(|(_, why)| why)?
            .len(),
        1
    );
    let documents = buffers(&[
        (ROOT, "{{#include parts/shared.md}}\n"),
        ("file:///docs/parts/shared.md", part),
        ("file:///docs/end.md", "# End\n"),
    ]);
    let moves = params(&[("file:///docs/end.md", "file:///docs/finish.md")]);
    let result = request(commonmark, &documents, &moves).map_err(|(_, why)| why)?;
    let output = apply_changes(&documents, &result)?;
    assert_eq!(output[ROOT], documents[ROOT].text);
    assert_eq!(
        output["file:///docs/parts/shared.md"],
        part.replace("../end.md#end", "../finish.md#end")
    );
    assert!(
        request(plus, &documents, &moves).is_err(),
        "GFM+ sees an active include whose missing source cannot be guessed"
    );
    assert_eq!(documents["file:///docs/parts/shared.md"].text, part);
    Ok(())
}

#[test]
fn includes_refuse_new_target_capture_and_unrepresentable_host_paths() -> TestResult {
    let source = "[missing](new.md)\n\n{{#include part.md}}\n";
    let documents = buffers(&[(ROOT, source), ("file:///docs/part.md", "Text\n")]);
    assert!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/part.md", "file:///docs/new.md"),])
        )
        .is_err()
    );
    for path in [
        "/docs/part.md",
        "parts\\part.md",
        "C:/part.md",
        "C:part.md",
        ".",
        "..",
        "parts/",
    ] {
        let source = format!("{{{{#include \"{path}\"}}}}\n");
        let documents = buffers(&[(ROOT, &source), ("file:///docs/part.md", "Text\n")]);
        assert!(
            request(
                Parser::default(),
                &documents,
                &params(&[("file:///docs/part.md", "file:///docs/new.md"),])
            )
            .is_err(),
            "{path}"
        );
    }
    for new in ["file://remote/docs/new.md", "file:///D:/docs/new.md"] {
        let documents = buffers(&[
            ("file:///C:/docs/root.md", "{{#include part.md}}\n"),
            ("file:///C:/docs/part.md", "Text\n"),
        ]);
        assert!(
            request(
                Parser::default(),
                &documents,
                &params(&[("file:///C:/docs/part.md", new),])
            )
            .is_err()
        );
    }
    Ok(())
}

#[test]
fn include_expansion_and_source_copy_work_share_one_request_budget() -> TestResult {
    let payload = "x".repeat(1024 * 1024);
    let sources: Vec<_> = (0..20)
        .map(|i| {
            (
                format!("file:///docs/root-{i}.md"),
                "{{#include part.md}}\n".to_string(),
            )
        })
        .chain(std::iter::once((
            "file:///docs/part.md".to_string(),
            payload,
        )))
        .collect();
    let entries: Vec<_> = sources
        .iter()
        .map(|(uri, text)| (uri.as_str(), text.as_str()))
        .collect();
    let documents = buffers(&entries);
    let error = request(
        Parser::default(),
        &documents,
        &params(&[("file:///docs/part.md", "file:///docs/new.md")]),
    )
    .err()
    .ok_or("aggregate expansion was unbounded")?;
    assert_eq!(error.0, -32803);
    assert!(error.1.contains("bounded opened transclusion graph"));
    Ok(())
}

#[test]
fn core_include_inventory_and_serializer_preserve_exact_literal_path_ownership() -> TestResult {
    let source =
        "`literal\r\n{{#include hidden.md}}\r\nend`\r\n\r\n{{#include \"parts/a`b.md\":2:3}}\r\n";
    let tokens = transclude::include_target_spans(source, None)?;
    assert_eq!(tokens.len(), 1);
    assert_eq!(&source[tokens[0].range.clone()], "\"parts/a`b.md\"");
    assert_eq!(
        &source[tokens[0].line.clone()],
        "{{#include \"parts/a`b.md\":2:3}}\r\n"
    );
    for path in [
        "parts/a%20#?:x.md",
        "中 text.md",
        "a\"b.md",
        "a\"b'c.md",
        "a`b.md",
    ] {
        let token = transclude::include_path_token(path)?;
        let source = format!("{{{{#include {token}:sample}}}}\n");
        let tokens = transclude::include_target_spans(&source, None)?;
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].path, path);
    }
    for path in ["", "line\nbreak.md", "nul\0.md", "a\"b':c.md"] {
        assert!(transclude::include_path_token(path).is_err(), "{path:?}");
    }
    Ok(())
}

#[test]
fn stale_documents_aliases_and_known_resource_collisions_are_transactional_refusals() -> TestResult
{
    let moves = [("file:///docs/old.md", "file:///docs/new.md")];
    let mut documents = buffers(&[(ROOT, "[go](old.md)\n"), ("untitled:stale", "draft")]);
    documents
        .get_mut("untitled:stale")
        .ok_or("missing draft")?
        .synchronized = false;
    assert_eq!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .map(|error| error.0),
        Some(-32801)
    );
    let documents = buffers(&[
        (ROOT, "[go](old.md)\n"),
        ("file://localhost/docs/README.md", "other revision"),
    ]);
    assert!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("aliases"))
    );
    let documents = buffers(&[
        (ROOT, "[go](old.md)\n"),
        ("file:///docs/new.md", "# Existing\n"),
    ]);
    assert!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("collides"))
    );
    let documents = buffers(&[(ROOT, "[old](old.md) ![existing](new.md)\n")]);
    assert!(
        request(Parser::default(), &documents, &params(&moves))
            .err()
            .is_some_and(|error| error.1.contains("merge distinct"))
    );
    Ok(())
}

#[test]
fn request_validation_rejects_roots_overlaps_duplicate_identities_and_invalid_uris() -> TestResult {
    let documents = buffers(&[(ROOT, "[go](old.md)\n")]);
    for moves in [
        vec![("file:///", "file:///moved")],
        vec![("file:///C:/", "file:///D:/moved")],
        vec![("https://example.test/old.md", "file:///new.md")],
        vec![("file:///docs/old.md?x=1", "file:///new.md")],
        vec![("file:///docs/%2F.md", "file:///new.md")],
        vec![("file:///docs/old.md", "file:///docs/old.md/child.md")],
        vec![
            ("file:///docs", "file:///new"),
            ("file:///docs/old.md", "file:///other.md"),
        ],
        vec![
            ("file:///docs/old.md", "file:///new.md"),
            ("file://localhost/docs/old.md", "file:///other.md"),
        ],
        vec![
            ("file:///docs/old.md", "file:///new.md"),
            ("file:///docs/other.md", "file://localhost/new.md"),
        ],
    ] {
        assert!(
            request(Parser::default(), &documents, &params(&moves)).is_err(),
            "{moves:?}"
        );
    }
    assert_eq!(
        request(Parser::default(), &documents, &object([]))
            .err()
            .map(|error| error.0),
        Some(-32602)
    );
    assert_eq!(
        request(Parser::default(), &documents, &params(&[])).map_err(|(_, reason)| reason)?,
        Json::Null
    );
    assert_eq!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/old.md", "file://localhost/docs/old.md")])
        )
        .map_err(|(_, reason)| reason)?,
        Json::Null
    );
    Ok(())
}

#[test]
fn operation_document_and_response_budgets_refuse_incomplete_changes() -> TestResult {
    let documents = buffers(&[(ROOT, "[go](old.md)\n")]);
    let files = Json::Array(
        (0..129)
            .map(|index| {
                object([
                    ("oldUri", string(&format!("file:///old/{index}.md"))),
                    ("newUri", string(&format!("file:///new/{index}.md"))),
                ])
            })
            .collect(),
    );
    assert!(
        request(Parser::default(), &documents, &object([("files", files)]))
            .err()
            .is_some_and(|error| error.1.contains("128-operation"))
    );
    let oversized = "x".repeat(MAX_DOCUMENT_BYTES + 1);
    let documents = buffers(&[(ROOT, &oversized)]);
    assert!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/old.md", "file:///docs/new.md")])
        )
        .err()
        .is_some_and(|error| error.1.contains("source budget"))
    );
    let source = "[go](old.md) ".repeat(MAX_EDITS + 1);
    let documents = buffers(&[(ROOT, &source)]);
    assert!(
        request(
            Parser::default(),
            &documents,
            &params(&[("file:///docs/old.md", "file:///docs/new.md")])
        )
        .err()
        .is_some_and(|error| error.1.contains("edit response budget"))
    );
    Ok(())
}

#[test]
fn opaque_buffers_allow_explicit_file_links_but_never_infer_a_relative_directory() -> TestResult {
    let origin = "untitled:draft";
    let documents = buffers(&[(
        origin,
        "[absolute](file:///docs/old.md#part) [relative](old.md#part)\n",
    )]);
    let output = changed(
        &documents,
        &[("file:///docs/old.md", "file:///docs/new.md")],
    )?;
    assert_eq!(
        output[origin],
        "[absolute](file:///docs/new.md#part) [relative](old.md#part)\n"
    );
    Ok(())
}

#[test]
fn initialize_negotiates_versioned_file_operation_edits_and_dispatch_keeps_buffers_unchanged()
-> TestResult {
    for (versions, operations, enabled) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (true, true, true),
    ] {
        let mut server = Server::default();
        let initialize = object([
            ("jsonrpc", string("2.0")),
            ("id", number(1)),
            ("method", string("initialize")),
            (
                "params",
                object([(
                    "capabilities",
                    object([(
                        "workspace",
                        object([
                            (
                                "workspaceEdit",
                                object([("documentChanges", Json::Bool(versions))]),
                            ),
                            (
                                "fileOperations",
                                object([("willRename", Json::Bool(operations))]),
                            ),
                        ]),
                    )]),
                )]),
            ),
        ]);
        let replies = server.handle(initialize);
        assert_eq!(server.file_renames, enabled);
        assert!(server.phase == Phase::Running);
        let advertised = replies[0]
            .get("result")
            .and_then(|result| result.get("capabilities"))
            .and_then(|caps| caps.get("workspace"))
            .and_then(|workspace| workspace.get("fileOperations"))
            .and_then(|operations| operations.get("willRename"));
        assert_eq!(advertised.is_some(), enabled);
        server.documents = buffers(&[(ROOT, "[go](old.md)\n")]);
        let call = object([
            ("jsonrpc", string("2.0")),
            ("id", number(2)),
            ("method", string("workspace/willRenameFiles")),
            (
                "params",
                params(&[("file:///docs/old.md", "file:///docs/new.md")]),
            ),
        ]);
        let replies = server.handle(call);
        if enabled {
            assert_eq!(
                apply_changes(
                    &server.documents,
                    replies[0].get("result").ok_or("missing result")?
                )?[ROOT],
                "[go](new.md)\n"
            );
        } else {
            assert_eq!(
                replies[0]
                    .get("error")
                    .and_then(|error| error.get("code"))
                    .and_then(integer),
                Some(-32601)
            );
        }
        assert_eq!(server.documents[ROOT].text, "[go](old.md)\n");
    }
    Ok(())
}

#[test]
fn opened_include_proof_accepts_unicode_at_the_end_of_a_destination_token() -> TestResult {
    for destination in ["../guide.md#終", "../guide.md#終😀", "../終"] {
        let card = format!("[go]({destination})\n");
        let documents = buffers(&[
            (ROOT, "{{#include parts/card.md}}\n"),
            ("file:///docs/parts/card.md", &card),
            ("file:///docs/guide.md", "# 終😀\n"),
            ("file:///docs/終", "# Target\n"),
        ]);
        let output = changed(
            &documents,
            &[("file:///docs/parts/card.md", "file:///docs/archive/card.md")],
        )?;
        assert_eq!(output[ROOT], "{{#include \"archive/card.md\"}}\n");
        assert_eq!(output["file:///docs/parts/card.md"], card);
    }
    Ok(())
}

#[test]
fn include_origin_proof_checks_interior_mapping_segments_and_generated_bytes() -> TestResult {
    use transclude::{ExpandedDocument, ExpansionSpan};
    let span = |expanded, source_id, original, generated| ExpansionSpan {
        expanded,
        source_id,
        original,
        generated,
    };
    // Synthetic sidecars directly exercise proof admission. The outer source
    // offsets alone agree in every case; the middle segment decides ownership.
    let mut capture = ExpandedDocument {
        text: "abc".to_string(),
        sources: vec!["file:///docs/a.md".into(), "file:///docs/b.md".into()],
        spans: vec![
            span(0..1, 0, 0..1, false),
            span(1..2, 0, 1..2, false),
            span(2..3, 0, 2..3, false),
        ],
    };
    assert_eq!(
        includes::token_origin(&capture, &(0..3)).map_err(|(_, why)| why)?,
        "file:///docs/a.md"
    );
    capture.spans[1].source_id = 1;
    assert!(includes::token_origin(&capture, &(0..3)).is_err());
    capture.spans[1].source_id = 0;
    capture.spans[1].original = 9..10;
    assert!(includes::token_origin(&capture, &(0..3)).is_err());
    capture.spans[1].original = 1..2;
    capture.spans[1].generated = true;
    assert!(includes::token_origin(&capture, &(0..3)).is_err());
    Ok(())
}
