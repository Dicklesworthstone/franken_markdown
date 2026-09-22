//! Bounded image containers and intrinsic sizes. Pixel decoding stays with the
//! SVG viewer; no external source is ever fetched by this module.

use super::SvgWarning;
use std::collections::BTreeMap;

pub(super) const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SIDE: f64 = 16_384.0;
const MAX_PIXELS: f64 = 100_000_000.0;

pub(super) fn failure(code: &'static str, message: &str) -> SvgWarning {
    SvgWarning { code, message: message.to_owned() }
}

pub(super) fn inspect(bytes: &[u8]) -> Result<(&'static str, f64, f64), SvgWarning> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(failure("svg_image_limit", "image exceeds the 32 MiB byte limit"));
    }
    let (mime, width, height) = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let (w, h) = png_size(bytes).ok_or_else(|| failure("svg_image_invalid", "invalid PNG container"))?;
        ("image/png", f64::from(w), f64::from(h))
    } else if bytes.starts_with(&[0xff, 0xd8]) {
        let (w, h) = jpeg_size(bytes).ok_or_else(|| failure("svg_image_invalid", "invalid JPEG container"))?;
        ("image/jpeg", f64::from(w), f64::from(h))
    } else {
        let source = std::str::from_utf8(bytes).map_err(|_| failure("svg_image_unsupported", "image is not PNG, JPEG, or UTF-8 SVG"))?;
        let (w, h) = svg_size(source).ok_or_else(|| failure("svg_image_invalid", "invalid SVG XML or intrinsic size (DTDs are not supported)"))?;
        ("image/svg+xml", w, h)
    };
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0
        || width > MAX_SIDE || height > MAX_SIDE || width * height > MAX_PIXELS
    {
        return Err(failure("svg_image_dimensions", "image dimensions exceed 16384 per side or 100 million pixels"));
    }
    // Image intrinsic dimensions use CSS pixels; poster coordinates use points.
    Ok((mime, width * 0.75, height * 0.75))
}

fn be32(bytes: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}

fn be16(bytes: &[u8]) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(..2)?.try_into().ok()?))
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut offset = 8usize;
    let mut size = None;
    let mut data = false;
    let mut ended_data = false;
    let mut palette = false;
    let mut indexed = false;
    while offset < bytes.len() {
        let len = usize::try_from(be32(bytes.get(offset..)?)?).ok()?;
        let body = offset.checked_add(8)?;
        let end = body.checked_add(len)?;
        let next = end.checked_add(4)?;
        let tag = bytes.get(offset + 4..body)?;
        let payload = bytes.get(body..end)?;
        let checksum = be32(bytes.get(end..next)?)?;
        if franken_markdown::crc32(bytes.get(offset + 4..end)?) != checksum { return None; }
        if size.is_none() && tag != b"IHDR" { return None; }
        match tag {
            b"IHDR" => {
                if size.is_some() || len != 13 { return None; }
                let depth = payload[8];
                let color = payload[9];
                let valid_depth = match color {
                    0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
                    2 | 4 | 6 => matches!(depth, 8 | 16),
                    3 => matches!(depth, 1 | 2 | 4 | 8),
                    _ => false,
                };
                if !valid_depth || payload[10] != 0 || payload[11] != 0 || payload[12] > 1 { return None; }
                indexed = color == 3;
                size = Some((be32(payload)?, be32(&payload[4..])?));
            }
            b"PLTE" => {
                if palette || data || len == 0 || len > 768 || len % 3 != 0 { return None; }
                palette = true;
            }
            b"IDAT" => {
                if ended_data || (indexed && !palette) { return None; }
                data = true;
            }
            b"IEND" => return (len == 0 && data && next == bytes.len()).then_some(size).flatten(),
            _ => {
                // Unknown critical chunks cannot be interpreted faithfully.
                if !tag.iter().all(u8::is_ascii_alphabetic) || tag[0].is_ascii_uppercase() { return None; }
                ended_data |= data;
            }
        }
        offset = next;
    }
    None
}

fn jpeg_size(bytes: &[u8]) -> Option<(u16, u16)> {
    if !bytes.ends_with(&[0xff, 0xd9]) { return None; }
    let mut offset = 2usize;
    let mut size = None;
    while offset < bytes.len() {
        if *bytes.get(offset)? != 0xff { return None; }
        while *bytes.get(offset)? == 0xff { offset += 1; }
        let marker = *bytes.get(offset)?;
        offset += 1;
        if matches!(marker, 0x00 | 0xd8 | 0xd9 | 0xd0..=0xd7) { return None; }
        if marker == 0x01 { continue; }
        let len = usize::from(be16(bytes.get(offset..)?)?);
        if len < 2 { return None; }
        let end = offset.checked_add(len)?;
        let segment = bytes.get(offset + 2..end)?;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if segment.len() < 6 || size.is_some() { return None; }
            let components = usize::from(segment[5]);
            if components == 0 || segment.len() != 6 + 3 * components { return None; }
            size = Some((be16(&segment[3..])?, be16(&segment[1..])?));
        }
        // SOS terminates the metadata scan. The viewer decodes entropy data.
        if marker == 0xda {
            if segment.len() < 4 || segment.len() != 4 + 2 * usize::from(segment[0]) { return None; }
            return size;
        }
        offset = end;
    }
    None
}

pub(super) fn decode_data_uri(source: &str) -> Result<Vec<u8>, SvgWarning> {
    let (header, encoded) = source.split_once(',').ok_or_else(|| failure("svg_image_invalid", "malformed image data URI"))?;
    let mut fields = header.get(5..).ok_or_else(|| failure("svg_image_invalid", "malformed image data URI"))?.split(';');
    let mime = fields.next().unwrap_or("");
    if !["image/png", "image/jpeg", "image/svg+xml"].iter().any(|m| mime.eq_ignore_ascii_case(m)) {
        return Err(failure("svg_image_unsupported", "data URI must name PNG, JPEG, or SVG"));
    }
    let mut base64 = false;
    for field in fields {
        if field.eq_ignore_ascii_case("base64") && !base64 { base64 = true; }
        else if field.eq_ignore_ascii_case("charset=utf-8") && !base64 { /* SVG UTF-8 */ }
        else { return Err(failure("svg_image_invalid", "unsupported image data URI parameter")); }
    }
    if encoded.len() > MAX_IMAGE_BYTES.saturating_mul(3) {
        return Err(failure("svg_image_limit", "encoded image exceeds the source byte limit"));
    }
    let mut decoded = Vec::with_capacity(encoded.len().min(MAX_IMAGE_BYTES));
    let input = encoded.as_bytes();
    let mut i = 0;
    while i < input.len() {
        let byte = if input[i] == b'%' {
            let high = hex(*input.get(i + 1).ok_or_else(|| failure("svg_image_invalid", "truncated percent escape"))?)?;
            let low = hex(*input.get(i + 2).ok_or_else(|| failure("svg_image_invalid", "truncated percent escape"))?)?;
            i += 3;
            high * 16 + low
        } else { let b = input[i]; i += 1; b };
        decoded.push(byte);
        let limit = if base64 { MAX_IMAGE_BYTES.div_ceil(3) * 4 } else { MAX_IMAGE_BYTES };
        if decoded.len() > limit { return Err(failure("svg_image_limit", "decoded image exceeds the byte limit")); }
    }
    if base64 { decode_base64(&decoded) } else { Ok(decoded) }
}

fn hex(byte: u8) -> Result<u8, SvgWarning> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'), b'a'..=b'f' => Ok(byte - b'a' + 10), b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(failure("svg_image_invalid", "invalid percent escape")),
    }
}

fn decode_base64(input: &[u8]) -> Result<Vec<u8>, SvgWarning> {
    let invalid = || failure("svg_image_invalid", "invalid base64 image payload");
    if input.len() % 4 != 0 { return Err(invalid()); }
    let mut result = Vec::with_capacity(input.len() / 4 * 3);
    for (index, chunk) in input.chunks_exact(4).enumerate() {
        let last = index + 1 == input.len() / 4;
        let value = |b| match b {
            b'A'..=b'Z' => Some(b - b'A'), b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52), b'+' => Some(62), b'/' => Some(63), _ => None,
        };
        let a = value(chunk[0]).ok_or_else(invalid)?;
        let b = value(chunk[1]).ok_or_else(invalid)?;
        result.push((a << 2) | (b >> 4));
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' || b & 15 != 0 { return Err(invalid()); }
        } else {
            let c = value(chunk[2]).ok_or_else(invalid)?;
            result.push((b << 4) | (c >> 2));
            if chunk[3] == b'=' {
                if !last || c & 3 != 0 { return Err(invalid()); }
            } else {
                let d = value(chunk[3]).ok_or_else(invalid)?;
                result.push((c << 6) | d);
            }
        }
    }
    if result.len() > MAX_IMAGE_BYTES { return Err(failure("svg_image_limit", "image exceeds the byte limit")); }
    Ok(result)
}

fn xml_char(ch: char) -> bool {
    matches!(ch, '\t' | '\n' | '\r') || ('\u{20}'..='\u{d7ff}').contains(&ch)
        || ('\u{e000}'..='\u{fffd}').contains(&ch) || ch >= '\u{10000}'
}

fn entities(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let tail = &rest[index + 1..];
        let end = tail.find(';')?;
        let entity = &tail[..end];
        let ch = match entity {
            "amp" => '&', "lt" => '<', "gt" => '>', "quot" => '"', "apos" => '\'',
            _ => {
                let scalar = if let Some(hex) = entity.strip_prefix("#x") {
                    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
                    u32::from_str_radix(hex, 16).ok()?
                } else {
                    let digits = entity.strip_prefix('#')?;
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) { return None; }
                    digits.parse().ok()?
                };
                char::from_u32(scalar)?
            }
        };
        if !xml_char(ch) { return None; }
        out.push(ch);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

// XML validation is deliberately DTD-free and bounded. SVG is embedded as an
// image subresource, never interpolated as markup in the outer document.
fn svg_root(source: &str) -> Option<BTreeMap<String, String>> {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    if !source.chars().all(xml_char) { return None; }
    let mut rest = source;
    let mut stack = Vec::new();
    let mut root = None;
    let mut elements = 0usize;
    while !rest.is_empty() {
        let open = rest.find('<').unwrap_or(rest.len());
        let text = &rest[..open];
        if stack.is_empty() && !text.trim().is_empty() { return None; }
        if text.contains("]]>") { return None; }
        entities(text)?;
        rest = &rest[open..];
        if rest.is_empty() { break; }
        if let Some(comment) = rest.strip_prefix("<!--") {
            let end = comment.find("-->")?;
            if comment[..end].contains("--") { return None; }
            rest = &comment[end + 3..];
            continue;
        }
        if let Some(cdata) = rest.strip_prefix("<![CDATA[") {
            if stack.is_empty() { return None; }
            rest = &cdata[cdata.find("]]>")? + 3..];
            continue;
        }
        if rest.starts_with("<?xml ") && root.is_none() && stack.is_empty() {
            rest = &rest[rest.find("?>")? + 2..];
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") { return None; }
        let mut quote = None;
        let mut end = None;
        for (i, ch) in rest.char_indices().skip(1) {
            match (quote, ch) {
                (Some(q), c) if q == c => quote = None,
                (None, '\'' | '"') => quote = Some(ch),
                (None, '>') => { end = Some(i); break; }
                (_, '<') => return None,
                _ => {}
            }
        }
        let end = end?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(close) = tag.strip_prefix('/') {
            if stack.pop()? != close.trim() { return None; }
            continue;
        }
        if root.is_some() && stack.is_empty() { return None; }
        let empty = tag.ends_with('/');
        let tag = tag.strip_suffix('/').unwrap_or(tag);
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        let name = &tag[..name_end];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.'))
            || !name.as_bytes()[0].is_ascii_alphabetic() { return None; }
        let mut attrs = BTreeMap::new();
        let mut tail = &tag[name_end..];
        while !tail.trim().is_empty() {
            if !tail.starts_with(char::is_whitespace) { return None; }
            tail = tail.trim_start();
            let eq = tail.find('=')?;
            let key = tail[..eq].trim_end();
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-' | b'.')) { return None; }
            tail = tail[eq + 1..].trim_start();
            let quote = tail.chars().next()?;
            if !matches!(quote, '\'' | '"') { return None; }
            tail = &tail[1..];
            let end = tail.find(quote)?;
            let value = entities(&tail[..end])?;
            if attrs.insert(key.to_owned(), value).is_some() { return None; }
            tail = &tail[end + 1..];
        }
        if root.is_none() {
            if name != "svg" { return None; }
            if attrs.get("xmlns").is_some_and(|ns| ns != "http://www.w3.org/2000/svg") { return None; }
            root = Some(attrs);
        }
        elements += 1;
        if elements > 65_536 || stack.len() >= 128 { return None; }
        if !empty { stack.push(name); }
    }
    if stack.is_empty() { root } else { None }
}

fn svg_size(source: &str) -> Option<(f64, f64)> {
    let attrs = svg_root(source)?;
    let width = match attrs.get("width") { Some(value) => length(value)?, None => None };
    let height = match attrs.get("height") { Some(value) => length(value)?, None => None };
    let view = if let Some(view) = attrs.get("viewBox") {
        let values: Vec<_> = view.split(|c: char| c.is_ascii_whitespace() || c == ',')
            .filter(|s| !s.is_empty()).map(str::parse::<f64>).collect::<Result<_, _>>().ok()?;
        if values.len() != 4 || !values.iter().all(|v| v.is_finite()) || values[2] <= 0.0 || values[3] <= 0.0 { return None; }
        Some((values[2], values[3]))
    } else { None };
    match (width, height, view) {
        (Some(w), Some(h), _) => Some((w, h)),
        (Some(w), None, Some((vw, vh))) => Some((w, w * vh / vw)),
        (None, Some(h), Some((vw, vh))) => Some((h * vw / vh, h)),
        (None, None, Some(dimensions)) => Some(dimensions),
        (w, h, None) => Some((w.unwrap_or(300.0), h.unwrap_or(150.0))),
    }
}

// Distinguish absent/relative dimensions from syntactically invalid dimensions.
fn length(value: &str) -> Option<Option<f64>> {
    let value = value.trim();
    if value == "auto" { return Some(None); }
    let mut number = value;
    let mut factor = Some(1.0);
    for (unit, scale) in [
        ("rem", None), ("em", None), ("ex", None), ("%", None),
        ("px", Some(1.0)), ("pt", Some(96.0 / 72.0)), ("pc", Some(16.0)),
        ("in", Some(96.0)), ("cm", Some(96.0 / 2.54)), ("mm", Some(96.0 / 25.4)),
    ] {
        if let Some(prefix) = value.strip_suffix(unit) { number = prefix; factor = scale; break; }
    }
    let number: f64 = number.parse().ok()?;
    if !number.is_finite() || number <= 0.0 { return None; }
    Some(factor.map(|factor| number * factor))
}
