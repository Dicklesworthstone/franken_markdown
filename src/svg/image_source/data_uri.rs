//! Decode percent-escaped data URIs without materializing a base64-sized buffer.

use super::{MAX_IMAGE_BYTES, SvgWarning, failure};

pub(crate) fn decode_data_uri(source: &str) -> Result<Vec<u8>, SvgWarning> {
    decode_with_limit(source, MAX_IMAGE_BYTES)
}

fn decode_with_limit(source: &str, limit: usize) -> Result<Vec<u8>, SvgWarning> {
    let invalid = || failure("svg_image_invalid", "malformed image data URI");
    if !source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        return Err(invalid());
    }
    // Every supported header is short. Do not scan an unbounded fake header.
    let comma = source.as_bytes().iter().take(256).position(|&b| b == b',');
    let comma = comma.ok_or_else(invalid)?;
    let mut fields = source[5..comma].split(';');
    let mime = fields.next().unwrap_or("");
    if !["image/png", "image/jpeg", "image/svg+xml"]
        .iter()
        .any(|m| mime.eq_ignore_ascii_case(m))
    {
        return Err(failure(
            "svg_image_unsupported",
            "data URI must name PNG, JPEG, or SVG",
        ));
    }
    let mut base64 = false;
    let mut charset = false;
    for field in fields {
        if field.eq_ignore_ascii_case("base64") && !base64 {
            base64 = true;
        } else if field.eq_ignore_ascii_case("charset=utf-8") && !base64 && !charset {
            charset = true;
        } else {
            return Err(failure(
                "svg_image_invalid",
                "unsupported image data URI parameter",
            ));
        }
    }
    let encoded = &source.as_bytes()[comma + 1..];
    let symbols = if base64 {
        limit.div_ceil(3).saturating_mul(4)
    } else {
        limit
    };
    // Each base64 symbol may itself be a three-byte percent escape. Padding
    // is included, so even one- and two-byte images work at their exact limit.
    if encoded.len() > symbols.saturating_mul(3) {
        return Err(limit_error());
    }
    let mut result = Vec::new();
    let mut offset = 0;
    let mut quartet = [0; 4];
    let mut used = 0;
    let mut padded = false;
    while offset < encoded.len() {
        let byte = if encoded[offset] == b'%' {
            let pair = encoded
                .get(offset + 1..offset + 3)
                .ok_or_else(|| failure("svg_image_invalid", "truncated percent escape"))?;
            offset += 3;
            hex(pair[0])? * 16 + hex(pair[1])?
        } else {
            let byte = encoded[offset];
            offset += 1;
            byte
        };
        if !base64 {
            append(&mut result, &[byte], limit)?;
            continue;
        }
        if padded {
            return Err(base64_error());
        }
        quartet[used] = byte;
        used += 1;
        if used == 4 {
            let (bytes, count) = decode_quartet(quartet)?;
            append(&mut result, &bytes[..count], limit)?;
            padded = count != 3;
            used = 0;
        }
    }
    if used != 0 {
        return Err(base64_error());
    }
    Ok(result)
}

fn limit_error() -> SvgWarning {
    failure(
        "svg_image_limit",
        "image exceeds the byte or allocation limit",
    )
}

fn base64_error() -> SvgWarning {
    failure("svg_image_invalid", "invalid base64 image payload")
}

/// Bound both the produced length and requested capacity before writing.
/// `Vec::push`'s implicit geometric growth could otherwise exceed the budget.
fn append(out: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<(), SvgWarning> {
    if bytes.len() > limit.saturating_sub(out.len()) {
        return Err(limit_error());
    }
    let needed = out.len() + bytes.len();
    if needed > out.capacity() {
        let capacity = out
            .capacity()
            .saturating_mul(2)
            .max(needed)
            .max(limit.min(4096))
            .min(limit);
        out.try_reserve_exact(capacity - out.len())
            .map_err(|_| limit_error())?;
    }
    out.extend_from_slice(bytes);
    Ok(())
}

fn hex(byte: u8) -> Result<u8, SvgWarning> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(failure("svg_image_invalid", "invalid percent escape")),
    }
}

fn sextet(byte: u8) -> Result<u8, SvgWarning> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(base64_error()),
    }
}

fn decode_quartet(input: [u8; 4]) -> Result<([u8; 3], usize), SvgWarning> {
    let a = sextet(input[0])?;
    let b = sextet(input[1])?;
    let mut out = [(a << 2) | (b >> 4), 0, 0];
    if input[2] == b'=' {
        if input[3] != b'=' || b & 15 != 0 {
            return Err(base64_error());
        }
        return Ok((out, 1));
    }
    let c = sextet(input[2])?;
    out[1] = (b << 4) | (c >> 2);
    if input[3] == b'=' {
        if c & 3 != 0 {
            return Err(base64_error());
        }
        return Ok((out, 2));
    }
    out[2] = (c << 6) | sextet(input[3])?;
    Ok((out, 3))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn exact_limits_include_percent_encoded_padding() {
        for (payload, expected) in [
            ("", b"".as_slice()),
            ("%41%51%3D%3D", b"\x01".as_slice()),
            ("%41%51%49%3D", b"\x01\x02".as_slice()),
            ("%41%51%49%44", b"\x01\x02\x03".as_slice()),
        ] {
            let source = format!("data:image/png;base64,{payload}");
            let bytes = decode_with_limit(&source, expected.len()).unwrap();
            assert_eq!(bytes, expected);
            assert!(bytes.capacity() <= expected.len());
            if !expected.is_empty() {
                assert_eq!(
                    decode_with_limit(&source, expected.len() - 1)
                        .unwrap_err()
                        .code,
                    "svg_image_limit"
                );
            }
        }
    }

    #[test]
    fn byte_limit_is_enforced_during_percent_and_base64_decoding() {
        for source in [
            "data:image/png,12345678",
            "data:image/png,%31%32%33%34%35%36%37%38",
            "data:image/png;base64,MTIzNDU2Nzg=",
        ] {
            assert_eq!(decode_with_limit(source, 8).unwrap(), b"12345678");
            assert_eq!(
                decode_with_limit(source, 7).unwrap_err().code,
                "svg_image_limit"
            );
        }
    }

    #[test]
    fn prefix_and_parameters_are_validated_without_byte_slice_panics() {
        for source in [
            "xxxxximage/png,abc",
            "http:image/png,abc",
            "data:image/png;charset=utf-8;charset=utf-8,abc",
            "data:image/png;base64;charset=utf-8,AA==",
            "data:image/png;base64;base64,AA==",
            "daé:image/png,abc",
            "d🦀:image/png,abc",
            "data:image/png,%%",
        ] {
            assert!(decode_data_uri(source).is_err(), "{source}");
        }
        assert_eq!(
            decode_data_uri("DATA:IMAGE/PNG;CHARSET=UTF-8;BASE64,AQ==").unwrap(),
            [1]
        );
    }

    #[test]
    fn strict_padding_and_alphabet_survive_streaming() {
        for payload in [
            "A",
            "AAAAA",
            "=AAA",
            "A===",
            "AB==",
            "AAB=",
            "AA==AAAA",
            "AA$=",
            "AA%",
            "AA==%41%41%41%41",
            "AQ==\n",
            "AQ%3D%3D=",
            "AQ-_",
            "AQ==%00",
        ] {
            let source = format!("data:image/png;base64,{payload}");
            assert_eq!(
                decode_data_uri(&source).unwrap_err().code,
                "svg_image_invalid"
            );
        }
    }

    #[test]
    fn growth_never_requests_more_than_the_remaining_budget() {
        let mut bytes = Vec::new();
        for _ in 0..8193 {
            append(&mut bytes, &[7], 8193).unwrap();
            assert!(bytes.capacity() <= 8193);
        }
        assert_eq!(
            append(&mut bytes, &[7], 8193).unwrap_err().code,
            "svg_image_limit"
        );
        assert_eq!(bytes.len(), 8193);
    }
}
