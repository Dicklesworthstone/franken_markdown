//! Explicit PDF paper geometry for MCP, in the browser API's point units.
//!
//! Convert only after admission at host precision, then recheck the exact f32
//! arithmetic used by the renderer. No MediaBox patching or output scaling.

use super::{JsonValue, ToolError, invalid_options};
use crate::{PageMargins, PageSize, PageStyle};
use std::collections::BTreeMap;

const SIDES: [&str; 4] = ["topPt", "rightPt", "bottomPt", "leftPt"];
const MAX_POINT: f64 = 14_400.0;
const MIN_PAPER: f64 = 144.0;
const MIN_CONTENT: f64 = 72.0;

fn error(message: impl Into<String>) -> ToolError {
    invalid_options(message, "invalid_page")
}

fn record<'a>(value: &'a JsonValue, allowed: &[&str], label: &str)
    -> Result<&'a BTreeMap<String, JsonValue>, ToolError>
{
    let object = value.as_object().ok_or_else(|| error(format!("{label} must be an object")))?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(error(format!("Unsupported {label} field: '{key}'")));
        }
    }
    Ok(object)
}

fn point(value: &JsonValue, minimum: f64, label: &str) -> Result<f64, ToolError> {
    let value = value.as_f64().filter(|value| value.is_finite()
        && (minimum..=MAX_POINT).contains(value))
        .ok_or_else(|| error(format!("{label} must be a finite number from {minimum} to {MAX_POINT} points")))?;
    Ok(if value == 0.0 { 0.0 } else { value })
}

pub(super) fn parse(value: Option<&JsonValue>) -> Result<Option<PageStyle>, ToolError> {
    let Some(value) = value else { return Ok(None) };
    let page = record(value, &["size", "orientation", "margins"], "page")?;
    let (mut width, mut height) = match page.get("size") {
        None => (612.0, 792.0),
        Some(JsonValue::String(name)) => match name.as_str() {
            "letter" => (612.0, 792.0),
            "a4" => (210.0 * 72.0 / 25.4, 297.0 * 72.0 / 25.4),
            _ => return Err(error("page.size must be 'letter', 'a4', or explicit dimensions")),
        },
        Some(value) => {
            let size = record(value, &["widthPt", "heightPt"], "page.size")?;
            let width = size.get("widthPt").ok_or_else(|| error("page.size requires widthPt"))?;
            let height = size.get("heightPt").ok_or_else(|| error("page.size requires heightPt"))?;
            (point(width, MIN_PAPER, "page width")?, point(height, MIN_PAPER, "page height")?)
        }
    };
    if let Some(orientation) = page.get("orientation") {
        let rotate = match orientation.as_str() {
            Some("portrait") => width > height,
            Some("landscape") => height > width,
            _ => return Err(error("page.orientation must be 'portrait' or 'landscape'")),
        };
        if rotate { std::mem::swap(&mut width, &mut height); }
    }
    let mut margins = [72.0; 4];
    match page.get("margins") {
        None => {}
        Some(value @ JsonValue::Number(_)) => margins.fill(point(value, 0.0, "page margins")?),
        Some(value) => {
            let sides = record(value, &SIDES, "page.margins")?;
            for (i, name) in SIDES.iter().enumerate() {
                if let Some(value) = sides.get(*name) { margins[i] = point(value, 0.0, name)?; }
            }
        }
    }
    let [top, right, bottom, left] = margins;
    if width - left - right < MIN_CONTENT || height - top - bottom < MIN_CONTENT {
        return Err(error("page margins must leave at least 72 points of content width and height"));
    }
    let [width, height, top, right, bottom, left] =
        [width, height, top, right, bottom, left].map(|value| value as f32);
    if width - left - right < MIN_CONTENT as f32 || height - top - bottom < MIN_CONTENT as f32 {
        return Err(error("page margins leave less than 72 points after renderer precision conversion"));
    }
    let size = if width == PageSize::LETTER.width_pt && height == PageSize::LETTER.height_pt {
        PageSize::LETTER
    } else {
        PageSize { name: "custom", width_pt: width, height_pt: height }
    };
    Ok(Some(PageStyle {
        size, margins: PageMargins { top_pt: top, right_pt: right, bottom_pt: bottom, left_pt: left },
    }))
}

fn object(properties: Vec<(&str, JsonValue)>, required: &[&str]) -> JsonValue {
    let mut result = BTreeMap::from([
        ("type".into(), JsonValue::String("object".into())),
        ("properties".into(), JsonValue::Object(properties.into_iter()
            .map(|(name, value)| (name.to_string(), value)).collect())),
        ("additionalProperties".into(), JsonValue::Bool(false)),
    ]);
    if !required.is_empty() {
        result.insert("required".into(), JsonValue::Array(required.iter()
            .map(|name| JsonValue::String((*name).into())).collect()));
    }
    JsonValue::Object(result)
}

fn choices(values: &[&str]) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String("string".into())),
        ("enum".into(), JsonValue::Array(values.iter().map(|value| JsonValue::String((*value).into())).collect())),
    ]))
}

fn number(minimum: f64) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("type".into(), JsonValue::String("number".into())),
        ("minimum".into(), JsonValue::Number(minimum)),
        ("maximum".into(), JsonValue::Number(MAX_POINT)),
    ]))
}

fn either(first: JsonValue, second: JsonValue) -> JsonValue {
    JsonValue::Object(BTreeMap::from([("oneOf".into(), JsonValue::Array(vec![first, second]))]))
}

pub(super) fn schema() -> JsonValue {
    let mut schema = object(vec![
        ("size", either(choices(&["letter", "a4"]), object(vec![
            ("widthPt", number(MIN_PAPER)), ("heightPt", number(MIN_PAPER)),
        ], &["widthPt", "heightPt"]))),
        ("orientation", choices(&["portrait", "landscape"])),
        ("margins", either(number(0.0), object(SIDES.iter()
            .map(|side| (*side, number(0.0))).collect(), &[]))),
    ], &[]);
    if let JsonValue::Object(fields) = &mut schema {
        fields.insert("description".into(), JsonValue::String(
            "PDF paper and margins in points. Defaults: Letter, 72-point margins. Orientation rotates paper only. Margins must leave a 72-point content rectangle at host and renderer precision. File targets: pdf, both.".into(),
        ));
    }
    schema
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use crate::mcp::parse_json;

    fn parse_page(json: &str) -> Result<PageStyle, ToolError> {
        parse(Some(&parse_json(json).unwrap())).map(|page| page.unwrap())
    }

    #[test]
    fn omitted_and_explicit_default_page_preserve_letter_geometry() {
        assert!(parse(None).unwrap().is_none());
        let page = parse_page("{}").unwrap();
        assert_eq!(page.size, PageSize::LETTER);
        assert_eq!(page.margins.top_pt, 72.0);
        assert_eq!(page.margins.right_pt, 72.0);
        assert_eq!(page.margins.bottom_pt, 72.0);
        assert_eq!(page.margins.left_pt, 72.0);
    }

    #[test]
    fn paper_rotation_never_rotates_independent_margin_sides() {
        let page = parse_page(r#"{"size":"letter","orientation":"landscape","margins":{"topPt":20,"rightPt":30,"bottomPt":40,"leftPt":50}}"#).unwrap();
        assert_eq!((page.size.width_pt, page.size.height_pt), (792.0, 612.0));
        assert_eq!((page.margins.top_pt, page.margins.right_pt, page.margins.bottom_pt, page.margins.left_pt), (20.0, 30.0, 40.0, 50.0));
        let page = parse_page(r#"{"size":{"widthPt":900,"heightPt":500},"margins":0}"#).unwrap();
        assert_eq!((page.size.width_pt, page.size.height_pt), (900.0, 500.0));
    }

    #[test]
    fn a4_and_partial_margin_defaults_match_browser_point_contract() {
        let page = parse_page(r#"{"size":"a4","margins":{"leftPt":36}}"#).unwrap();
        assert_eq!(page.size.width_pt, (210.0_f64 * 72.0 / 25.4) as f32);
        assert_eq!(page.size.height_pt, (297.0_f64 * 72.0 / 25.4) as f32);
        assert_eq!(page.margins.left_pt, 36.0);
        assert_eq!(page.margins.right_pt, 72.0);
    }

    #[test]
    fn malformed_objects_and_invalid_geometry_fail_closed() {
        for json in ["null", "[]", r#"{"size":"legal"}"#, r#"{"size":{}}"#,
            r#"{"size":{"widthPt":300,"heightPt":500,"extra":1}}"#,
            r#"{"orientation":"sideways"}"#, r#"{"margins":null}"#,
            r#"{"margins":{"left":10}}"#, r#"{"margins":-1}"#,
            r#"{"size":{"widthPt":143,"heightPt":300},"margins":0}"#,
            r#"{"size":{"widthPt":14401,"heightPt":300}}"#,
            r#"{"margins":400}"#, r#"{"unknown":false}"#] {
            assert_eq!(parse_page(json).unwrap_err().2, "invalid_page", "{json}");
        }
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let value = JsonValue::Object(BTreeMap::from([("margins".into(), JsonValue::Number(invalid))]));
            assert_eq!(parse(Some(&value)).unwrap_err().2, "invalid_page");
        }
    }

    #[test]
    fn limits_and_host_precision_cannot_be_bypassed_by_rounding() {
        assert!(parse_page(r#"{"size":{"widthPt":144,"heightPt":144},"margins":36}"#).is_ok());
        assert!(parse_page(r#"{"size":{"widthPt":14400,"heightPt":14400},"margins":0}"#).is_ok());
        assert!(parse_page(r#"{"size":{"widthPt":144,"heightPt":144},"margins":{"leftPt":36,"rightPt":36.0000001,"topPt":0,"bottomPt":0}}"#).is_err());
        // Host subtraction leaves 72 points; sequential f32 subtraction does not.
        assert!(parse_page(r#"{"size":{"widthPt":144.00731624,"heightPt":300},"margins":{"leftPt":36.00576428,"rightPt":36.00155196,"topPt":0,"bottomPt":0}}"#).is_err());
    }

    #[test]
    fn nested_schema_refuses_extra_fields_and_requires_custom_dimensions() {
        let schema = schema();
        assert_eq!(schema.get("additionalProperties"), Some(&JsonValue::Bool(false)));
        let properties = schema.get("properties").unwrap();
        let JsonValue::Array(sizes) = properties.get("size").unwrap().get("oneOf").unwrap() else { panic!("size choices") };
        assert_eq!(sizes[1].get("additionalProperties"), Some(&JsonValue::Bool(false)));
        assert_eq!(sizes[1].get("required"), Some(&JsonValue::Array(vec![
            JsonValue::String("widthPt".into()), JsonValue::String("heightPt".into()),
        ])));
    }
}
