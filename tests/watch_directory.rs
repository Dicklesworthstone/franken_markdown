//! Directory watch renders complete trees and keeps every affected output fresh.
#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "fmd-watch-directory-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Watch {
    child: Child,
    events: Receiver<String>,
}
impl Watch {
    fn start(input: &Path, output: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fmd"))
            .args(["--no-config", "watch"])
            .arg(input)
            .arg("--out")
            .arg(output)
            .args(["--interval", "10", "--json"])
            .args(extra)
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
    fn event(&self, expected: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut seen = Vec::new();
        loop {
            match self
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) if line.contains(expected) => return line,
                Ok(line) => seen.push(line),
                Err(error) => panic!("waiting for {expected}: {error}; events: {seen:?}"),
            }
        }
    }

    fn assert_refused(&mut self) {
        self.event("refusing to overwrite watched input");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(64));
                return;
            }
            assert!(
                Instant::now() < deadline,
                "unsafe initial watch did not exit"
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

fn wait_contains(path: &Path, content: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        if contents.contains(content) {
            return contents;
        }
        assert!(
            Instant::now() < deadline,
            "{} never contained {content}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_changed(path: &Path, previous: &[u8]) -> Vec<u8> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(bytes) = std::fs::read(path)
            && !bytes.is_empty()
            && bytes != previous
        {
            return bytes;
        }
        assert!(
            Instant::now() < deadline,
            "{} was not rebuilt",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn initial_export_preserves_relative_tree_and_initializes_preview() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# First chapter\n");
    dir.write("sources/nested/b.markdown", "# Nested chapter\n");
    let out = dir.path("output");
    std::fs::create_dir(&out).unwrap();
    let watch = Watch::start(&dir.path("sources"), &out, &["--serve"]);
    let event = watch.event("\"event\":\"watching\"");
    assert!(out.join("a.html").is_file());
    assert!(out.join("nested/b.html").is_file());
    assert!(!dir.path("sources/a.html").exists());
    assert!(!dir.path("sources/nested/b.html").exists());
    let port: u16 = event
        .split("http://127.0.0.1:")
        .nth(1)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.contains("First chapter"));
    assert!(response.contains("EventSource('/events')"));
}

#[test]
fn simultaneous_edits_shared_styles_and_new_files_rebuild_all_needed_outputs() {
    let dir = Directory::new();
    let first = dir.write("sources/a.md", "# Alpha original\n");
    let second = dir.write("sources/b.md", "# Beta original\n");
    let css = dir.write("theme.css", "body{color:red}\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(
        &dir.path("sources"),
        &output,
        &["--css", css.to_str().unwrap()],
    );
    watch.event("\"event\":\"watching\"");
    std::fs::write(&first, "# Alpha changed\n").unwrap();
    std::fs::write(&second, "# Beta changed\n").unwrap();
    wait_contains(&output.join("a.html"), "Alpha changed");
    wait_contains(&output.join("b.html"), "Beta changed");
    std::fs::write(&css, "body{color:purple}\n").unwrap();
    wait_contains(&output.join("a.html"), "color:purple");
    wait_contains(&output.join("b.html"), "color:purple");
    dir.write("sources/new/c.md", "# Added chapter\n");
    wait_contains(&output.join("new/c.html"), "Added chapter");
    std::fs::remove_file(&first).unwrap();
    watch.event("watch_source_removed");
    assert!(
        output.join("a.html").exists(),
        "source deletion must not delete a user's export"
    );
    dir.write("sources/a.md", "# Returned chapter\n");
    wait_contains(&output.join("a.html"), "Returned chapter");
}

#[test]
fn directory_pdf_outputs_have_pdf_names_and_all_inputs_render() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# Alpha\n");
    dir.write("sources/nested/b.md", "# Beta\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(&dir.path("sources"), &output, &["--to", "pdf"]);
    watch.event("\"event\":\"watching\"");
    for path in [output.join("a.pdf"), output.join("nested/b.pdf")] {
        assert!(std::fs::read(path).unwrap().starts_with(b"%PDF"));
    }
    assert!(!output.join("a.html").exists());
}

#[test]
fn a_shared_include_change_rebuilds_each_dependent_document() {
    let dir = Directory::new();
    dir.write("sources/a.md", "# Alpha\n\n{{#include shared.txt}}\n");
    dir.write("sources/b.md", "# Beta\n\n{{#include shared.txt}}\n");
    let included = dir.write("sources/shared.txt", "Shared original paragraph.\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let watch = Watch::start(&dir.path("sources"), &output, &[]);
    watch.event("\"event\":\"watching\"");
    std::fs::write(&included, "Shared replacement paragraph.\n").unwrap();
    for output in [output.join("a.html"), output.join("b.html")] {
        let html = wait_contains(&output, "Shared replacement paragraph.");
        assert!(!html.contains("Shared original paragraph."));
    }
}

#[test]
fn colliding_sources_and_directory_measure_are_refused_before_rendering() {
    let dir = Directory::new();
    dir.write("sources/same.md", "# First\n");
    dir.write("sources/same.markdown", "# Second\n");
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "watch"])
        .arg(dir.path("sources"))
        .arg("--out")
        .arg(&output)
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&result.stderr).contains("same output"));
    assert!(std::fs::read_dir(&output).unwrap().next().is_none());
    let result = Command::new(env!("CARGO_BIN_EXE_fmd"))
        .args(["--no-config", "watch"])
        .arg(dir.path("sources"))
        .arg("--out")
        .arg(&output)
        .args(["--measure", "1"])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(64));
    assert!(String::from_utf8_lossy(&result.stderr).contains("--measure"));
}

const DRAWING: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;

#[test]
fn initial_image_conflicts_preserve_all_inputs_before_any_output_is_written() {
    for source in [
        "![Drawing][art]\n\n[art]: a.svg?view=1#panel\n",
        "<img src=\"a.svg\" alt=\"Drawing\">\n",
        "{{#include fragment.txt}}\n",
    ] {
        let dir = Directory::new();
        let root = dir.path("sources");
        let first = dir.write("sources/a.md", "# Alpha\n");
        let second = dir.write("sources/b.md", source);
        let asset = dir.write("sources/a.svg", DRAWING);
        dir.write("sources/fragment.txt", "![Included drawing](a.svg)\n");
        let mut watch = Watch::start(&root, &root, &["--to", "svg"]);
        watch.assert_refused();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "# Alpha\n");
        assert_eq!(std::fs::read_to_string(&second).unwrap(), source);
        assert_eq!(std::fs::read_to_string(&asset).unwrap(), DRAWING);
        assert!(!root.join("b.svg").exists());

        // Sharing the image is safe with a distinct output directory.
        let output = dir.path("output");
        std::fs::create_dir(&output).unwrap();
        let safe_watch = Watch::start(&root, &output, &["--to", "svg"]);
        safe_watch.event("\"event\":\"watching\"");
        assert!(output.join("a.svg").is_file());
        assert!(output.join("b.svg").is_file());
        assert_eq!(std::fs::read_to_string(asset).unwrap(), DRAWING);
    }
}

#[test]
fn include_inputs_and_both_format_siblings_are_protected_even_when_missing() {
    for (target, include) in [("html", "a.html"), ("both", "a.pdf")] {
        for exists in [false, true] {
            let dir = Directory::new();
            let root = dir.path("sources");
            dir.write("sources/a.md", "# Alpha\n");
            dir.write("sources/b.md", &format!("{{{{#include {include}}}}}\n"));
            let fragment = root.join(include);
            if exists {
                std::fs::write(&fragment, "Owned source fragment.\n").unwrap();
            }
            let mut watch = Watch::start(&root, &root, &["--to", target]);
            watch.assert_refused();
            if exists {
                assert_eq!(
                    std::fs::read_to_string(&fragment).unwrap(),
                    "Owned source fragment.\n"
                );
            } else {
                assert!(
                    !fragment.exists(),
                    "an export must not create a missing include"
                );
            }
            assert!(!root.join("b.html").exists());
            assert!(!root.join("b.pdf").exists());
            if target == "both" {
                assert!(
                    !root.join("a.html").exists(),
                    "reject both siblings before either write"
                );
            }
        }
    }
}

#[test]
fn shared_css_font_and_explicit_image_paths_are_reserved_before_rendering() {
    for option in ["--css", "--pdf-font", "--pdf-image"] {
        let dir = Directory::new();
        let root = dir.path("sources");
        dir.write("sources/a.md", "# Alpha\n");
        dir.write("sources/b.md", "# Beta\n");
        let resource = dir.write("sources/a.html", DRAWING);
        let spec = match option {
            "--pdf-font" => format!("body-regular={}", resource.display()),
            "--pdf-image" => format!("logo={}", resource.display()),
            _ => resource.display().to_string(),
        };
        let mut watch = Watch::start(&root, &root, &[option, &spec]);
        watch.assert_refused();
        assert_eq!(std::fs::read_to_string(resource).unwrap(), DRAWING);
        assert!(!root.join("b.html").exists());
    }
}

#[cfg(unix)]
#[test]
fn aliased_output_directories_and_dangling_image_aliases_cannot_replace_inputs() {
    use std::os::unix::fs::symlink;

    let dir = Directory::new();
    let root = dir.path("sources");
    dir.write("sources/nested/a.md", "# Alpha\n");
    dir.write("sources/b.md", "![Owned drawing](nested/a.svg)\n");
    let asset = dir.write("sources/nested/a.svg", DRAWING);
    let output = dir.path("output");
    std::fs::create_dir(&output).unwrap();
    symlink(root.join("nested"), output.join("nested")).unwrap();
    let mut watch = Watch::start(&root, &output, &["--to", "svg"]);
    watch.assert_refused();
    assert_eq!(std::fs::read_to_string(asset).unwrap(), DRAWING);
    assert!(!output.join("b.svg").exists());

    let dir = Directory::new();
    let root = dir.path("sources");
    dir.write("sources/a.md", "# Alpha\n");
    dir.write("sources/b.md", "![Missing drawing](alias.svg)\n");
    symlink("a.svg", root.join("alias.svg")).unwrap();
    let mut watch = Watch::start(&root, &root, &["--to", "svg"]);
    watch.assert_refused();
    assert!(!root.join("a.svg").exists());
    assert!(!root.join("b.svg").exists());
    assert_eq!(
        std::fs::read_link(root.join("alias.svg")).unwrap(),
        Path::new("a.svg")
    );
}

#[test]
fn a_new_root_cannot_overwrite_an_existing_roots_asset_and_recovers_after_repair() {
    let dir = Directory::new();
    let root = dir.path("sources");
    let source = dir.write("sources/b.md", "# Beta\n\n![Drawing](a.svg)\n");
    let asset = dir.write("sources/a.svg", DRAWING);
    let mut watch = Watch::start(&root, &root, &["--to", "svg"]);
    watch.event("\"event\":\"watching\"");
    dir.write("sources/a.md", "# New alpha chapter\n");
    dir.write("sources/c.md", "# Independent chapter\n");
    watch.event("refusing to overwrite watched input");
    wait_changed(&root.join("c.svg"), &[]);
    assert_eq!(std::fs::read_to_string(&asset).unwrap(), DRAWING);
    assert!(
        watch.child.try_wait().unwrap().is_none(),
        "conflicts leave watch running"
    );

    std::fs::write(source, "# Beta repaired\n").unwrap();
    wait_changed(&asset, DRAWING.as_bytes());
    assert!(watch.child.try_wait().unwrap().is_none());
}

#[test]
fn refresh_conflicts_survive_failed_source_reads_and_only_block_the_unsafe_output() {
    let dir = Directory::new();
    let root = dir.path("sources");
    let first = dir.write("sources/a.md", "# Alpha original\n");
    let second = dir.write("sources/b.md", "# Beta original\n");
    let independent = dir.write("sources/c.md", "# Gamma original\n");
    let mut watch = Watch::start(&root, &root, &["--to", "svg"]);
    watch.event("\"event\":\"watching\"");
    let asset = root.join("a.svg");
    let original = std::fs::read(&asset).unwrap();
    let previous_independent = std::fs::read(root.join("c.svg")).unwrap();

    std::fs::write(&second, "# Beta uses alpha\n\n![Drawing](a.svg)\n").unwrap();
    watch.event(&format!("\"path\":\"{}\"", second.display()));
    std::fs::write(&first, "# Alpha changed\n").unwrap();
    std::fs::write(&independent, "# Gamma changed independently\n").unwrap();
    watch.event("refusing to overwrite watched input");
    wait_changed(&root.join("c.svg"), &previous_independent);
    assert_eq!(std::fs::read(&asset).unwrap(), original);

    // Both unreadable UTF-8 and an unsuccessful include expansion retain the
    // last known image input instead of releasing it for another root to write.
    for bad_source in [
        &[0xff][..],
        b"{{#include missing-fragment.txt}}\n".as_slice(),
    ] {
        std::fs::write(&second, bad_source).unwrap();
        watch.event("\"code\":\"input_error\"");
        assert_eq!(std::fs::read(&asset).unwrap(), original);
        assert!(watch.child.try_wait().unwrap().is_none());
    }

    // The blocked Alpha edit is queued across those failures. Removing the
    // dependency retries it without requiring another edit to Alpha itself.
    std::fs::write(&second, "# Beta repaired\n").unwrap();
    wait_changed(&asset, &original);
    assert!(watch.child.try_wait().unwrap().is_none());
}

#[test]
fn navigation_links_to_generated_sibling_pages_do_not_reserve_those_outputs() {
    let dir = Directory::new();
    let root = dir.path("sources");
    dir.write("sources/a.md", "# Alpha\n\n[Beta](b.html#beta)\n");
    let second = dir.write("sources/b.md", "# Beta\n");
    dir.write("sources/b.html", "Previous export\n");
    let mut watch = Watch::start(&root, &root, &[]);
    watch.event("\"event\":\"watching\"");
    assert!(
        std::fs::read_to_string(root.join("a.html"))
            .unwrap()
            .contains("href=\"b.html#beta\"")
    );
    wait_contains(&root.join("b.html"), "<h1 id=\"beta\"");
    std::fs::write(second, "# Beta revised\n").unwrap();
    wait_contains(&root.join("b.html"), "Beta revised");
    assert!(watch.child.try_wait().unwrap().is_none());
}
