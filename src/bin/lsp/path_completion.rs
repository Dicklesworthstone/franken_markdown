//! Complete destinations from explicitly opened, synchronized file buffers.
//! Directories are inferred only from those identities; no directory is read.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Buffer, Json, LineIndex, MAX_COMPLETIONS, MAX_DESTINATION_BYTES, SourceSpan, Token,
    completion_list, number, object, prefix_text, range, string, workspace_links,
};
use workspace_links::FileKey;

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

pub(super) fn complete(
    uri: &str,
    source: &str,
    index: &LineIndex,
    token: Token,
    offset: usize,
    documents: &BTreeMap<String, Buffer>,
) -> Json {
    let raw = &source[token.start..token.end];
    let path_end = token.start + raw.find(['?', '#']).unwrap_or(raw.len());
    // A Markdown character reference can change both the file identity and
    // where its query/fragment begins. The lexical token has no decoded-byte
    // source map; never authorize an edit against a different opened file.
    // Generated destinations use percent encoding, including `%26` for '&'.
    if source[token.start..path_end].contains('&') {
        return completion_list(Vec::new(), false);
    }
    // Queries and fragments keep their own meaning, even when no matching
    // target is open. A file completion edits only the path before them.
    if offset > path_end || source[offset..path_end].contains('/') {
        return completion_list(Vec::new(), false);
    }
    let typed = &source[token.start..offset];
    let directory_end = typed.rfind('/').map_or(0, |slash| slash + 1);
    let directory = &typed[..directory_end];
    let raw_prefix = &typed[directory_end..];
    let Some(prefix) = prefix_text(raw_prefix) else {
        // A partially typed percent escape may become valid on the next key.
        return completion_list(Vec::new(), true);
    };
    if prefix.contains(['/', '\\']) || prefix.chars().any(char::is_control) {
        return completion_list(Vec::new(), false);
    }
    let Some((mut parent, false)) =
        workspace_links::addressed(uri, &format!("{directory}__fmd_path_completion__"))
    else {
        return completion_list(Vec::new(), false);
    };
    parent.parts.pop();

    // Preserve ambiguous identities even if one alias is unsynchronized: a
    // completion must never silently pick the other alias's revision.
    let mut opened: BTreeMap<FileKey, Vec<(&str, &Buffer)>> = BTreeMap::new();
    for (opened_uri, buffer) in documents {
        if let Some((key, false)) = workspace_links::file_uri(opened_uri)
            && !key.parts.is_empty()
        {
            opened.entry(key).or_default().push((opened_uri, buffer));
        }
    }
    let mut choices = BTreeSet::new();
    for (key, aliases) in &opened {
        if aliases.len() != 1
            || !aliases[0].1.synchronized
            || key.authority != parent.authority
            || !key.parts.starts_with(&parent.parts)
        {
            continue;
        }
        let remaining = &key.parts[parent.parts.len()..];
        let Some(name) = remaining.first() else {
            continue;
        };
        if !name.starts_with(&prefix) {
            continue;
        }
        let folder = remaining.len() > 1;
        let mut child = parent.clone();
        child.parts.push(name.clone());
        // Contradictory open identities cannot establish whether this is a
        // file or a folder. Refuse both instead of generating a broken link.
        if folder && opened.contains_key(&child)
            || !folder
                && opened.keys().any(|other| {
                    other.authority == child.authority
                        && other.parts.len() > child.parts.len()
                        && other.parts.starts_with(&child.parts)
                })
        {
            continue;
        }
        choices.insert((name.clone(), folder));
    }

    let mut items = Vec::new();
    let mut incomplete = false;
    let edit_range = range(index, source, SourceSpan::new(token.start, path_end));
    for (name, folder) in choices {
        let suffix = if folder { "/" } else { "" };
        let replacement = format!("{directory}{}{suffix}", encode_component(&name));
        if replacement.len() > MAX_DESTINATION_BYTES || items.len() >= MAX_COMPLETIONS {
            incomplete = true;
            continue;
        }
        // Keep the literal escape spelling already typed so client filtering
        // agrees with the server's decoded filename-prefix comparison.
        let filter = format!("{typed}{}{suffix}", encode_component(&name[prefix.len()..]));
        items.push(object([
            ("label", string(&format!("{name}{suffix}"))),
            ("kind", number(if folder { 19 } else { 17 })),
            (
                "detail",
                string(if folder {
                    "Folder containing an open document"
                } else {
                    "Open document"
                }),
            ),
            ("filterText", string(&filter)),
            ("insertTextFormat", number(1)),
            (
                "textEdit",
                object([
                    ("range", edit_range.clone()),
                    ("newText", string(&replacement)),
                ]),
            ),
        ]));
    }
    completion_list(items, incomplete)
}

#[cfg(test)]
#[path = "path_completion_tests.rs"]
mod tests;
