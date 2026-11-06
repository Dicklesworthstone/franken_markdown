//! Unresolved image aliases and consulted missing files remain book inputs.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const CONFIG: &str = "settings/nested/fmd.conf";

fn workspace(source: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-alias-protection-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), source).unwrap();
    root
}

fn fmd(root: &Path, no_config: bool, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
    if no_config {
        command.arg("--no-config");
    }
    command
        .args(["book", "book"])
        .args(args)
        .arg("--json")
        .env("FMD_CONFIG", root.join(CONFIG))
        .env_remove("SOURCE_DATE_EPOCH")
        .current_dir(root)
        .output()
        .unwrap()
}

fn refused(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(73),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"ok\":false"), "{stderr}");
    assert!(
        stderr.contains("output would overwrite a book input"),
        "{stderr}"
    );
}

fn succeeded(output: &Output) -> String {
    assert!(
        output.status.success(),
        "status {:?}: stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt = String::from_utf8(output.stdout.clone()).unwrap();
    assert!(receipt.contains("\"ok\":true"), "{receipt}");
    receipt
}

#[cfg(unix)]
#[test]
fn cyclic_image_alias_entries_cannot_be_replaced_through_directory_aliases() {
    use std::os::unix::fs::symlink;

    let source = "# A\n\n![Unavailable artwork](loop.svg)\n";
    let root = workspace(source);
    symlink("loop.svg", root.join("book/loop.svg")).unwrap();
    symlink("book", root.join("book-alias")).unwrap();
    for destination in ["book/loop.svg", "book-alias/loop.svg"] {
        refused(&fmd(&root, true, &["--to", "epub", "--out", destination]));
        assert_eq!(
            fs::read_link(root.join("book/loop.svg")).unwrap(),
            Path::new("loop.svg")
        );
        assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
    }

    let receipt = succeeded(&fmd(&root, true, &["--to", "epub", "--out", "safe.epub"]));
    assert!(receipt.contains("image_unavailable: loop.svg"), "{receipt}");
    assert!(receipt.contains("symlink component"), "{receipt}");
    assert!(receipt.contains("\"images\":0"), "{receipt}");
    assert!(fs::read(root.join("safe.epub")).unwrap().starts_with(b"PK"));
    assert_eq!(
        fs::read_link(root.join("book/loop.svg")).unwrap(),
        Path::new("loop.svg")
    );
}

#[cfg(unix)]
#[test]
fn image_alias_entries_survive_when_target_resolution_exceeds_the_link_limit() {
    use std::os::unix::fs::symlink;

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#246"/></svg>"##;
    let source = "# A\n\n![Unavailable artwork](link00.svg)\n";
    let root = workspace(source);
    fs::write(root.join("book/original.svg"), SVG).unwrap();
    let mut links = Vec::new();
    for index in 0..48 {
        let name = format!("link{index:02}.svg");
        let target = if index == 47 {
            "original.svg".to_owned()
        } else {
            format!("link{:02}.svg", index + 1)
        };
        symlink(&target, root.join("book").join(&name)).unwrap();
        links.push((name, target));
    }
    refused(&fmd(
        &root,
        true,
        &["--to", "epub", "--out", "book/link00.svg"],
    ));
    for (name, target) in links {
        assert_eq!(
            fs::read_link(root.join("book").join(&name)).unwrap(),
            Path::new(&target),
            "image alias {name} must remain unchanged"
        );
    }
    assert_eq!(
        fs::read_to_string(root.join("book/original.svg")).unwrap(),
        SVG
    );
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
}

#[test]
fn a_consulted_missing_manifest_cannot_be_created_as_a_publication() {
    let source = "# A\n\nChapter text.\n";
    let root = workspace(source);
    refused(&fmd(
        &root,
        true,
        &["--to", "epub", "--out", "book/book.toml"],
    ));
    assert!(!root.join("book/book.toml").exists());
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
}

#[test]
fn a_missing_config_path_is_reserved_only_when_configuration_is_enabled() {
    let source = "# A\n\nChapter text.\n";
    let root = workspace(source);
    refused(&fmd(&root, false, &["--to", "epub", "--out", CONFIG]));
    assert!(!root.join("settings").exists());
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);

    succeeded(&fmd(&root, true, &["--to", "epub", "--out", CONFIG]));
    assert!(fs::read(root.join(CONFIG)).unwrap().starts_with(b"PK"));
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
}
