//! Referenced image identities remain inputs even when their bytes cannot load.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="#246"/></svg>"##;

fn workspace(source: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "fmd-book-asset-protection-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed),
    ));
    fs::create_dir_all(root.join("book")).unwrap();
    fs::write(root.join("book/a.md"), source).unwrap();
    root
}

fn fmd(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "book", "book"])
        .args(args)
        .arg("--json")
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
    assert!(
        output.stdout.is_empty(),
        "failure must leave data stdout empty"
    );
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

#[test]
fn oversized_markdown_and_raw_html_images_cannot_be_replaced_by_publication() {
    for image in [
        "![Original](logo.svg)",
        "<img src='logo.svg' alt='Original'>",
    ] {
        let source = format!("# A\n\n{image}\n");
        let root = workspace(&source);
        fs::write(root.join("book/logo.svg"), SVG).unwrap();
        let output = fmd(
            &root,
            &[
                "--to",
                "epub",
                "--out",
                "book/logo.svg",
                "--max-pdf-image-bytes",
                "4",
            ],
        );
        refused(&output);
        assert_eq!(fs::read_to_string(root.join("book/logo.svg")).unwrap(), SVG);
        assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
    }
}

#[test]
fn unavailable_image_protection_uses_the_decoded_path_without_its_uri_suffix() {
    let root = workspace(
        "# A\n\n<img src='art/plot%20%231.svg?version=2&amp;view=small#detail' alt='Plot'>\n",
    );
    fs::create_dir(root.join("book/art")).unwrap();
    fs::write(root.join("book/art/plot #1.svg"), SVG).unwrap();
    let output = fmd(
        &root,
        &[
            "--to",
            "epub",
            "--out",
            "book/art/plot #1.svg",
            "--max-pdf-image-bytes",
            "4",
        ],
    );
    refused(&output);
    assert_eq!(
        fs::read_to_string(root.join("book/art/plot #1.svg")).unwrap(),
        SVG
    );
}

#[test]
fn missing_image_paths_cannot_be_created_by_single_file_publication() {
    let source = "# A\n\n![Future artwork](art/nested/missing.svg)\n";
    let root = workspace(source);
    let output = fmd(
        &root,
        &["--to", "epub", "--out", "book/art/nested/missing.svg"],
    );
    refused(&output);
    assert!(!root.join("book/art").exists());
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
}

#[test]
fn a_missing_image_collision_preserves_every_existing_combined_output() {
    let root = workspace("# A\n\n![Future artwork](publication/a.html)\n");
    fs::create_dir(root.join("book/publication")).unwrap();
    let previous = [
        ("index.html", "previous landing page"),
        ("search-index.json", "previous search index"),
        ("book.pdf", "previous PDF publication"),
    ];
    for (name, contents) in previous {
        fs::write(root.join("book/publication").join(name), contents).unwrap();
    }
    let output = fmd(&root, &["--to", "both", "--out-dir", "book/publication"]);
    refused(&output);
    assert!(!root.join("book/publication/a.html").exists());
    for (name, contents) in previous {
        assert_eq!(
            fs::read_to_string(root.join("book/publication").join(name)).unwrap(),
            contents,
            "{name} must survive the rejected publication"
        );
    }
}

#[cfg(unix)]
#[test]
fn rejected_image_symlinks_protect_the_alias_target_and_hard_link_identities() {
    use std::os::unix::fs::{MetadataExt, symlink};

    let root = workspace("# A\n\n![Original](linked.svg)\n");
    fs::write(root.join("outside.svg"), SVG).unwrap();
    symlink("../outside.svg", root.join("book/linked.svg")).unwrap();
    fs::hard_link(root.join("outside.svg"), root.join("hard-link.svg")).unwrap();
    let original_inode = fs::metadata(root.join("outside.svg")).unwrap().ino();
    for destination in ["book/linked.svg", "outside.svg", "hard-link.svg"] {
        let output = fmd(&root, &["--to", "epub", "--out", destination]);
        refused(&output);
        assert_eq!(fs::read_to_string(root.join("outside.svg")).unwrap(), SVG);
        assert_eq!(fs::read_to_string(root.join("hard-link.svg")).unwrap(), SVG);
        assert_eq!(
            fs::metadata(root.join("hard-link.svg")).unwrap().ino(),
            original_inode
        );
        assert_eq!(
            fs::read_link(root.join("book/linked.svg")).unwrap(),
            Path::new("../outside.svg")
        );
    }

    let receipt = succeeded(&fmd(&root, &["--to", "epub", "--out", "safe.epub"]));
    assert!(
        receipt.contains("image_unavailable: linked.svg"),
        "{receipt}"
    );
    assert!(receipt.contains("symlink component"), "{receipt}");
    assert!(receipt.contains("\"images\":0"), "{receipt}");
    assert!(fs::read(root.join("safe.epub")).unwrap().starts_with(b"PK"));
}

#[cfg(unix)]
#[test]
fn loaded_images_are_protected_through_hard_link_output_aliases() {
    use std::os::unix::fs::MetadataExt;

    let root = workspace("# A\n\n![Original](logo.svg)\n");
    fs::write(root.join("book/logo.svg"), SVG).unwrap();
    fs::hard_link(root.join("book/logo.svg"), root.join("published.epub")).unwrap();
    let original_inode = fs::metadata(root.join("book/logo.svg")).unwrap().ino();
    refused(&fmd(&root, &["--to", "epub", "--out", "published.epub"]));
    assert_eq!(fs::read_to_string(root.join("book/logo.svg")).unwrap(), SVG);
    assert_eq!(
        fs::read_to_string(root.join("published.epub")).unwrap(),
        SVG
    );
    assert_eq!(
        fs::metadata(root.join("published.epub")).unwrap().ino(),
        original_inode
    );
}

#[cfg(unix)]
#[test]
fn a_dangling_image_symlink_protects_its_missing_target_without_creating_parents() {
    use std::os::unix::fs::symlink;

    let root = workspace("# A\n\n![Future artwork](linked.svg)\n");
    symlink("../future/missing.svg", root.join("book/linked.svg")).unwrap();
    for destination in ["book/linked.svg", "future/missing.svg"] {
        refused(&fmd(&root, &["--to", "epub", "--out", destination]));
        assert!(!root.join("future").exists());
        assert_eq!(
            fs::read_link(root.join("book/linked.svg")).unwrap(),
            Path::new("../future/missing.svg")
        );
    }
}

#[cfg(unix)]
#[test]
fn a_rejected_directory_alias_protects_missing_descendant_images() {
    use std::os::unix::fs::symlink;

    let root = workspace("# A\n\n![Future artwork](assets/nested/missing.svg)\n");
    fs::create_dir(root.join("external")).unwrap();
    symlink("../external", root.join("book/assets")).unwrap();
    for destination in [
        "book/assets/nested/missing.svg",
        "external/nested/missing.svg",
    ] {
        refused(&fmd(&root, &["--to", "epub", "--out", destination]));
        assert!(!root.join("external/nested").exists());
        assert_eq!(
            fs::read_link(root.join("book/assets")).unwrap(),
            Path::new("../external")
        );
    }
}

#[test]
fn unavailable_images_still_allow_unrelated_publication_with_warnings() {
    let source = "# A\n\n![Large](large.svg) ![Missing](art/missing.svg)\n";
    let root = workspace(source);
    fs::write(root.join("book/large.svg"), SVG).unwrap();
    let receipt = succeeded(&fmd(
        &root,
        &[
            "--to",
            "epub",
            "--out",
            "dist/manual.epub",
            "--max-pdf-image-bytes",
            "4",
        ],
    ));
    assert!(
        receipt.contains("image_unavailable: large.svg"),
        "{receipt}"
    );
    assert!(
        receipt.contains("image_unavailable: art/missing.svg"),
        "{receipt}"
    );
    assert!(receipt.contains("\"images\":0"), "{receipt}");
    assert!(
        fs::read(root.join("dist/manual.epub"))
            .unwrap()
            .starts_with(b"PK")
    );
    assert_eq!(
        fs::read_to_string(root.join("book/large.svg")).unwrap(),
        SVG
    );
    assert_eq!(fs::read_to_string(root.join("book/a.md")).unwrap(), source);
    assert!(!root.join("book/art").exists());
}
