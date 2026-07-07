//! Staged filesystem writes for native CLI surfaces.
//!
//! This module is feature-gated behind `cli`; the render core and WASM builds do
//! not read or write the filesystem.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One rendered output to commit to disk.
pub(crate) struct OutputFile<'a> {
    pub(crate) path: &'a Path,
    pub(crate) bytes: &'a [u8],
}

/// A filesystem write error annotated with the destination path that failed.
#[derive(Debug)]
pub(crate) struct OutputWriteError {
    pub(crate) path: PathBuf,
    pub(crate) source: io::Error,
}

/// Stage every output in a same-directory temporary file, then replace the
/// destination paths only after all staging writes have succeeded.
pub(crate) fn write_outputs_staged(outputs: &[OutputFile<'_>]) -> Result<(), OutputWriteError> {
    preflight_outputs(outputs)?;

    let mut staged = Vec::with_capacity(outputs.len());
    for output in outputs {
        match stage_output(output) {
            Ok(temp_path) => staged.push(StagedOutput {
                temp_path,
                final_path: output.path.to_path_buf(),
            }),
            Err(source) => {
                cleanup_staged(&staged);
                return Err(OutputWriteError {
                    path: output.path.to_path_buf(),
                    source,
                });
            }
        }
    }

    commit_staged(staged)
}

/// True iff two paths name the same existing on-disk file.
#[cfg(feature = "batch")]
pub(crate) fn same_existing_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
}

fn preflight_outputs(outputs: &[OutputFile<'_>]) -> Result<(), OutputWriteError> {
    let mut seen: Vec<&Path> = Vec::new();
    for output in outputs {
        if output.path.is_dir() {
            return Err(OutputWriteError {
                path: output.path.to_path_buf(),
                source: io::Error::new(io::ErrorKind::AlreadyExists, "destination is a directory"),
            });
        }
        if seen
            .iter()
            .any(|path| same_output_destination(path, output.path))
        {
            return Err(OutputWriteError {
                path: output.path.to_path_buf(),
                source: io::Error::new(io::ErrorKind::InvalidInput, "duplicate output path"),
            });
        }
        seen.push(output.path);
    }
    Ok(())
}

fn same_output_destination(a: &Path, b: &Path) -> bool {
    a == b
        || same_existing_output_entry(a, b)
        || same_parent_output_entry(a, b)
        || lexical_output_identity(a) == lexical_output_identity(b)
}

fn same_existing_output_entry(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
            (Ok(ma), Ok(mb)) => ma.dev() == mb.dev() && ma.ino() == mb.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(ca), Ok(cb)) => ca == cb,
            _ => false,
        }
    }
}

fn same_parent_output_entry(a: &Path, b: &Path) -> bool {
    if !same_file_name(a.file_name(), b.file_name()) {
        return false;
    }
    let a_parent = output_parent(a);
    let b_parent = output_parent(b);
    match (
        std::fs::canonicalize(a_parent),
        std::fs::canonicalize(b_parent),
    ) {
        (Ok(a), Ok(b)) => stable_path_eq(&a, &b),
        _ => false,
    }
}

fn output_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn same_file_name(a: Option<&OsStr>, b: Option<&OsStr>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => stable_os_str_eq(a, b),
        _ => false,
    }
}

fn stable_path_eq(a: &Path, b: &Path) -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        a == b
    }
}

fn stable_os_str_eq(a: &OsStr, b: &OsStr) -> bool {
    #[cfg(any(windows, target_os = "macos"))]
    {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        a == b
    }
}

fn lexical_output_identity(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    normalize_lexical_path(&absolute)
}

fn normalize_lexical_path(path: &Path) -> PathBuf {
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
    #[cfg(any(windows, target_os = "macos"))]
    {
        PathBuf::from(normalized.to_string_lossy().to_lowercase())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        normalized
    }
}

fn stage_output(output: &OutputFile<'_>) -> io::Result<PathBuf> {
    for _ in 0..128 {
        let temp_path = temp_path_for(output.path, "tmp")?;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(mut file) => {
                if let Err(err) = file.write_all(output.bytes).and_then(|()| file.flush()) {
                    let _ = std::fs::remove_file(&temp_path);
                    return Err(err);
                }
                return Ok(temp_path);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a temporary output path",
    ))
}

#[derive(Debug)]
struct StagedOutput {
    temp_path: PathBuf,
    final_path: PathBuf,
}

#[derive(Debug)]
struct CommittedOutput {
    final_path: PathBuf,
    backup_path: Option<PathBuf>,
}

fn commit_staged(staged: Vec<StagedOutput>) -> Result<(), OutputWriteError> {
    let mut committed = Vec::with_capacity(staged.len());
    for item in &staged {
        match commit_one(item) {
            Ok(backup_path) => committed.push(CommittedOutput {
                final_path: item.final_path.clone(),
                backup_path,
            }),
            Err(source) => {
                rollback_committed(&committed);
                cleanup_staged(&staged);
                return Err(OutputWriteError {
                    path: item.final_path.clone(),
                    source,
                });
            }
        }
    }

    for committed in &committed {
        if let Some(backup) = &committed.backup_path {
            let _ = std::fs::remove_file(backup);
        }
    }
    Ok(())
}

fn commit_one(item: &StagedOutput) -> io::Result<Option<PathBuf>> {
    let backup_path = match std::fs::symlink_metadata(&item.final_path) {
        Ok(meta) => {
            if meta.file_type().is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "destination is a directory",
                ));
            }
            let backup = vacant_temp_path_for(&item.final_path, "bak")?;
            std::fs::rename(&item.final_path, &backup)?;
            Some(backup)
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };

    if let Err(err) = std::fs::rename(&item.temp_path, &item.final_path) {
        if let Some(backup) = &backup_path {
            let _ = std::fs::rename(backup, &item.final_path);
        }
        return Err(err);
    }
    Ok(backup_path)
}

fn rollback_committed(committed: &[CommittedOutput]) {
    for item in committed.iter().rev() {
        let _ = std::fs::remove_file(&item.final_path);
        if let Some(backup) = &item.backup_path {
            let _ = std::fs::rename(backup, &item.final_path);
        }
    }
}

fn cleanup_staged(staged: &[StagedOutput]) {
    for item in staged {
        let _ = std::fs::remove_file(&item.temp_path);
    }
}

fn vacant_temp_path_for(path: &Path, tag: &str) -> io::Result<PathBuf> {
    for _ in 0..128 {
        let candidate = temp_path_for(path, tag)?;
        if !path_entry_exists(&candidate)? {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a temporary backup path",
    ))
}

fn path_entry_exists(path: &Path) -> io::Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

fn temp_path_for(path: &Path, tag: &str) -> io::Result<PathBuf> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    let count = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut temp_name = OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".fmd-{tag}-{}-{count}", std::process::id()));
    Ok(parent.join(temp_name))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fresh_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fmd-file-write-{tag}-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn staging_failure_preserves_existing_outputs() {
        let dir = fresh_dir("stage-fail");
        let html = dir.join("doc.html");
        let pdf = dir.join("missing").join("doc.pdf");
        std::fs::write(&html, "old html").unwrap();

        let err = write_outputs_staged(&[
            OutputFile {
                path: &html,
                bytes: b"new html",
            },
            OutputFile {
                path: &pdf,
                bytes: b"new pdf",
            },
        ])
        .expect_err("missing parent should fail while staging");

        assert_eq!(err.path, pdf);
        assert_eq!(std::fs::read_to_string(&html).unwrap(), "old html");
        assert!(
            std::fs::read_dir(&dir).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".fmd-tmp")),
            "staging temp files should be cleaned"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn successful_write_replaces_existing_outputs_after_staging() {
        let dir = fresh_dir("success");
        let html = dir.join("doc.html");
        let pdf = dir.join("doc.pdf");
        std::fs::write(&html, "old html").unwrap();

        write_outputs_staged(&[
            OutputFile {
                path: &html,
                bytes: b"new html",
            },
            OutputFile {
                path: &pdf,
                bytes: b"new pdf",
            },
        ])
        .unwrap();

        assert_eq!(std::fs::read_to_string(&html).unwrap(), "new html");
        assert_eq!(std::fs::read_to_string(&pdf).unwrap(), "new pdf");
        assert!(
            std::fs::read_dir(&dir).unwrap().all(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                !name.contains(".fmd-tmp") && !name.contains(".fmd-bak")
            }),
            "temporary files should not remain after a successful commit"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn directory_destinations_fail_before_staging() {
        let dir = fresh_dir("dir-dest");
        let html = dir.join("doc.html");
        std::fs::create_dir_all(&html).unwrap();

        let err = write_outputs_staged(&[OutputFile {
            path: &html,
            bytes: b"new html",
        }])
        .expect_err("directory destination should be rejected");

        assert_eq!(err.path, html);
        assert_eq!(err.source.kind(), io::ErrorKind::AlreadyExists);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn duplicate_lexical_destinations_fail_before_staging() {
        let dir = fresh_dir("duplicate-alias");
        let path = dir.join("doc.html");
        let alias = dir.join(".").join("doc.html");

        let err = write_outputs_staged(&[
            OutputFile {
                path: &path,
                bytes: b"first",
            },
            OutputFile {
                path: &alias,
                bytes: b"second",
            },
        ])
        .expect_err("lexically aliased destinations should be rejected");

        assert_eq!(err.path, alias);
        assert_eq!(err.source.kind(), io::ErrorKind::InvalidInput);
        assert!(
            !path.exists(),
            "duplicate preflight failure must not create the shared destination"
        );
        assert!(
            std::fs::read_dir(&dir).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".fmd-tmp")),
            "duplicate preflight failure must not stage temp files"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_existing_file_destinations_fail_before_staging() {
        let dir = fresh_dir("duplicate-hardlink");
        let path = dir.join("doc.html");
        let alias = dir.join("alias.html");
        std::fs::write(&path, "old").unwrap();
        std::fs::hard_link(&path, &alias).unwrap();

        let err = write_outputs_staged(&[
            OutputFile {
                path: &path,
                bytes: b"first",
            },
            OutputFile {
                path: &alias,
                bytes: b"second",
            },
        ])
        .expect_err("hard-linked output destinations should be rejected");

        assert_eq!(err.path, alias);
        assert_eq!(err.source.kind(), io::ErrorKind::InvalidInput);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(std::fs::read_to_string(&alias).unwrap(), "old");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn duplicate_symlink_parent_destinations_fail_before_staging() {
        let dir = fresh_dir("duplicate-symlink-parent");
        let real_dir = dir.join("real");
        let link_dir = dir.join("link");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
        let path = real_dir.join("doc.html");
        let alias = link_dir.join("doc.html");

        let err = write_outputs_staged(&[
            OutputFile {
                path: &path,
                bytes: b"first",
            },
            OutputFile {
                path: &alias,
                bytes: b"second",
            },
        ])
        .expect_err("symlinked parent aliases should be rejected");

        assert_eq!(err.path, alias);
        assert_eq!(err.source.kind(), io::ErrorKind::InvalidInput);
        assert!(
            !path.exists(),
            "duplicate preflight failure must not create the shared destination"
        );
        assert!(
            std::fs::read_dir(&real_dir).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".fmd-tmp")),
            "duplicate preflight failure must not stage temp files"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn rollback_restores_broken_symlink_destination() {
        let dir = fresh_dir("rollback-broken-symlink");
        let target = dir.join("missing-target");
        let final_path = dir.join("doc.html");
        let temp_path = dir.join(".doc.html.fmd-tmp-test");
        let failing_final = dir.join("later.html");
        let missing_temp = dir.join(".later.html.fmd-tmp-missing");
        std::os::unix::fs::symlink(&target, &final_path).unwrap();
        std::fs::write(&temp_path, "new html").unwrap();

        let err = commit_staged(vec![
            StagedOutput {
                temp_path: temp_path.clone(),
                final_path: final_path.clone(),
            },
            StagedOutput {
                temp_path: missing_temp,
                final_path: failing_final,
            },
        ])
        .expect_err("missing second temp should force rollback after first commit");

        assert_eq!(err.path, dir.join("later.html"));
        let restored = std::fs::symlink_metadata(&final_path)
            .expect("rollback must restore the original broken symlink");
        assert!(restored.file_type().is_symlink());
        assert_eq!(std::fs::read_link(&final_path).unwrap(), target);
        assert!(!temp_path.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_refuses_directory_that_appears_after_staging() {
        let dir = fresh_dir("commit-directory-race");
        let final_path = dir.join("doc.html");
        let temp_path = dir.join(".doc.html.fmd-tmp-test");
        std::fs::write(&temp_path, "new html").unwrap();
        std::fs::create_dir_all(&final_path).unwrap();

        let err = commit_staged(vec![StagedOutput {
            temp_path: temp_path.clone(),
            final_path: final_path.clone(),
        }])
        .expect_err("a directory destination that appears after preflight must fail");

        assert_eq!(err.path, final_path);
        assert_eq!(err.source.kind(), io::ErrorKind::AlreadyExists);
        assert!(final_path.is_dir(), "the directory destination must remain");
        assert!(
            !temp_path.exists(),
            "failed commit must clean the staged temp file"
        );
        assert!(
            std::fs::read_dir(&dir).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".fmd-bak")),
            "failed directory commit must not leave backup artifacts"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
