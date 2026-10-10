//! PDF running header/footer for MCP (GH #13), mirroring the browser shape:
//! `{"header": {"left", "center", "right", "rule", "image"}, "footer": {...},
//! "skipFirstPage": bool}`. Unknown fields and wrong types are refused.

use super::{JsonValue, ToolError, invalid_options};
use crate::{PdfRunningBand, PdfRunningContent, PdfRunningImage, PdfRunningImagePosition};
use std::collections::BTreeMap;

const SLOTS: [&str; 3] = ["left", "center", "right"];
const MAX_SLOT_BYTES: usize = 4096;

fn error(message: impl Into<String>) -> ToolError {
    invalid_options(message, "invalid_running")
}

fn record<'a>(
    value: &'a JsonValue,
    allowed: &[&str],
    label: &str,
) -> Result<&'a BTreeMap<String, JsonValue>, ToolError> {
    let object = value
        .as_object()
        .ok_or_else(|| error(format!("{label} must be an object")))?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(error(format!("Unsupported {label} field: '{key}'")));
        }
    }
    Ok(object)
}

fn image(value: Option<&JsonValue>, label: &str) -> Result<Option<PdfRunningImage>, ToolError> {
    let Some(value) = value else { return Ok(None) };
    let object = record(value, &["dest", "position", "heightPt"], label)?;
    let dest = object
        .get("dest")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|dest| !dest.is_empty() && dest.len() <= 4096)
        .ok_or_else(|| error(format!("{label}.dest requires 1..4096 UTF-8 bytes")))?;
    let position = match object.get("position").map(JsonValue::as_str) {
        None | Some(Some("left")) => PdfRunningImagePosition::Left,
        Some(Some("right")) => PdfRunningImagePosition::Right,
        _ => return Err(error(format!("{label}.position must be left or right"))),
    };
    let height_pt = object
        .get("heightPt")
        .map(|value| {
            value
                .as_u64()
                .and_then(|height| u16::try_from(height).ok())
                .filter(|height| *height > 0)
                .ok_or_else(|| {
                    error(format!(
                        "{label}.heightPt must be an integer in 1..65535 points"
                    ))
                })
        })
        .transpose()?;
    Ok(Some(PdfRunningImage {
        dest: dest.into(),
        position,
        height_pt,
    }))
}

fn band(value: Option<&JsonValue>, label: &str) -> Result<PdfRunningBand, ToolError> {
    let Some(value) = value else {
        return Ok(PdfRunningBand::default());
    };
    let object = record(value, &["left", "center", "right", "rule", "image"], label)?;
    let mut slots: [Option<String>; 3] = Default::default();
    for (slot, name) in slots.iter_mut().zip(SLOTS) {
        let Some(value) = object.get(name) else {
            continue;
        };
        let JsonValue::String(text) = value else {
            return Err(error(format!("{label}.{name} must be a string")));
        };
        if text.len() > MAX_SLOT_BYTES {
            return Err(error(format!(
                "{label}.{name} exceeds {MAX_SLOT_BYTES} UTF-8 bytes"
            )));
        }
        *slot = Some(text.clone()).filter(|text| !text.is_empty());
    }
    let rule = match object.get("rule") {
        None => false,
        Some(JsonValue::Bool(rule)) => *rule,
        Some(_) => return Err(error(format!("{label}.rule must be a boolean"))),
    };
    let [left, center, right] = slots;
    Ok(PdfRunningBand {
        left,
        center,
        right,
        rule,
        image: image(object.get("image"), &format!("{label}.image"))?,
    })
}

pub(super) fn parse(value: Option<&JsonValue>) -> Result<PdfRunningContent, ToolError> {
    let Some(value) = value else {
        return Ok(PdfRunningContent::default());
    };
    let object = record(value, &["header", "footer", "skipFirstPage"], "running")?;
    let skip_first_page = match object.get("skipFirstPage") {
        None => false,
        Some(JsonValue::Bool(skip)) => *skip,
        Some(_) => return Err(error("running.skipFirstPage must be a boolean")),
    };
    Ok(PdfRunningContent {
        header: band(object.get("header"), "running.header")?,
        footer: band(object.get("footer"), "running.footer")?,
        skip_first_page,
    })
}

fn typed(kind: &str, description: &str) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String(kind.into())),
        ("description".into(), JsonValue::String(description.into())),
    ]))
}

fn band_schema(description: &str) -> JsonValue {
    let mut properties: BTreeMap<String, JsonValue> = SLOTS
        .iter()
        .map(|slot| {
            (
                (*slot).to_string(),
                typed(
                    "string",
                    "Slot template; tokens {page} {pages} {title} {author} {date}",
                ),
            )
        })
        .collect();
    properties.insert(
        "rule".into(),
        typed("boolean", "Hairline under the header / over the footer"),
    );
    properties.insert("image".into(), JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String("object".into())),
        ("description".into(), JsonValue::String("Repeated logo from an explicitly supplied images asset; never loads a URL or file. Text fits beside the image.".into())),
        ("required".into(), JsonValue::Array(vec![JsonValue::String("dest".into())])),
        ("additionalProperties".into(), JsonValue::Bool(false)),
        ("properties".into(), JsonValue::Object(BTreeMap::from([
            ("dest".into(), typed("string", "Image asset destination key, 1..4096 UTF-8 bytes")),
            ("position".into(), JsonValue::Object(BTreeMap::from([
                ("type".into(), JsonValue::String("string".into())),
                ("enum".into(), JsonValue::Array(vec![JsonValue::String("left".into()), JsonValue::String("right".into())])),
                ("default".into(), JsonValue::String("left".into())),
            ]))),
            ("heightPt".into(), JsonValue::Object(BTreeMap::from([
                ("type".into(), JsonValue::String("integer".into())),
                ("minimum".into(), JsonValue::Number(1.0)),
                ("maximum".into(), JsonValue::Number(65535.0)),
                ("description".into(), JsonValue::String("Optional height in points; omission fits the margin automatically".into())),
            ]))),
        ]))),
    ])));
    JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String("object".into())),
        ("description".into(), JsonValue::String(description.into())),
        ("properties".into(), JsonValue::Object(properties)),
        ("additionalProperties".into(), JsonValue::Bool(false)),
    ]))
}

pub(super) fn schema() -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String("object".into())),
        ("description".into(), JsonValue::String(
            "PDF running header/footer drawn in the page margins. {date} comes from metadataEpochSeconds (never the clock); unknown tokens stay literal. A band that does not fit its margin fails the render. pageNumbers is sugar for footer.center '{page}'. File targets: pdf, both.".into(),
        )),
        ("properties".into(), JsonValue::Object(BTreeMap::from([
            ("header".into(), band_schema("Band in the top margin")),
            ("footer".into(), band_schema("Band in the bottom margin")),
            ("skipFirstPage".into(), typed("boolean", "Leave page 1 bare")),
        ]))),
        ("additionalProperties".into(), JsonValue::Bool(false)),
    ]))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::mcp::parse_json;

    fn parse_running(json: &str) -> Result<PdfRunningContent, ToolError> {
        parse(Some(&parse_json(json).unwrap()))
    }

    #[test]
    fn omitted_and_empty_running_are_the_default() {
        assert_eq!(parse(None).unwrap(), PdfRunningContent::default());
        assert_eq!(parse_running("{}").unwrap(), PdfRunningContent::default());
        assert_eq!(
            parse_running(r#"{"header":{"left":""}}"#).unwrap(),
            PdfRunningContent::default()
        );
    }

    #[test]
    fn slots_rules_and_first_page_reach_native_options() {
        let running = parse_running(r#"{"header":{"right":"{title}","rule":true},"footer":{"center":"{page} / {pages}"},"skipFirstPage":true}"#).unwrap();
        assert_eq!(running.header.right.as_deref(), Some("{title}"));
        assert!(running.header.rule && !running.footer.rule && running.skip_first_page);
        assert_eq!(running.footer.center.as_deref(), Some("{page} / {pages}"));
        assert_eq!(running.footer.left, None);
    }

    #[test]
    fn image_settings_reference_host_assets_with_exact_integer_heights() {
        let running = parse_running(r#"{"header":{"image":{"dest":" logo.svg ","position":"right","heightPt":24}},"footer":{"image":{"dest":"mark.png"}},"skipFirstPage":true}"#).unwrap();
        let header = running.header.image.unwrap();
        assert_eq!(header.dest, "logo.svg");
        assert_eq!(header.position, PdfRunningImagePosition::Right);
        assert_eq!(header.height_pt, Some(24));
        let footer = running.footer.image.unwrap();
        assert_eq!(footer.position, PdfRunningImagePosition::Left);
        assert_eq!(footer.height_pt, None);
        assert!(running.skip_first_page);
        for image in [
            "null",
            "[]",
            "{}",
            r#"{"dest":" "}"#,
            r#"{"dest":2}"#,
            r#"{"dest":"logo","position":"center"}"#,
            r#"{"dest":"logo","heightPt":0}"#,
            r#"{"dest":"logo","heightPt":1.5}"#,
            r#"{"dest":"logo","heightPt":65536}"#,
            r#"{"dest":"logo","heightPt":"24"}"#,
            r#"{"dest":"logo","bytes":[1,2]}"#,
        ] {
            let input = format!(r#"{{"header":{{"image":{image}}}}}"#);
            assert_eq!(
                parse_running(&input).unwrap_err().2,
                "invalid_running",
                "{input}"
            );
        }
    }

    #[test]
    fn malformed_running_fails_closed() {
        let long = format!(
            r#"{{"footer":{{"left":"{}"}}}}"#,
            "x".repeat(MAX_SLOT_BYTES + 1)
        );
        for json in [
            "null",
            "[]",
            r#"{"top":{}}"#,
            r#"{"header":[]}"#,
            r#"{"header":{"middle":"x"}}"#,
            r#"{"footer":{"left":3}}"#,
            r#"{"footer":{"rule":"yes"}}"#,
            r#"{"skipFirstPage":1}"#,
            r#"{"header":null}"#,
            long.as_str(),
        ] {
            assert_eq!(
                parse_running(json).unwrap_err().2,
                "invalid_running",
                "{json}"
            );
        }
    }

    #[test]
    fn schema_refuses_extra_fields_at_every_level() {
        let schema = schema();
        assert_eq!(
            schema.get("additionalProperties"),
            Some(&JsonValue::Bool(false))
        );
        let header = schema.get("properties").unwrap().get("header").unwrap();
        assert_eq!(
            header.get("additionalProperties"),
            Some(&JsonValue::Bool(false))
        );
        let image = header.get("properties").unwrap().get("image").unwrap();
        assert_eq!(
            image.get("additionalProperties"),
            Some(&JsonValue::Bool(false))
        );
        assert_eq!(
            image.get("required"),
            Some(&JsonValue::Array(vec![JsonValue::String("dest".into())]))
        );
    }
}
