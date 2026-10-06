//! Resolve Markdown URLs only against the client's explicitly opened buffers.
//! File URIs are logical identities here, never permission to touch a file.

use std::collections::BTreeMap;

use super::Buffer;

type Failure = (i32, &'static str);
type Target<'a> = (&'a str, &'a str);

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileKey {
    authority: String,
    parts: Vec<String>,
}

fn decode_component(raw: &str) -> Option<String> {
    let hex = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            decoded.push(hex(*bytes.get(index + 1)?)? * 16 + hex(*bytes.get(index + 2)?)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(decoded).ok()?;
    // Encoded separators are not aliases for directory boundaries. Decode
    // once, so a literal filename containing "%2F" remains distinguishable.
    (!decoded.contains(['/', '\\']) && !decoded.chars().any(char::is_control))
        .then_some(decoded)
}

fn drive(part: &str) -> bool {
    part.len() == 2 && part.as_bytes()[0].is_ascii_alphabetic() && part.as_bytes()[1] == b':'
}

fn append(parts: &mut Vec<String>, path: &str) -> Option<bool> {
    let mut directory = false;
    for raw in path.split('/') {
        let part = decode_component(raw)?;
        directory = matches!(part.as_str(), "" | "." | "..");
        match part.as_str() {
            "" | "." => {}
            ".." => {
                // Neither a URI root nor a Windows drive root can be escaped.
                if parts.len() == 1 && drive(&parts[0]) {
                    return None;
                }
                parts.pop()?;
            }
            _ => {
                let part = if parts.is_empty() && drive(&part) {
                    part.to_ascii_uppercase()
                } else {
                    part
                };
                parts.push(part);
            }
        }
    }
    Some(directory)
}

fn file_uri(uri: &str) -> Option<(FileKey, bool)> {
    if uri.len() > 8192
        || !uri.get(..5)?.eq_ignore_ascii_case("file:")
        || uri.contains(['?', '#', '\\'])
    {
        return None;
    }
    let tail = &uri[5..];
    let (authority, path) = if let Some(tail) = tail.strip_prefix("//") {
        let slash = tail.find('/')?;
        (&tail[..slash], &tail[slash..])
    } else {
        ("", tail)
    };
    if !path.starts_with('/')
        || path.starts_with("//")
        || !authority
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return None;
    }
    let authority = if authority.eq_ignore_ascii_case("localhost") {
        String::new()
    } else {
        authority.to_ascii_lowercase()
    };
    let mut key = FileKey { authority, parts: Vec::new() };
    let directory = append(&mut key.parts, path)?;
    Some((key, directory))
}

fn addressed(origin: &str, destination: &str) -> Option<(FileKey, bool)> {
    if destination.is_empty() || destination.len() > 8192 {
        return None;
    }
    let path = destination.split(['?', '#']).next()?;
    if path.get(..5).is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:")) {
        return file_uri(path);
    }
    // Other schemes and protocol-relative network URLs never acquire access
    // merely because the client also happened to open a similarly named URI.
    if path.is_empty() || path.starts_with("//") || path.split('/').next()?.contains(':') {
        return None;
    }
    let (mut key, directory) = file_uri(origin)?;
    if directory || key.parts.is_empty() {
        return None;
    }
    if path.starts_with('/') {
        key.parts.clear();
    } else {
        key.parts.pop();
    }
    let directory = append(&mut key.parts, path)?;
    Some((key, directory))
}

fn select<'a>(mut matches: impl Iterator<Item = (&'a str, &'a Buffer)>)
    -> Result<Option<Target<'a>>, Failure>
{
    let Some((uri, buffer)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        // Two buffer URIs may normalize to the same identity. Never choose a
        // revision by map order, even if their text currently happens to agree.
        return Ok(None);
    }
    if !buffer.synchronized {
        return Err((-32801, "target document requires full-text resynchronization"));
    }
    Ok(Some((uri, &buffer.text)))
}

pub(super) fn resolve<'a>(
    documents: &'a BTreeMap<String, Buffer>,
    origin: &str,
    destination: &str,
) -> Result<Option<Target<'a>>, Failure> {
    let Some((key, directory)) = addressed(origin, destination) else {
        return Ok(None);
    };
    let opened: Vec<_> = documents
        .iter()
        .filter_map(|(uri, buffer)| {
            let (key, directory) = file_uri(uri)?;
            (!directory && !key.parts.is_empty()).then_some((key, uri.as_str(), buffer))
        })
        .collect();
    if !directory && opened.iter().any(|(candidate, _, _)| candidate == &key) {
        return select(opened.iter().filter_map(|(candidate, uri, buffer)| {
            (candidate == &key).then_some((*uri, *buffer))
        }));
    }
    if !directory && key.parts.last().is_none_or(|name| name.contains('.')) {
        return Ok(None);
    }
    let mut candidates = Vec::with_capacity(6);
    if !directory {
        for extension in [".md", ".markdown"] {
            let mut candidate = key.clone();
            if let Some(name) = candidate.parts.last_mut() {
                name.push_str(extension);
            }
            candidates.push(candidate);
        }
    }
    for name in ["index.md", "index.markdown", "README.md", "README.markdown"] {
        let mut candidate = key.clone();
        candidate.parts.push(name.to_string());
        candidates.push(candidate);
    }
    select(opened.iter().filter_map(|(key, uri, buffer)| {
        candidates.contains(key).then_some((*uri, *buffer))
    }))
}

#[cfg(test)]
#[path = "workspace_links_tests.rs"]
mod tests;
