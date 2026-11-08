//! Explicit collection faces become the same native publications as the source
//! standalone fonts, including retained watch inputs and book/batch adapters.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const COLLECTION: &[u8] = include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV1.ttc");
const COLLECTION_V2: &[u8] =
    include_bytes!("../fmd-font/fonts/test-collection/FmdMixedFacesV2.otc");
const TRUETYPE: &[u8] =
    include_bytes!("../fmd-font/fonts/test-collection/FmdCollectionTrueType.ttf");
const CFF: &[u8] = include_bytes!("../fmd-font/fonts/test-cid-cff/FmdCidFormat3.otf");
const SOURCE: &str = "# AB日\n\nAB日\n";

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "fmd-collection-fonts-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("book")).unwrap();
        fs::write(root.join("book/one.md"), SOURCE).unwrap();
        fs::write(root.join("fonts.ttc"), COLLECTION).unwrap();
        fs::write(root.join("fonts.otc"), COLLECTION_V2).unwrap();
        fs::write(root.join("standalone.ttf"), TRUETYPE).unwrap();
        fs::write(root.join("standalone.otf"), CFF).unwrap();
        Self(root)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_fmd"));
        command
            .current_dir(&self.0)
            .env("SOURCE_DATE_EPOCH", "1700000000")
            .arg("--no-config");
        command
    }

    fn bytes(&self, name: &str) -> Vec<u8> {
        fs::read(self.0.join(name)).unwrap()
    }

    fn render(&self, target: &str, font: &str, face: Option<u32>, stem: &str) -> Vec<Vec<u8>> {
        let extension = if target == "both" { "html" } else { target };
        let path = format!("{stem}.{extension}");
        let mut command = self.command();
        command.args(["book/one.md", "--to", target, "--out", &path]);
        font_options(&mut command, font, face);
        success(command.output().unwrap());
        let mut output = vec![self.bytes(&path)];
        if target == "both" {
            output.push(self.bytes(&format!("{stem}.pdf")));
        }
        output
    }

    fn book(&self, target: &str, font: &str, face: Option<u32>, stem: &str) -> Vec<u8> {
        let mut command = self.command();
        command.args(["book", "book", "--to", target]);
        let path = if target == "html" {
            command.args(["--out-dir", stem]);
            format!("{stem}/one.html")
        } else {
            let path = format!("{stem}.{target}");
            command.args(["--out", &path]);
            path
        };
        font_options(&mut command, font, face);
        success(command.output().unwrap());
        self.bytes(&path)
    }
}

fn font_options(command: &mut Command, font: &str, face: Option<u32>) {
    command
        .arg("--pdf-font")
        .arg(format!("body-regular={font}"));
    if let Some(index) = face {
        command.arg("--pdf-font-face").arg(index.to_string());
    }
}

fn success(output: Output) {
    assert!(
        output.status.success(),
        "status={:?} stdout={} stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn selected_truetype_and_cid_faces_match_standalone_exports() {
    let workspace = Workspace::new();
    for target in ["both", "svg", "epub"] {
        let mut previous = None;
        for (face, standalone) in [(0, "standalone.ttf"), (1, "standalone.otf")] {
            let expected = workspace.render(target, standalone, None, "standalone");
            for collection in ["fonts.ttc", "fonts.otc"] {
                let actual = workspace.render(target, collection, Some(face), "selected");
                assert_eq!(actual, expected, "face {face}, {target}, {collection}");
            }
            if let Some(previous) = previous {
                assert_ne!(expected, previous, "different faces must change {target}");
            }
            previous = Some(expected);
        }
    }
    assert_eq!(workspace.bytes("fonts.ttc"), COLLECTION);
    assert_eq!(workspace.bytes("fonts.otc"), COLLECTION_V2);
}

#[test]
fn independent_slot_selectors_preserve_weight_options() {
    let workspace = Workspace::new();
    let mut selected = workspace.command();
    selected.args([
        "book/one.md",
        "--to",
        "both",
        "--out",
        "selected.html",
        "--pdf-font",
        "body-regular=fonts.ttc",
        "--pdf-font-face",
        "0",
        "--pdf-font",
        "body-bold=fonts.otc",
        "--pdf-font-face",
        "bold=1",
        "--pdf-font-weight",
        "650",
        "--pdf-font-weight",
        "body-bold=750",
    ]);
    success(selected.output().unwrap());
    success(
        workspace
            .command()
            .args([
                "book/one.md",
                "--to",
                "both",
                "--out",
                "standalone.html",
                "--pdf-font",
                "body-regular=standalone.ttf",
                "--pdf-font",
                "body-bold=standalone.otf",
                "--pdf-font-weight",
                "650",
                "--pdf-font-weight",
                "body-bold=750",
            ])
            .output()
            .unwrap(),
    );
    for extension in ["html", "pdf"] {
        assert_eq!(
            workspace.bytes(&format!("selected.{extension}")),
            workspace.bytes(&format!("standalone.{extension}"))
        );
    }
}

#[test]
fn native_book_uses_the_same_selected_collection_face() {
    let workspace = Workspace::new();
    for target in ["html", "pdf", "epub"] {
        assert_eq!(
            workspace.book(target, "fonts.otc", Some(1), "selected"),
            workspace.book(target, "standalone.otf", None, "standalone"),
            "book {target}"
        );
    }
}

#[cfg(feature = "batch")]
#[test]
fn batch_shares_the_normalized_face_across_both_outputs_and_all_inputs() {
    let workspace = Workspace::new();
    fs::write(workspace.0.join("book/two.md"), "# BA日\n\nBA日\n").unwrap();
    for (output, font, face) in [
        ("selected", "fonts.ttc", Some(1)),
        ("standalone", "standalone.otf", None),
    ] {
        let mut command = workspace.command();
        command.args([
            "batch",
            "book",
            "--to",
            "both",
            "--out-dir",
            output,
            "--workers",
            "1",
        ]);
        font_options(&mut command, font, face);
        success(command.output().unwrap());
    }
    for name in ["one", "two"] {
        for extension in ["html", "pdf"] {
            assert_eq!(
                workspace.bytes(&format!("selected/{name}.{extension}")),
                workspace.bytes(&format!("standalone/{name}.{extension}"))
            );
        }
    }
}

#[test]
fn invalid_selectors_fail_before_output_and_ambiguous_slots_fail_before_io() {
    let workspace = Workspace::new();
    let cases: &[(&[&str], &str)] = &[
        (&["--pdf-font-face", "1"], "requires a mapped --pdf-font"),
        (
            &[
                "--pdf-font",
                "regular=missing.ttc",
                "--pdf-font-face",
                "0",
                "--pdf-font-face",
                "regular=1",
            ],
            "duplicate --pdf-font-face",
        ),
        (
            &[
                "--pdf-font",
                "regular=missing.ttc",
                "--pdf-font-face",
                "mono=0",
            ],
            "requires a mapped --pdf-font",
        ),
        (
            &["--pdf-font", "regular=missing.ttc", "--pdf-font-face=-1"],
            "zero-based face index",
        ),
        (&["--pdf-font", "regular=fonts.ttc"], "--pdf-font-face"),
        (
            &["--pdf-font", "regular=fonts.ttc", "--pdf-font-face", "2"],
            "face 2",
        ),
        (
            &[
                "--pdf-font",
                "regular=standalone.ttf",
                "--pdf-font-face",
                "0",
            ],
            "not a TTC/OTC collection",
        ),
    ];
    for (flags, message) in cases {
        for route in [
            vec!["book/one.md", "--out", "untouched.html"],
            vec!["book", "book", "--to", "epub", "--out", "untouched.epub"],
        ] {
            let output = workspace
                .command()
                .args(route)
                .args(*flags)
                .arg("--json")
                .output()
                .unwrap();
            assert_eq!(
                output.status.code(),
                Some(64),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stdout.is_empty());
            assert!(String::from_utf8_lossy(&output.stderr).contains(message));
            assert!(!workspace.0.join("untouched.html").exists());
            assert!(!workspace.0.join("untouched.epub").exists());
        }
    }
}

#[test]
fn original_collection_files_remain_protected_after_later_slot_assignments() {
    let workspace = Workspace::new();
    let expected = workspace.render("html", "standalone.otf", None, "direct");
    success(
        workspace
            .command()
            .args([
                "book/one.md",
                "--out",
                "overridden.html",
                "--pdf-font",
                "regular=standalone.ttf",
                "--pdf-font",
                "regular=fonts.ttc",
                "--pdf-font-face",
                "1",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(workspace.bytes("overridden.html"), expected[0]);
    let output = workspace
        .command()
        .args([
            "book/one.md",
            "--out",
            "fonts.ttc",
            "--json",
            "--pdf-font",
            "regular=standalone.ttf",
            "--pdf-font",
            "regular=fonts.ttc",
            "--pdf-font",
            "regular=fonts.otc",
            "--pdf-font-face",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&output.stderr).contains("overwrite"));
    assert_eq!(workspace.bytes("fonts.ttc"), COLLECTION);
    assert_eq!(workspace.bytes("fonts.otc"), COLLECTION_V2);
    assert_eq!(workspace.bytes("standalone.ttf"), TRUETYPE);

    fs::write(workspace.0.join("protected.epub"), COLLECTION).unwrap();
    let output = workspace
        .command()
        .args([
            "book",
            "book",
            "--to",
            "epub",
            "--out",
            "protected.epub",
            "--json",
            "--pdf-font",
            "regular=protected.epub",
            "--pdf-font-face",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(73));
    assert!(String::from_utf8_lossy(&output.stderr).contains("overwrite"));
    assert_eq!(workspace.bytes("protected.epub"), COLLECTION);
}

struct Watch {
    child: Child,
    events: Receiver<String>,
}

impl Watch {
    fn start(workspace: &Workspace) -> Self {
        let mut child = workspace
            .command()
            .args([
                "watch",
                "book/one.md",
                "--out",
                "watched.html",
                "--interval",
                "10",
                "--json",
                "--pdf-font",
                "regular=fonts.ttc",
                "--pdf-font-face",
                "1",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (sender, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = sender.send(line);
            }
        });
        Self { child, events }
    }

    fn event(&self, expected: &str) {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line.contains(expected) => return,
                Ok(line) => seen.push(line),
                Err(error) => panic!("waiting for {expected}: {error}; events={seen:?}"),
            }
        }
    }

    fn matches(&mut self, path: &Path, expected: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if fs::read(path).is_ok_and(|actual| actual == expected) {
                return;
            }
            assert!(self.child.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "watch did not publish the selected replacement face"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn replacing_a_collection_refreshes_the_selected_face_without_editing_markdown() {
    let workspace = Workspace::new();
    let original = workspace
        .render("html", "standalone.otf", None, "original")
        .remove(0);
    let replacement = workspace
        .render("html", "standalone.ttf", None, "replacement")
        .remove(0);
    assert_ne!(original, replacement);
    let mut watch = Watch::start(&workspace);
    watch.event("\"event\":\"watching\"");
    watch.matches(&workspace.0.join("watched.html"), &original);
    // Swap the two face offsets, preserving both SFNT directories and shared
    // payloads. The configured index stays 1 while its selected face changes.
    let mut reordered = COLLECTION.to_vec();
    let first: [u8; 4] = reordered[12..16].try_into().unwrap();
    let second: [u8; 4] = reordered[16..20].try_into().unwrap();
    reordered[12..16].copy_from_slice(&second);
    reordered[16..20].copy_from_slice(&first);
    fs::write(workspace.0.join("replacement.ttc"), reordered).unwrap();
    fs::rename(
        workspace.0.join("replacement.ttc"),
        workspace.0.join("fonts.ttc"),
    )
    .unwrap();
    watch.event("\"event\":\"rebuild\"");
    watch.matches(&workspace.0.join("watched.html"), &replacement);
    assert_eq!(workspace.bytes("book/one.md"), SOURCE.as_bytes());
}
