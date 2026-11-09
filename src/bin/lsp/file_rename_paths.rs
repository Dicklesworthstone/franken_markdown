//! Logical file moves; all URI decoding and alias rules come from navigation.

use std::collections::{BTreeMap, BTreeSet};

use super::super::workspace_links::{FileKey, addressed, alternative_targets, file_uri};
use super::super::{Buffer, Json};
use super::Failure;

const MAX_RENAMES: usize = 128;
pub(super) const MAX_DESTINATION_BYTES: usize = 8192;

struct Move {
    old: FileKey,
    new: FileKey,
}

pub(super) struct Plan {
    moves: Vec<Move>,
    before: BTreeSet<FileKey>,
    after: BTreeSet<FileKey>,
}

pub(super) struct Route {
    pub before: FileKey,
    pub after: FileKey,
    pub replacement: Option<String>,
}

fn prefix(parent: &FileKey, child: &FileKey) -> bool {
    parent.authority == child.authority && child.parts.starts_with(&parent.parts)
}

fn overlaps(left: &FileKey, right: &FileKey) -> bool {
    prefix(left, right) || prefix(right, left)
}

fn drive(key: &FileKey) -> Option<&str> {
    key.parts.first().map(String::as_str).filter(|part| {
        part.len() == 2 && part.as_bytes()[0].is_ascii_alphabetic() && part.ends_with(':')
    })
}

fn endpoint(value: &Json, field: &str) -> Result<FileKey, Failure> {
    let uri = value
        .get(field)
        .and_then(Json::as_str)
        .ok_or((-32602, "file rename requires string oldUri and newUri"))?;
    let (key, _) = file_uri(uri).ok_or((
        -32602,
        "file rename requires bounded absolute file URIs without query or fragment",
    ))?;
    if key.parts.is_empty() || key.parts.len() == 1 && drive(&key).is_some() {
        return Err((-32602, "file rename cannot move a URI or drive root"));
    }
    Ok(key)
}

fn resolve(
    known: &BTreeSet<FileKey>,
    key: &FileKey,
    directory: bool,
    aliases: bool,
) -> Result<Option<FileKey>, Failure> {
    if !directory && known.contains(key) {
        return Ok(Some(key.clone()));
    }
    if !aliases {
        return Ok(None);
    }
    let mut matched = alternative_targets(key, directory)
        .into_iter()
        .filter(|candidate| known.contains(candidate));
    let result = matched.next();
    if matched.next().is_some() {
        return Err((
            -32803,
            "file rename cannot choose an ambiguous extension or directory-index target",
        ));
    }
    Ok(result)
}

fn component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            result.push(char::from(byte));
        } else {
            result.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            result.push(char::from(HEX[usize::from(byte >> 4)]));
            result.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    result
}

fn rooted_path(key: &FileKey) -> String {
    let mut result = String::new();
    for (index, part) in key.parts.iter().enumerate() {
        result.push('/');
        if index == 0 && drive(key).is_some() {
            result.push_str(part);
        } else {
            result.push_str(&component(part));
        }
    }
    if result.is_empty() {
        result.push('/');
    }
    result
}

fn uri(key: &FileKey) -> String {
    format!("file://{}{}", key.authority, rooted_path(key))
}

fn relative(origin: &FileKey, target: &FileKey) -> String {
    if origin.authority != target.authority || drive(origin) != drive(target) {
        return uri(target);
    }
    let parent = &origin.parts[..origin.parts.len() - 1];
    let common = parent
        .iter()
        .zip(&target.parts)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts = vec!["..".to_string(); parent.len() - common];
    parts.extend(target.parts[common..].iter().map(|part| component(part)));
    if parts.is_empty() {
        ".".to_string()
    } else {
        parts.join("/")
    }
}

impl Plan {
    pub(super) fn new(
        params: &Json,
        documents: &BTreeMap<String, Buffer>,
    ) -> Result<Self, Failure> {
        let Some(Json::Array(files)) = params.get("files") else {
            return Err((-32602, "file rename requires a files array"));
        };
        if files.len() > MAX_RENAMES {
            return Err((-32803, "file rename exceeds the 128-operation budget"));
        }
        let mut moves: Vec<Move> = Vec::new();
        let mut old_keys = BTreeSet::new();
        let mut new_keys = BTreeSet::new();
        for file in files {
            let old = endpoint(file, "oldUri")?;
            let new = endpoint(file, "newUri")?;
            if !old_keys.insert(old.clone()) || !new_keys.insert(new.clone()) {
                return Err((
                    -32602,
                    "file rename has duplicate source or destination identities",
                ));
            }
            if old == new {
                continue;
            }
            if overlaps(&old, &new)
                || moves
                    .iter()
                    .any(|movement| overlaps(&old, &movement.old) || overlaps(&new, &movement.new))
            {
                return Err((
                    -32803,
                    "file rename has overlapping paths or moves a path inside itself",
                ));
            }
            moves.push(Move { old, new });
        }
        let mut before = BTreeSet::new();
        for opened in documents.keys() {
            if let Some((key, false)) = file_uri(opened) {
                if !before.insert(key) {
                    return Err((
                        -32803,
                        "file rename cannot edit ambiguous opened file URI aliases",
                    ));
                }
            }
        }
        // An opened text document and one of its descendants cannot both be
        // files. Do not guess whether a request endpoint names a directory.
        for left in &before {
            if before
                .iter()
                .any(|right| left != right && prefix(left, right))
            {
                return Err((
                    -32803,
                    "opened file identities overlap as files and directories",
                ));
            }
        }
        let mut plan = Self {
            moves,
            before,
            after: BTreeSet::new(),
        };
        let mut owners = BTreeMap::new();
        for key in plan
            .before
            .iter()
            .chain(plan.moves.iter().map(|movement| &movement.old))
        {
            let destination = plan.moved(key);
            if let Some(previous) = owners.insert(destination.clone(), key.clone())
                && previous != *key
            {
                return Err((
                    -32803,
                    "file rename collides with another known file identity",
                ));
            }
        }
        for movement in &plan.moves {
            for original in &plan.before {
                let renamed = plan.moved(original);
                if (prefix(&movement.new, &renamed) && !prefix(&movement.old, original))
                    || (prefix(&renamed, &movement.new) && renamed != movement.new)
                {
                    return Err((
                        -32803,
                        "file rename collides with a known file or directory subtree",
                    ));
                }
            }
        }
        plan.after = plan.before.iter().map(|key| plan.moved(key)).collect();
        for left in &plan.after {
            if plan
                .after
                .iter()
                .any(|right| left != right && prefix(left, right))
            {
                return Err((
                    -32803,
                    "renamed file identities overlap as files and directories",
                ));
            }
        }
        Ok(plan)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.moves.is_empty()
    }

    fn moved(&self, key: &FileKey) -> FileKey {
        self.moves
            .iter()
            .find(|movement| prefix(&movement.old, key))
            .map_or_else(
                || key.clone(),
                |movement| {
                    let mut result = movement.new.clone();
                    result
                        .parts
                        .extend_from_slice(&key.parts[movement.old.parts.len()..]);
                    result
                },
            )
    }

    fn incoming_alias(&self, address: &FileKey, directory: bool) -> bool {
        let candidates = alternative_targets(address, directory);
        self.moves.iter().any(|movement| {
            // Opened sources already have an exact post-move file identity.
            // A closed endpoint can be a file OR a directory. Do not guess:
            // either can introduce an exact-path or index shadow of an alias.
            !self.before.contains(&movement.old)
                && ((!directory && movement.new == *address)
                    || candidates
                        .iter()
                        .any(|candidate| prefix(&movement.new, candidate)))
        })
    }

    pub(super) fn route(
        &self,
        origin: &str,
        destination: &str,
        aliases: bool,
    ) -> Result<Option<Route>, Failure> {
        let Some((address, directory)) = addressed(origin, destination) else {
            return Ok(None);
        };
        let resolved = resolve(&self.before, &address, directory, aliases)?;
        let direct_after = self.moved(&address);
        if aliases
            && resolved.is_none()
            && address == direct_after
            && alternative_targets(&address, directory)
                .iter()
                .any(|candidate| self.moved(candidate) != *candidate)
        {
            return Err((
                -32803,
                "file rename needs the target opened to disambiguate an extension or index alias",
            ));
        }
        let before = resolved.as_ref().unwrap_or(&address).clone();
        let after = self.moved(&before);
        let new_origin = file_uri(origin)
            .filter(|(key, directory)| !directory && !key.parts.is_empty())
            .map(|(key, _)| self.moved(&key));
        let new_uri = new_origin.as_ref().map(uri);
        let unchanged = addressed(new_uri.as_deref().unwrap_or(origin), destination)
            .is_some_and(|(key, new_directory)| {
                if resolved.is_some() {
                    // A move can introduce a new index alias or exact-path
                    // shadow. Spell the original target explicitly in that case.
                    matches!(resolve(&self.after, &key, new_directory, aliases), Ok(Some(found)) if found == after)
                        && ((!new_directory && key == after) || !self.incoming_alias(&key, new_directory))
                } else {
                    key == after && new_directory == directory
                }
            });
        if unchanged {
            return Ok(Some(Route {
                before,
                after,
                replacement: None,
            }));
        }
        let separator = destination.find(['?', '#']).unwrap_or(destination.len());
        let path = &destination[..separator];
        let suffix = &destination[separator..];
        let mut replacement = if path
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("file:"))
        {
            uri(&after)
        } else if path.starts_with('/') {
            if new_origin
                .as_ref()
                .is_some_and(|key| key.authority == after.authority)
            {
                rooted_path(&after)
            } else {
                uri(&after)
            }
        } else {
            let origin = new_origin.as_ref().ok_or((
                -32803,
                "relative file rename has no unambiguous source file identity",
            ))?;
            relative(origin, &after)
        };
        if directory && resolved.is_none() && !replacement.ends_with('/') {
            replacement.push('/');
        }
        replacement.push_str(suffix);
        if replacement.len() > MAX_DESTINATION_BYTES {
            return Err((-32803, "renamed file destination exceeds its byte budget"));
        }
        // Reuse the same decoder to prove the new URI denotes the intended
        // target; percent-encoded names must never acquire extra separators.
        let (rewritten, rewritten_directory) =
            addressed(new_uri.as_deref().unwrap_or(origin), &replacement).ok_or((
                -32803,
                "renamed file destination cannot be represented safely",
            ))?;
        if rewritten != after || rewritten_directory != (directory && resolved.is_none()) {
            return Err((
                -32803,
                "renamed file destination changed its logical target",
            ));
        }
        Ok(Some(Route {
            before,
            after,
            replacement: Some(replacement),
        }))
    }
}
