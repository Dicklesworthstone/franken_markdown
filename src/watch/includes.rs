//! Shared bounded native transclusion for render and watch dependency discovery.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use super::Fingerprint;

pub(crate) struct Expansion {
    pub result: Result<String, String>,
    pub(super) dependencies: BTreeMap<PathBuf, Option<Fingerprint>>,
}

/// Resolve through the same include engine used for rendering, including code
/// fences, selectors, recursive paths, cycles and output budgets. Dependencies
/// discovered before an error remain available so watch can recover on save.
pub(crate) fn expand_file_includes(src: &str, input: &Path, max_bytes: u64) -> Expansion {
    let dependencies = RefCell::new(BTreeMap::new());
    let result = (|| {
        if !crate::transclude::has_includes(src) {
            return Ok(src.to_owned());
        }
        let base = input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let root = base
            .canonicalize()
            .map_err(|e| format!("include sandbox root {}: {e}", base.display()))?;
        let resolver = |requested: &str, origin: &str| {
            let base = if origin == "<input>" {
                root.as_path()
            } else {
                Path::new(origin).parent().unwrap_or(&root)
            };
            let candidate = base.join(requested);
            let canonical = match candidate.canonicalize() {
                Ok(path) => path,
                Err(_) => {
                    if let Some(path) = missing_inside_root(&candidate, &root) {
                        dependencies.borrow_mut().insert(path, None);
                    }
                    return Ok(None);
                }
            };
            if !canonical.starts_with(&root) {
                return Err(format!(
                    "include_escape: {} leaves the document root {}",
                    candidate.display(),
                    root.display()
                ));
            }
            // Observe the requested path too: replacing an in-root symlink
            // must not leave the watcher attached only to its old target.
            dependencies.borrow_mut().insert(candidate.clone(), None);
            let metadata = std::fs::metadata(&canonical)
                .map_err(|e| format!("include_read: {}: {e}", candidate.display()))?;
            if !metadata.is_file() {
                return Err(format!(
                    "include_read: {} is not a regular file",
                    candidate.display()
                ));
            }
            if metadata.len() > max_bytes {
                return Err(format!(
                    "include_oversize: {} is {} bytes, over the {max_bytes}-byte input cap",
                    candidate.display(),
                    metadata.len()
                ));
            }
            let file = std::fs::File::open(&canonical)
                .map_err(|e| format!("include_read: {}: {e}", candidate.display()))?;
            let metadata = file
                .metadata()
                .map_err(|e| format!("include_read: {}: {e}", candidate.display()))?;
            if !metadata.is_file() {
                return Err(format!(
                    "include_read: {} is not a regular file",
                    candidate.display()
                ));
            }
            if metadata.len() > max_bytes {
                return Err(format!(
                    "include_oversize: {} is {} bytes, over the {max_bytes}-byte input cap",
                    candidate.display(),
                    metadata.len()
                ));
            }
            let mut bytes = Vec::new();
            file.take(max_bytes.saturating_add(1))
                .read_to_end(&mut bytes)
                .map_err(|e| format!("include_read: {}: {e}", candidate.display()))?;
            if bytes.len() as u64 > max_bytes {
                return Err(format!(
                    "include_oversize: {} exceeds the {max_bytes}-byte input cap",
                    candidate.display()
                ));
            }
            let fingerprint = Fingerprint {
                len: bytes.len() as u64,
                hash: bytes.iter().fold(super::FNV_OFFSET, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(super::FNV_PRIME)
                }),
            };
            dependencies
                .borrow_mut()
                .insert(candidate, Some(fingerprint));
            let source = String::from_utf8(bytes).map_err(|_| {
                format!("include_invalid_utf8: {} is not UTF-8", canonical.display())
            })?;
            Ok(Some((source, canonical.to_string_lossy().into_owned())))
        };
        let budget = usize::try_from(max_bytes)
            .unwrap_or(usize::MAX)
            .min(crate::transclude::DEFAULT_MAX_EXPANDED_BYTES);
        crate::transclude::expand_includes_with_limit(src, &resolver, budget)
            .map_err(|e| e.to_string())
    })();
    Expansion {
        result,
        dependencies: dependencies.into_inner(),
    }
}

/// A missing leaf may be watched, but never through an escaping parent or
/// symlink. Finding the nearest existing ancestor also supports a new directory.
fn missing_inside_root(candidate: &Path, root: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component.as_os_str()),
        }
    }
    if !normalized.starts_with(root) {
        return None;
    }
    let mut existing = candidate;
    while !existing.exists() {
        existing = existing.parent()?;
    }
    existing
        .canonicalize()
        .ok()?
        .starts_with(root)
        .then(|| candidate.to_path_buf())
}
