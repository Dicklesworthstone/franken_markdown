//! Clickable, parser-owned Markdown destinations, including unopened files.
//!
//! Returning a URI is not a filesystem lookup or permission to fetch it. The
//! client decides whether to open a link after the user activates it. File
//! identities use the same decoder as definition/rename; no extension or index
//! file is guessed. This provider deliberately has no lazy resolve operation.

use super::{Json, Parser, Position, object, position, string, workspace_links};

const MAX_LINKS: usize = 4096;
const MAX_TARGET_BYTES: usize = 8192;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
type Failure = (i32, &'static str);

/// URI percent escapes are preserved, never decoded a second time. Refuse an
/// invalid spelling rather than handing different clients different targets.
fn valid_escapes(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if !bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                || !bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit)
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn push_encoded(output: &mut String, value: &str, reserved: bool) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || reserved && b":/?#[]@!$&'()*+,;=%".contains(&byte)
        {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
}

fn file_target(origin: &str, destination: &str) -> Option<String> {
    // Same-document fragments already have exact source go-to-definition.
    // Avoid suggesting that a generic client's file URI opener can locate a
    // Markdown heading from its rendered HTML ID.
    if destination.is_empty() || destination.starts_with(['#', '?']) {
        return None;
    }
    let (key, directory) = workspace_links::addressed(origin, destination)?;
    let mut output = format!("file://{}", key.authority);
    for (index, part) in key.parts.iter().enumerate() {
        output.push('/');
        let drive = index == 0
            && part.len() == 2
            && part.as_bytes()[0].is_ascii_alphabetic()
            && part.ends_with(':');
        if drive {
            output.push_str(part);
        } else {
            push_encoded(&mut output, part, false);
        }
    }
    if directory || key.parts.is_empty() {
        output.push('/');
    }
    if let Some(start) = destination.find(['?', '#']) {
        push_encoded(&mut output, &destination[start..], true);
    }
    Some(output)
}

fn target(origin: &str, destination: &str) -> Result<Option<String>, Failure> {
    if destination.len() > MAX_TARGET_BYTES {
        return Err((-32803, "document link exceeds the 8192-byte target budget"));
    }
    if destination.chars().any(char::is_control)
        || destination.contains('\\')
        || !valid_escapes(destination)
    {
        return Ok(None);
    }
    let scheme = destination
        .split(['/', '?', '#'])
        .next()
        .and_then(|prefix| prefix.split_once(':').map(|(scheme, _)| scheme));
    let output = match scheme {
        Some(scheme)
            if scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http") =>
        {
            let rest = &destination[scheme.len() + 1..];
            let Some(authority) = rest
                .strip_prefix("//")
                .and_then(|rest| rest.split('/').next())
            else {
                return Ok(None);
            };
            let authority = authority.split(['?', '#']).next().unwrap_or("");
            if authority.is_empty() || authority.chars().any(char::is_whitespace) {
                return Ok(None);
            }
            let mut output = String::new();
            push_encoded(&mut output, destination, true);
            Some(output)
        }
        Some(scheme) if scheme.eq_ignore_ascii_case("mailto") => {
            if destination.len() == scheme.len() + 1 {
                return Ok(None);
            }
            let mut output = String::new();
            push_encoded(&mut output, destination, true);
            Some(output)
        }
        Some(scheme) if !scheme.eq_ignore_ascii_case("file") => None,
        _ => file_target(origin, destination),
    };
    if output
        .as_ref()
        .is_some_and(|output| output.len() > MAX_TARGET_BYTES)
    {
        return Err((-32803, "document link exceeds the 8192-byte target budget"));
    }
    Ok(output)
}

/// Convert ordered byte ranges in one source pass. LineIndex::position is
/// appropriate for isolated lookups, but rescanning a long line for thousands
/// of destinations would multiply the document's work by its link count.
struct Coordinates {
    offset: usize,
    value: Position,
}

impl Coordinates {
    fn new() -> Self {
        Self {
            offset: 0,
            value: Position {
                line: 0,
                character: 0,
            },
        }
    }

    fn position(&mut self, source: &str, offset: usize) -> Result<Position, Failure> {
        if offset < self.offset || !source.is_char_boundary(offset) {
            return Err((
                -32803,
                "document links have unordered or invalid source ranges",
            ));
        }
        for (relative, ch) in source[self.offset..offset].char_indices() {
            match ch {
                // At the byte between CR and LF, an editor position still
                // addresses the preceding line's end, just like LineIndex.
                '\r' if source.as_bytes().get(self.offset + relative + 1) == Some(&b'\n') => {}
                '\r' | '\n' => {
                    self.value.line += 1;
                    self.value.character = 0;
                }
                _ => self.value.character += ch.len_utf16(),
            }
        }
        self.offset = offset;
        Ok(self.value)
    }
}

pub(super) fn request(parser: Parser, uri: &str, source: &str) -> Result<Json, Failure> {
    let mut coordinates = Coordinates::new();
    let mut links = Vec::new();
    let mut response_bytes: usize = 2; // The result array's brackets.
    // This inventory excludes code, HTML attributes, abandoned speculative
    // links and bare/angle autolinks. It also includes accepted unused or
    // shadowed reference definitions: their URL token is itself clickable.
    for destination in parser.destinations(source) {
        let Some(target) = target(uri, &destination.dest)? else {
            continue;
        };
        let Some(raw) = source.get(destination.range.clone()) else {
            return Err((-32803, "document link has no exact source range"));
        };
        let angle = raw.starts_with('<') && raw.ends_with('>');
        let start = destination.range.start + usize::from(angle);
        let end = destination.range.end - usize::from(angle);
        if start >= end {
            continue;
        }
        if links.len() == MAX_LINKS {
            return Err((-32803, "document links exceed the 4096-item result budget"));
        }
        let start = coordinates.position(source, start)?;
        let end = coordinates.position(source, end)?;
        let link = object([
            (
                "range",
                object([
                    ("start", position(start)),
                    ("end", position(end)),
                ]),
            ),
            ("target", string(&target)),
        ]);
        // A short relative URL can expand to the source URI\'s entire directory.
        // Bound the serialized result, not just its item count or source size.
        let added = link.to_json_string().len() + usize::from(!links.is_empty());
        if added > MAX_RESPONSE_BYTES.saturating_sub(response_bytes) {
            return Err((-32803, "document links exceed the 1 MiB response budget"));
        }
        response_bytes += added;
        links.push(link);
    }
    Ok(Json::Array(links))
}

#[cfg(test)]
#[path = "document_links_tests.rs"]
mod tests;
