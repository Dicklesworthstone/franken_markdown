//! Native output admission shared by batch and directory watch.
//! Tracks existing file identities and missing paths without opening contents.

use std::path::{Path, PathBuf};

pub(crate) fn output_keys_are_case_folded() -> bool {
    // Avoid false collision failures on ordinary case-sensitive Unix outputs.
    // Windows and default macOS volumes are commonly case-insensitive, so fold
    // there to catch Doc.md/doc.md overwrite races before rendering.
    cfg!(any(windows, target_os = "macos"))
}

pub(crate) fn normalize_lexical_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::ParentDir) | Some(Component::CurDir) | None => {
                    normalized.push("..");
                }
            },
            Component::Normal(part) => normalized.push(part),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

/// Resolve a path's identity even before its final file or directories exist.
/// Canonicalizing the nearest existing ancestor preserves directory aliases;
/// following a dangling alias also protects the missing file it will resolve
/// to once created. Bound symlink traversal so cycles cannot stall preflight.
fn protected_path_identity(path: &Path) -> Option<PathBuf> {
    let mut path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut followed_links = 0;
    'resolve: loop {
        let mut ancestor = path.as_path();
        let mut suffix = Vec::new();
        loop {
            if let Ok(mut identity) = std::fs::canonicalize(ancestor) {
                for component in suffix.iter().rev() {
                    identity.push(component);
                }
                let identity = normalize_lexical_path(&identity);
                return Some(if output_keys_are_case_folded() {
                    PathBuf::from(identity.to_string_lossy().to_lowercase())
                } else {
                    identity
                });
            }
            if let Ok(target) = std::fs::read_link(ancestor) {
                if followed_links == 40 {
                    return None;
                }
                followed_links += 1;
                let mut target = if target.is_absolute() {
                    target
                } else {
                    ancestor.parent()?.join(target)
                };
                for component in suffix.iter().rev() {
                    target.push(component);
                }
                path = target;
                continue 'resolve;
            }
            suffix.push(
                ancestor
                    .components()
                    .next_back()?
                    .as_os_str()
                    .to_os_string(),
            );
            ancestor = ancestor.parent()?;
        }
    }
}

#[derive(Default)]
pub(crate) struct ProtectedInputs {
    identities: std::collections::BTreeMap<PathBuf, PathBuf>,
    #[cfg(unix)]
    inodes: std::collections::BTreeMap<(u64, u64), PathBuf>,
}

impl ProtectedInputs {
    pub(crate) fn insert(&mut self, path: &Path) {
        if let Some(identity) = protected_path_identity(path) {
            self.identities
                .entry(identity)
                .or_insert_with(|| path.to_path_buf());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(path) {
                self.inodes
                    .entry((meta.dev(), meta.ino()))
                    .or_insert_with(|| path.to_path_buf());
            }
        }
    }

    pub(crate) fn source_for(&self, output: &Path) -> Option<&Path> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(output)
                && let Some(source) = self.inodes.get(&(meta.dev(), meta.ino()))
            {
                return Some(source);
            }
        }
        protected_path_identity(output)
            .and_then(|path| self.identities.get(&path))
            .map(PathBuf::as_path)
    }
}
