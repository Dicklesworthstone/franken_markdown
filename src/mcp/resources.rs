//! Explicit MCP resource bytes. Markdown destinations never trigger filesystem
//! reads or network requests. Admission is checked before base64 allocation.

use std::collections::BTreeMap;
use crate::{FontAssetSlot, FontAssets, PdfImageAsset};
use super::{JsonValue, ToolError, ERROR_INPUT_TOO_LARGE, ERROR_INVALID_OPTIONS};

const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
const MAX_IMAGES: usize = 4096;
const MAX_KEY_BYTES: usize = 4096;

#[derive(Default)]
pub(crate) struct Resources {
    pub(crate) fonts: FontAssets,
    pub(crate) images: Vec<PdfImageAsset>,
}

fn invalid(message: &str) -> ToolError {
    (ERROR_INVALID_OPTIONS, message.to_owned(), "invalid_resources")
}

fn limit() -> ToolError {
    (ERROR_INPUT_TOO_LARGE, "Resource limits: 32 MiB per asset, 64 MiB combined encoded/decoded bytes, 4096 images and five font slots".to_owned(), "resources_too_large")
}

fn array<'a>(args: &'a JsonValue, key: &str, maximum: usize) -> Result<&'a [JsonValue], ToolError> {
    match args.get(key) {
        None => Ok(&[]),
        Some(JsonValue::Array(items)) if items.len() <= maximum => Ok(items),
        Some(JsonValue::Array(_)) => Err(limit()),
        _ => Err(invalid("images and fonts must be arrays")),
    }
}

fn object<'a>(value: &'a JsonValue, allowed: &[&str]) -> Result<&'a BTreeMap<String, JsonValue>, ToolError> {
    let object = value.as_object().ok_or_else(|| invalid("Each resource must be an object"))?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid("Unknown resource field"));
    }
    Ok(object)
}

fn string<'a>(value: &'a BTreeMap<String, JsonValue>, key: &str) -> Result<&'a str, ToolError> {
    value.get(key).and_then(JsonValue::as_str).ok_or_else(|| invalid("Resource identifiers and base64 payloads must be strings"))
}

struct Encoded<'a> {
    key: &'a str,
    bytes: &'a str,
    slot: Option<FontAssetSlot>,
    weight: Option<u16>,
}

pub(crate) fn parse(args: &JsonValue) -> Result<Resources, ToolError> {
    let images = array(args, "images", MAX_IMAGES)?;
    let fonts = array(args, "fonts", FontAssetSlot::ALL.len())?;
    let mut planned = Vec::with_capacity(images.len() + fonts.len());
    let mut encoded_total = 0usize;
    let mut decoded_total = 0usize;
    let mut seen_slots = Vec::new();
    for (items, font) in [(images, false), (fonts, true)] {
        for item in items {
            let item = object(item, if font { &["slot", "base64", "weight"] } else { &["destination", "base64"] })?;
            let key = string(item, if font { "slot" } else { "destination" })?.trim();
            if key.is_empty() || key.len() > MAX_KEY_BYTES || key.chars().any(char::is_control) {
                return Err(invalid("Resource keys must contain 1..=4096 bytes without control characters"));
            }
            let slot = if font {
                let slot = FontAssetSlot::ALL.into_iter().find(|slot| slot.as_str() == key)
                    .ok_or_else(|| invalid("Unknown font slot; use a canonical body or mono slot"))?;
                if seen_slots.contains(&slot) { return Err(invalid("Duplicate font slot")); }
                seen_slots.push(slot);
                Some(slot)
            } else { None };
            let weight = item.get("weight").map(|value| {
                value.as_u64().filter(|weight| (1..=1000).contains(weight))
                    .and_then(|weight| u16::try_from(weight).ok())
                    .ok_or_else(|| invalid("Font weight must be an integer in 1..=1000"))
            }).transpose()?;
            let bytes = string(item, "base64")?;
            let decoded = decoded_length(bytes)?;
            encoded_total = encoded_total.checked_add(bytes.len()).and_then(|n| n.checked_add(key.len()))
                .filter(|n| *n <= MAX_TOTAL_BYTES).ok_or_else(limit)?;
            decoded_total = decoded_total.checked_add(decoded).filter(|n| *n <= MAX_TOTAL_BYTES).ok_or_else(limit)?;
            planned.push(Encoded { key, bytes, slot, weight });
        }
    }
    let mut result = Resources::default();
    for item in planned {
        let bytes = decode(item.bytes)?;
        if let Some(slot) = item.slot {
            result.fonts.set_slot(slot, bytes).map_err(|error| invalid(&format!("Invalid {} font: {error}", slot.as_str())))?;
            if let Some(weight) = item.weight {
                result.fonts.set_slot_weight(slot, weight)
                    .map_err(|_| invalid("Invalid font weight"))?;
            }
        } else {
            result.images.push(PdfImageAsset::new(item.key, bytes));
        }
    }
    Ok(result)
}

fn decoded_length(source: &str) -> Result<usize, ToolError> {
    let input = source.as_bytes();
    if input.len() > MAX_ASSET_BYTES.div_ceil(3) * 4 { return Err(limit()); }
    if input.is_empty() || input.len() % 4 != 0 { return Err(invalid("Resource base64 must be nonempty canonical padded base64")); }
    let padding = usize::from(input.ends_with(b"=")) + usize::from(input.ends_with(b"=="));
    let length = input.len() / 4 * 3 - padding;
    if length > MAX_ASSET_BYTES { return Err(limit()); }
    Ok(length)
}

fn decode(source: &str) -> Result<Vec<u8>, ToolError> {
    let length = decoded_length(source)?;
    let mut result = Vec::with_capacity(length);
    let input = source.as_bytes();
    let bad = || invalid("Invalid resource base64 alphabet or padding");
    let value = |byte| match byte {
        b'A'..=b'Z' => Some(byte - b'A'), b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52), b'+' => Some(62), b'/' => Some(63), _ => None,
    };
    for (index, chunk) in input.chunks_exact(4).enumerate() {
        let last = index + 1 == input.len() / 4;
        let a = value(chunk[0]).ok_or_else(bad)?;
        let b = value(chunk[1]).ok_or_else(bad)?;
        result.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 15 != 0 { return Err(bad()); }
        } else {
            let c = value(chunk[2]).ok_or_else(bad)?;
            result.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 3 != 0 { return Err(bad()); }
            } else {
                let d = value(chunk[3]).ok_or_else(bad)?;
                result.push((c << 6) | d);
            }
        }
    }
    Ok(result)
}

/// Nested schemas are emitted alongside the executable resource admission rules.
pub(super) fn schema(name: &str) -> JsonValue {
    let font = name == "fonts";
    let text = |description: &str| JsonValue::Object(BTreeMap::from([
        ("type".to_owned(), JsonValue::String("string".to_owned())),
        ("description".to_owned(), JsonValue::String(description.to_owned())),
    ]));
    let key = if font { "slot" } else { "destination" };
    let mut identifier = text(if font { "Canonical font slot" } else { "Markdown destination identifier; not read or fetched" });
    if let JsonValue::Object(fields) = &mut identifier {
        fields.insert("minLength".to_owned(), JsonValue::Number(1.0));
        fields.insert("maxLength".to_owned(), JsonValue::Number(MAX_KEY_BYTES as f64));
        if font {
            fields.insert("enum".to_owned(), JsonValue::Array(FontAssetSlot::ALL.into_iter()
                .map(|slot| JsonValue::String(slot.as_str().to_owned())).collect()));
        }
    }
    let mut payload = text("Canonical padded base64 bytes; 32 MiB decoded maximum");
    if let JsonValue::Object(fields) = &mut payload {
        fields.insert("minLength".to_owned(), JsonValue::Number(4.0));
        fields.insert("maxLength".to_owned(), JsonValue::Number((MAX_ASSET_BYTES.div_ceil(3) * 4) as f64));
    }
    let mut fields = BTreeMap::from([(key.to_owned(), identifier), ("base64".to_owned(), payload)]);
    if font {
        fields.insert("weight".to_owned(), JsonValue::Object(BTreeMap::from([
            ("type".to_owned(), JsonValue::String("integer".to_owned())),
            ("minimum".to_owned(), JsonValue::Number(1.0)),
            ("maximum".to_owned(), JsonValue::Number(1000.0)),
        ])));
    }
    JsonValue::Object(BTreeMap::from([
        ("type".to_owned(), JsonValue::String("array".to_owned())),
        ("maxItems".to_owned(), JsonValue::Number(if font { 5.0 } else { MAX_IMAGES as f64 })),
        ("items".to_owned(), JsonValue::Object(BTreeMap::from([
            ("type".to_owned(), JsonValue::String("object".to_owned())),
            ("additionalProperties".to_owned(), JsonValue::Bool(false)),
            ("required".to_owned(), JsonValue::Array(vec![JsonValue::String(key.to_owned()), JsonValue::String("base64".to_owned())])),
            ("properties".to_owned(), JsonValue::Object(fields)),
        ]))),
    ]))
}

pub(crate) fn with_svg_warnings(mut response: JsonValue, warnings: Vec<crate::svg::SvgWarning>) -> JsonValue {
    if !warnings.is_empty() && let JsonValue::Object(result) = &mut response {
        result.insert("warnings".to_owned(), JsonValue::Array(warnings.into_iter().map(|warning| {
            JsonValue::Object(BTreeMap::from([
                ("code".to_owned(), JsonValue::String(warning.code.to_owned())),
                ("message".to_owned(), JsonValue::String(warning.message)),
            ]))
        }).collect()));
    }
    response
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::mcp::{base64_encode, parse_json};

    #[test]
    fn canonical_base64_roundtrips_all_lengths_and_bytes() {
        for length in 1..1025 {
            let bytes: Vec<_> = (0..length).map(|n| ((n * 71 + length) % 256) as u8).collect();
            assert_eq!(decode(&base64_encode(&bytes)).unwrap(), bytes);
        }
    }

    #[test]
    fn malformed_alphabet_padding_and_empty_payloads_are_rejected() {
        for input in ["", "A", "AAAAA", "AA==AAAA", "AB==", "AAB=", "AA$=", "AAAA\n", "A===", "===="] {
            assert!(decode(input).is_err(), "accepted {input:?}");
        }
    }

    #[test]
    fn nested_resource_contract_rejects_wrong_types_unknown_fields_and_duplicates() {
        for args in [
            r#"{"images":null}"#, r#"{"images":[4]}"#, r#"{"images":[{"destination":"x","base64":true}]}"#,
            r#"{"images":[{"destination":"x","base64":"AA==","path":"/private"}]}"#,
            r#"{"images":[{"destination":"","base64":"AA=="}]}"#,
            r#"{"fonts":[{"slot":"unknown","base64":"AA=="}]}"#,
            r#"{"fonts":[{"slot":"body-regular","base64":"AA==","weight":1.5}]}"#,
            r#"{"fonts":[{"slot":"body-regular","base64":"AA=="},{"slot":"body-regular","base64":"AA=="}]}"#,
        ] {
            assert!(parse(&parse_json(args).unwrap()).is_err(), "accepted {args}");
        }
    }

    #[test]
    fn resource_preflight_enforces_count_key_and_decoded_size_without_rendering() {
        let image = parse_json(r#"{"destination":"x","base64":"AA=="}"#).unwrap();
        let args = JsonValue::Object(BTreeMap::from([("images".to_owned(), JsonValue::Array(vec![image; MAX_IMAGES + 1]))]));
        assert_eq!(parse(&args).err().unwrap().2, "resources_too_large");
        let too_large = "A".repeat(MAX_ASSET_BYTES.div_ceil(3) * 4 + 4);
        assert_eq!(decoded_length(&too_large).err().unwrap().2, "resources_too_large");
    }

    #[test]
    fn image_order_and_binary_bytes_are_preserved() {
        let args = parse_json(r#"{"images":[{"destination":" x ","base64":"AP8="},{"destination":"x","base64":"AQ=="}]}"#).unwrap();
        let resources = parse(&args).unwrap();
        assert_eq!(resources.images[0], PdfImageAsset::new("x", vec![0, 255]));
        assert_eq!(resources.images[1], PdfImageAsset::new("x", vec![1]));
    }

    #[test]
    fn font_bytes_and_weight_pins_reach_the_shared_container() {
        let bytes = crate::text::bundled::CM_REGULAR;
        let args = parse_json(&format!(r#"{{"fonts":[{{"slot":"body-regular","base64":"{}","weight":450}}]}}"#, base64_encode(bytes))).unwrap();
        let resources = parse(&args).unwrap();
        assert_eq!(resources.fonts.slot_bytes(FontAssetSlot::BodyRegular).unwrap(), bytes);
        assert_eq!(resources.fonts.slot_weight(FontAssetSlot::BodyRegular), Some(450));
    }
}
