//! MCP collection selection must reach the shared renderer as standalone fonts.
#![cfg(feature = "mcp")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;

use franken_markdown::mcp::{
    ERROR_INVALID_OPTIONS, JsonValue as Json, base64_encode, handle_tool_call, tools_list_result,
};

const TTC: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV1.ttc");
const OTC: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV2.otc");
const TTF: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdCollectionTrueType.ttf");
const CFF: &[u8] = include_bytes!("../fmd-font/fonts/test-cid-cff/FmdCidFormat3.otf");
const SOURCE: &str = "# AB\n\nAB AB.\n";
type Args = BTreeMap<String, Json>;
type Failure = (i32, String, &'static str);

fn text(value: &str) -> Json {
    Json::String(value.to_owned())
}

fn object<const N: usize>(fields: [(&str, Json); N]) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

fn array(value: &Json) -> &[Json] {
    match value {
        Json::Array(items) => items,
        _ => panic!("expected array"),
    }
}

fn font(bytes: &[u8], face: Option<Json>) -> Json {
    let mut fields = BTreeMap::from([
        ("slot".to_owned(), text("body-regular")),
        ("base64".to_owned(), text(&base64_encode(bytes))),
        ("weight".to_owned(), Json::Number(450.0)),
    ]);
    if let Some(face) = face {
        fields.insert("faceIndex".to_owned(), face);
    }
    Json::Object(fields)
}

fn args(bytes: &[u8], face: Option<Json>) -> Args {
    BTreeMap::from([
        ("markdown".to_owned(), text(SOURCE)),
        ("fonts".to_owned(), Json::Array(vec![font(bytes, face)])),
    ])
}

fn call(name: &str, args: Args) -> Result<Json, Failure> {
    handle_tool_call(
        Some(&object([
            ("name", text(name)),
            ("arguments", Json::Object(args)),
        ])),
        1024 * 1024,
    )
}

#[test]
fn selected_truetype_and_cff_faces_match_independent_standalone_publications() {
    for collection in [TTC, OTC] {
        for (index, standalone) in [TTF, CFF].into_iter().enumerate() {
            for tool in [
                "fmd.render_html",
                "fmd.render_pdf",
                "fmd.render_svg",
                "fmd.render_epub",
                "fmd.verify",
            ] {
                let selected =
                    call(tool, args(collection, Some(Json::Number(index as f64)))).unwrap();
                let expected = call(tool, args(standalone, None)).unwrap();
                assert_eq!(selected, expected, "{tool}, face {index}");
            }
        }
    }
}

#[test]
fn each_font_slot_selects_its_own_face_in_a_multiformat_book() {
    let fonts = |collection: bool| {
        let mut regular = font(if collection { TTC } else { TTF }, None);
        let mut bold = font(if collection { TTC } else { CFF }, None);
        if let Json::Object(fields) = &mut regular
            && collection
        {
            fields.insert("faceIndex".to_owned(), Json::Number(0.0));
        }
        if let Json::Object(fields) = &mut bold {
            fields.insert("slot".to_owned(), text("body-bold"));
            if collection {
                fields.insert("faceIndex".to_owned(), Json::Number(1.0));
            }
        }
        Json::Array(vec![regular, bold])
    };
    let request = |collection| {
        BTreeMap::from([
            (
                "chapters".to_owned(),
                Json::Array(vec![
                    object([
                        ("path", text("one.md")),
                        ("markdown", text("# AB\n\nAB **AB**.")),
                    ]),
                    object([
                        ("path", text("two.md")),
                        ("markdown", text("# BA\n\nBA **BA**.")),
                    ]),
                ]),
            ),
            ("to".to_owned(), text("all")),
            ("fonts".to_owned(), fonts(collection)),
        ])
    };
    let selected = call("fmd.render_book", request(true)).unwrap();
    let expected = call("fmd.render_book", request(false)).unwrap();
    assert_eq!(selected, expected);
}

#[test]
fn face_indices_are_strict_and_admitted_before_base64_decoding() {
    for face in [
        Json::Null,
        Json::Bool(false),
        text("0"),
        Json::Number(-1.0),
        Json::Number(0.5),
        Json::Number(1024.0),
        Json::Number(f64::INFINITY),
        Json::Number(f64::NAN),
    ] {
        let mut request = args(TTC, Some(face));
        if let Json::Array(fonts) = request.get_mut("fonts").unwrap()
            && let Json::Object(fields) = &mut fonts[0]
        {
            // Noncanonical padding would fail decoding if admission ran late.
            fields.insert("base64".to_owned(), text("AB=="));
        }
        let failure = call("fmd.render_html", request).unwrap_err();
        assert_eq!(failure.0, ERROR_INVALID_OPTIONS);
        assert_eq!(failure.2, "invalid_resources");
        assert!(failure.1.contains("faceIndex"), "{failure:?}");
    }
}

#[test]
fn collection_and_standalone_selection_errors_remain_actionable() {
    for (bytes, index, message) in [
        (TTC, None, "supply its zero-based faceIndex"),
        (TTF, Some(0.0), "omit faceIndex"),
        (CFF, Some(1.0), "omit faceIndex"),
        (TTC, Some(2.0), "InvalidFaceIndex"),
        (OTC, Some(1023.0), "InvalidFaceIndex"),
        (b"ttcf".as_slice(), Some(0.0), "Malformed"),
    ] {
        let failure = call("fmd.render_pdf", args(bytes, index.map(Json::Number))).unwrap_err();
        assert_eq!(failure.0, ERROR_INVALID_OPTIONS);
        assert_eq!(failure.2, "invalid_resources");
        assert!(failure.1.contains("body-regular"), "{failure:?}");
        assert!(failure.1.contains(message), "{failure:?}");
    }
}

#[test]
fn face_selection_does_not_expand_image_fields_or_allow_duplicate_font_slots() {
    let mut request = args(TTF, None);
    request.insert(
        "images".to_owned(),
        Json::Array(vec![object([
            ("destination", text("image")),
            ("base64", text("AA==")),
            ("faceIndex", Json::Number(0.0)),
        ])]),
    );
    assert_eq!(
        call("fmd.render_html", request).unwrap_err().2,
        "invalid_resources"
    );
    let mut request = args(TTC, Some(Json::Number(0.0)));
    if let Json::Array(fonts) = request.get_mut("fonts").unwrap() {
        fonts.push(font(TTC, Some(Json::Number(1.0))));
    }
    let failure = call("fmd.verify", request).unwrap_err();
    assert_eq!(failure.2, "invalid_resources");
    assert!(failure.1.contains("Duplicate font slot"), "{failure:?}");
}

#[test]
fn every_resource_consumer_advertises_the_same_bounded_face_selector() {
    let listing = tools_list_result();
    let mut checked = 0;
    for tool in array(listing.get("tools").unwrap()) {
        let fields = tool.get("inputSchema").unwrap().get("properties").unwrap();
        let Some(fonts) = fields.get("fonts") else {
            continue;
        };
        checked += 1;
        let items = fonts.get("items").unwrap();
        let selector = items.get("properties").unwrap().get("faceIndex").unwrap();
        assert_eq!(selector.get("type"), Some(&text("integer")));
        assert_eq!(selector.get("minimum").and_then(Json::as_u64), Some(0));
        assert_eq!(selector.get("maximum").and_then(Json::as_u64), Some(1023));
        assert!(!array(items.get("required").unwrap()).contains(&text("faceIndex")));
        assert_eq!(items.get("additionalProperties"), Some(&Json::Bool(false)));
        let image = fields.get("images").unwrap().get("items").unwrap();
        assert!(image.get("properties").unwrap().get("faceIndex").is_none());
    }
    assert_eq!(
        checked, 8,
        "all six publication tools, document verification and book PDF verification"
    );
}
